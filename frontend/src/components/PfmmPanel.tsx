import { useEffect, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import {
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
} from "@solana/spl-token";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import {
  findHistory3,
  findPool3,
  findQueue3,
  findTicket3,
  ixAddLiquidity3,
  ixClaim3,
  ixClearBatch3,
  ixInitPool3,
  ixSwapRequest3,
} from "../lib/ix";
import { buildBareTokenAccountIxs } from "../lib/spl";
import { sendTx, explorerTx } from "../lib/tx";
import type { ClusterConfig } from "../lib/programs";
import { truncatePubkey } from "../lib/format";
import {
  AddLiquidityForm,
  ClearClaimButtons,
  InitPoolForm,
  PoolStatus,
  SwapRequestForm,
  type PoolView,
} from "./PfmmControls";

// Pool layout: window_end is at offset 256 (per pfda-amm-3.local.e2e.ts:263).
const POOL_OFFSET_WINDOW_END = 256;

/// Pfmm interaction panel.
///
/// Exact e2e mirror flow: pick 3 mints (order-sensitive — pool PDA is
/// keyed by [mint0, mint1, mint2] in selection order) → InitializePool
/// (with bare vault accounts) → AddLiquidity → SwapRequest →
/// wait for window → ClearBatch → Claim.
///
/// Each button maps to one tx so the user can see progress and so we
/// don't blow the 1232-byte tx-size cap on the larger composites.
export function PfmmPanel({
  selectedMints,
  walletDecimals,
  config,
}: {
  selectedMints: string[];
  walletDecimals: Record<string, number>;
  config: ClusterConfig;
}) {
  const { connection } = useConnection();
  const wallet = useWallet();
  const { publicKey } = wallet;

  const [feeBps, setFeeBps] = useState(30);
  const [windowSlots, setWindowSlots] = useState(40);
  const [liquidityUi, setLiquidityUi] = useState(100);
  const [swapInIdx, setSwapInIdx] = useState(0);
  const [swapOutIdx, setSwapOutIdx] = useState(1);
  const [swapAmountUi, setSwapAmountUi] = useState(1);

  // We persist the pool's vault keypairs locally because the pool
  // doesn't store them in its account data and InitPool / AddLiquidity
  // / Claim all need them. In production they'd be deterministic ATAs
  // owned by the pool PDA — for the demo we keep a Map<poolPk → vaults>.
  const [vaultsByPool, setVaultsByPool] = useState<
    Record<string, [string, string, string]>
  >({});

  const [pool, setPool] = useState<PoolView | null>(null);
  const [stage, setStage] = useState<string>("idle");
  const [log, setLog] = useState<string[]>([]);
  const [currentSlot, setCurrentSlot] = useState<bigint | null>(null);
  const pfmm = config.programs.find((p) => p.name === "pfda-amm-3")!.address;

  function pushLog(s: string) {
    setLog((l) => [...l, s]);
  }

  // Refresh pool view + slot whenever the 3-mint selection changes.
  useEffect(() => {
    if (selectedMints.length !== 3 || !publicKey) {
      setPool(null);
      return;
    }
    const m = selectedMints.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
    const [poolPk] = findPool3(pfmm, m[0], m[1], m[2]);
    let cancelled = false;
    void (async () => {
      const info = await connection.getAccountInfo(poolPk);
      if (cancelled) return;
      if (!info) {
        setPool({ exists: false, pool: poolPk });
      } else {
        const data = info.data;
        const windowEnd = data.readBigUInt64LE(POOL_OFFSET_WINDOW_END);
        // Pool layout offset for `current_batch_id` is not asserted in the
        // e2e suite; we display windowEnd only and infer batch progression
        // from queue3 PDAs.
        setPool({ exists: true, pool: poolPk, windowEnd });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedMints.join(","), publicKey?.toBase58(), connection, pfmm.toBase58()]); // eslint-disable-line react-hooks/exhaustive-deps

  // Slot poller for the window-end countdown.
  useEffect(() => {
    let cancelled = false;
    const tick = () =>
      void connection.getSlot("confirmed").then((s) => {
        if (!cancelled) setCurrentSlot(BigInt(s));
      });
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [connection]);

  function dec(mint: string, fallback = 6): number {
    return walletDecimals[mint] ?? fallback;
  }

  function uiToBase(ui: number, mint: string): bigint {
    return BigInt(Math.round(ui * 10 ** dec(mint)));
  }

  async function initPool() {
    if (!publicKey || selectedMints.length !== 3) return;
    setStage("init");
    setLog([]);
    try {
      const m = selectedMints.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
      const [poolPk] = findPool3(pfmm, m[0], m[1], m[2]);
      const [queue0] = findQueue3(pfmm, poolPk, 0n);
      const vaults = await buildBareTokenAccountIxs(connection, publicKey, 3);
      pushLog(`Pool: ${poolPk.toBase58()}`);
      pushLog(`Queue0: ${queue0.toBase58()}`);
      pushLog(
        `Vaults: ${vaults.pubkeys.map((v) => truncatePubkey(v.toBase58())).join(", ")}`,
      );

      const initIx = ixInitPool3({
        programId: pfmm,
        payer: publicKey,
        pool: poolPk,
        queue: queue0,
        mints: m,
        vaults: vaults.pubkeys as [PublicKey, PublicKey, PublicKey],
        treasury: publicKey, // treasury = self for the demo
        feeBps,
        windowSlots: BigInt(windowSlots),
        weights: [333_333, 333_333, 333_334],
      });
      const sig = await sendTx(
        connection,
        wallet,
        [...vaults.ixs, initIx],
        vaults.signers,
      );
      pushLog(`✓ init_pool: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`);
      // Persist vault pubkeys for follow-up txs in this session.
      setVaultsByPool((cur) => ({
        ...cur,
        [poolPk.toBase58()]: vaults.pubkeys.map((v) => v.toBase58()) as [
          string,
          string,
          string,
        ],
      }));
      // Force pool refresh
      const info = await connection.getAccountInfo(poolPk);
      if (info) {
        const windowEnd = info.data.readBigUInt64LE(POOL_OFFSET_WINDOW_END);
        setPool({ exists: true, pool: poolPk, windowEnd });
      }
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  function getVaults(poolPk: PublicKey): [PublicKey, PublicKey, PublicKey] | null {
    const stored = vaultsByPool[poolPk.toBase58()];
    if (!stored) return null;
    return stored.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
  }

  async function addLiquidity() {
    if (!publicKey || !pool?.exists) return;
    const vaults = getVaults(pool.pool);
    if (!vaults) {
      pushLog("✗ vault pubkeys missing — InitPool from this session, or paste them");
      return;
    }
    setStage("addLiq");
    try {
      const m = selectedMints.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
      const userTokens = m.map((mint) =>
        getAssociatedTokenAddressSync(mint, publicKey),
      ) as [PublicKey, PublicKey, PublicKey];
      const amounts = selectedMints.map((mintStr) =>
        uiToBase(liquidityUi, mintStr),
      ) as [bigint, bigint, bigint];
      const ix = ixAddLiquidity3({
        programId: pfmm,
        payer: publicKey,
        pool: pool.pool,
        vaults,
        userTokens,
        amounts,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(`✓ add_liquidity: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function swapRequest() {
    if (!publicKey || !pool?.exists) return;
    const vaults = getVaults(pool.pool);
    if (!vaults) {
      pushLog("✗ vault pubkeys missing");
      return;
    }
    if (swapInIdx === swapOutIdx) {
      pushLog("✗ in_idx == out_idx");
      return;
    }
    setStage("swap");
    try {
      // Find current batch id by walking queue3 PDAs. For the demo,
      // we keep a small counter — real apps would read it from pool state.
      let batchId = 0n;
      for (let i = 0; i < 16; i++) {
        const [q] = findQueue3(pfmm, pool.pool, BigInt(i));
        const info = await connection.getAccountInfo(q);
        if (info) {
          batchId = BigInt(i);
          break;
        }
      }
      pushLog(`Active batch detected: ${batchId}`);
      const [queue] = findQueue3(pfmm, pool.pool, batchId);
      const [ticket] = findTicket3(pfmm, pool.pool, publicKey, batchId);
      const inMint = new PublicKey(selectedMints[swapInIdx]);
      const userTokenIn = getAssociatedTokenAddressSync(inMint, publicKey);
      const ix = ixSwapRequest3({
        programId: pfmm,
        user: publicKey,
        pool: pool.pool,
        queue,
        ticket,
        userTokenIn,
        vaultIn: vaults[swapInIdx],
        inIdx: swapInIdx,
        outIdx: swapOutIdx,
        amountIn: uiToBase(swapAmountUi, selectedMints[swapInIdx]),
        minOut: 0n,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ swap_request batch=${batchId} ${swapInIdx}→${swapOutIdx}: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function clearBatch() {
    if (!publicKey || !pool?.exists) return;
    setStage("clear");
    try {
      // Detect active batch — same scan as swap.
      let batchId = 0n;
      for (let i = 0; i < 16; i++) {
        const [q] = findQueue3(pfmm, pool.pool, BigInt(i));
        const info = await connection.getAccountInfo(q);
        if (info) {
          batchId = BigInt(i);
          break;
        }
      }
      const [queue] = findQueue3(pfmm, pool.pool, batchId);
      const [history] = findHistory3(pfmm, pool.pool, batchId);
      const [nextQueue] = findQueue3(pfmm, pool.pool, batchId + 1n);
      const ix = ixClearBatch3({
        programId: pfmm,
        cranker: publicKey,
        pool: pool.pool,
        queue,
        history,
        nextQueue,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ clear_batch batch=${batchId}: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function claim() {
    if (!publicKey || !pool?.exists) return;
    const vaults = getVaults(pool.pool);
    if (!vaults) {
      pushLog("✗ vault pubkeys missing");
      return;
    }
    setStage("claim");
    try {
      // Claim against the most recently CLEARED batch — that's batchId-1 of
      // the active queue. Walk: find first non-existent queue, claim previous.
      let batchId = 0n;
      for (let i = 0; i < 16; i++) {
        const [q] = findQueue3(pfmm, pool.pool, BigInt(i));
        const info = await connection.getAccountInfo(q);
        if (!info && i > 0) {
          batchId = BigInt(i - 1);
          break;
        }
        if (info) batchId = BigInt(i);
      }
      // Heuristic: the "claim batch" is the batch we already cleared, i.e.
      // the active batch - 1, or 0 if only batch 0 exists post-clear.
      // In practice the user will call clearBatch then claim immediately.
      const claimBatch = batchId > 0n ? batchId - 1n : 0n;
      const [history] = findHistory3(pfmm, pool.pool, claimBatch);
      const [ticket] = findTicket3(pfmm, pool.pool, publicKey, claimBatch);
      const m = selectedMints.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
      const userTokens = m.map((mint) =>
        getAssociatedTokenAddressSync(mint, publicKey),
      ) as [PublicKey, PublicKey, PublicKey];
      // Make sure ATAs exist (idempotent)
      const ataIxs = m.map((mint, i) =>
        createAssociatedTokenAccountIdempotentInstruction(
          publicKey,
          userTokens[i],
          publicKey,
          mint,
        ),
      );
      const ix = ixClaim3({
        programId: pfmm,
        user: publicKey,
        pool: pool.pool,
        history,
        ticket,
        vaults,
        userTokens,
      });
      const sig = await sendTx(connection, wallet, [...ataIxs, ix]);
      pushLog(
        `✓ claim batch=${claimBatch}: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  const can3 = selectedMints.length === 3;
  const windowOpen =
    pool?.windowEnd && currentSlot ? currentSlot < pool.windowEnd : false;
  const slotsLeft =
    pool?.windowEnd && currentSlot && currentSlot < pool.windowEnd
      ? Number(pool.windowEnd - currentSlot)
      : 0;

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-lg font-semibold">PFMM (pfda-amm-3)</h2>
        <span className="rounded-full bg-slate-800 px-2 py-0.5 font-mono text-[10px] text-slate-400">
          {truncatePubkey(pfmm.toBase58(), 6, 6)}
        </span>
      </header>

      {!publicKey ? (
        <p className="text-sm text-slate-400">Connect a wallet first.</p>
      ) : !can3 ? (
        <p className="text-sm text-slate-400">
          Pick exactly 3 tokens from the Tokens panel (order matters — the pool
          PDA is keyed by mint0/mint1/mint2 in selection order).
        </p>
      ) : (
        <div className="space-y-4">
          <PoolStatus
            pool={pool}
            currentSlot={currentSlot}
            windowOpen={windowOpen}
            slotsLeft={slotsLeft}
            explorerCluster={config.explorerCluster}
          />

          {!pool?.exists && (
            <InitPoolForm
              feeBps={feeBps}
              setFeeBps={setFeeBps}
              windowSlots={windowSlots}
              setWindowSlots={setWindowSlots}
              initPool={initPool}
              stage={stage}
            />
          )}

          {pool?.exists && (
            <>
              <AddLiquidityForm
                liquidityUi={liquidityUi}
                setLiquidityUi={setLiquidityUi}
                addLiquidity={addLiquidity}
                disabled={stage !== "idle" || !getVaults(pool.pool)}
                stage={stage}
              />
              <SwapRequestForm
                swapInIdx={swapInIdx}
                setSwapInIdx={setSwapInIdx}
                swapOutIdx={swapOutIdx}
                setSwapOutIdx={setSwapOutIdx}
                swapAmountUi={swapAmountUi}
                setSwapAmountUi={setSwapAmountUi}
                swapRequest={swapRequest}
                stage={stage}
              />
              <ClearClaimButtons
                clearBatch={clearBatch}
                claim={claim}
                windowOpen={windowOpen}
                stage={stage}
              />
            </>
          )}

          {log.length > 0 && (
            <pre className="max-h-64 overflow-auto rounded bg-slate-950/80 p-3 text-[11px] text-slate-300">
              {log.join("\n")}
            </pre>
          )}
        </div>
      )}
    </section>
  );
}

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
  ixCloseBatchHistory3,
  ixCloseExpiredTicket3,
  ixInitPool3,
  ixSetPaused3,
  ixSwapRequest3,
  ixWithdrawFees3,
} from "../lib/ix";
import { buildBareTokenAccountIxs } from "../lib/spl";
import { sendTx, sendVersionedTx, explorerTx } from "../lib/tx";
import type { ClusterConfig } from "../lib/programs";
import { truncatePubkey } from "../lib/format";
import {
  fetchHistory3,
  fetchPoolState3,
  fetchTicket3,
  type PoolState3Data,
} from "../lib/pfmmState";
import { buildJupiterSolSeedPlan } from "../lib/pfmmSeedPlan";
import {
  AddLiquidityForm,
  ClearClaimButtons,
  CrankerForm,
  InitPoolForm,
  JupiterSolSeedForm,
  PausedToggle,
  PoolStatus,
  SwapRequestForm,
  WithdrawFeesForm,
  type PoolView,
} from "./PfmmControls";

const CLOSE_HISTORY_DELAY = 100n;
const TICKET_EXPIRY_BATCHES = 200n;

/// Pfmm interaction panel.
///
/// Mirrors the e2e flow: pick 3 mints (order-sensitive — pool PDA is
/// keyed by [mint0, mint1, mint2] in selection order) → InitializePool
/// (with bare vault accounts) → AddLiquidity → SwapRequest →
/// wait for window → ClearBatch → Claim.
///
/// Vaults and batch ids are read from on-chain pool state once it
/// exists, so the panel survives a page reload (an earlier demo build
/// kept them only in React state). The vault keypairs we generate inside
/// InitPool are used only as part of that one tx; subsequent reads come
/// straight from `PoolState3` at offsets we mirror in `lib/pfmmState.ts`.
///
/// On mainnet, two extra Jupiter SOL-seed flows are exposed so users
/// arriving with only SOL can:
///   • Fund AddLiquidity by buying the 3 basket tokens proportionally
///     to the pool's weights, then call AddLiquidity.
///   • Fund a SwapRequest by buying just the input-side token, then
///     call SwapRequest.
/// Each is a separate tx the wallet signs in sequence — split rather
/// than bundled because Jupiter swap legs alone routinely flirt with
/// the 1232-byte wire cap once a PFMM ix is added.
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
  const [feeAmount0Ui, setFeeAmount0Ui] = useState(0);
  const [feeAmount1Ui, setFeeAmount1Ui] = useState(0);
  const [feeAmount2Ui, setFeeAmount2Ui] = useState(0);

  // Jupiter seed UI state — both flows share slippage but keep their
  // SOL inputs separate so the user doesn't have to re-enter when
  // toggling between AddLiquidity and SwapRequest.
  const [seedSolForLiq, setSeedSolForLiq] = useState(0.05);
  const [seedSolForSwap, setSeedSolForSwap] = useState(0.01);
  const [jupiterSlippageBps, setJupiterSlippageBps] = useState(50);

  // Cranker UI state — user pastes a batch_id (or owner pubkey for
  // tickets) so we can re-derive the PDA and read the on-chain state
  // before sending. Trying to enumerate all batches client-side here
  // would burn RPC budget; targeted lookups are cheaper.
  const [closeHistoryBatchId, setCloseHistoryBatchId] = useState("0");
  const [closeTicketBatchId, setCloseTicketBatchId] = useState("0");
  const [closeTicketOwner, setCloseTicketOwner] = useState("");

  // Vault keypairs for the in-flight InitPool tx. Cleared after the
  // pool fetch returns the on-chain values.
  const [pendingInitVaults, setPendingInitVaults] = useState<
    [PublicKey, PublicKey, PublicKey] | null
  >(null);

  const [pool, setPool] = useState<PoolState3Data | null>(null);
  const [poolMissing, setPoolMissing] = useState<boolean>(false);
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
      setPoolMissing(false);
      return;
    }
    const m = selectedMints.map((s) => new PublicKey(s)) as [PublicKey, PublicKey, PublicKey];
    const [poolPk] = findPool3(pfmm, m[0], m[1], m[2]);
    let cancelled = false;
    void (async () => {
      try {
        const data = await fetchPoolState3(connection, poolPk);
        if (cancelled) return;
        if (data) {
          setPool(data);
          setPoolMissing(false);
        } else {
          setPool(null);
          setPoolMissing(true);
        }
      } catch (e) {
        if (cancelled) return;
        setPool(null);
        setPoolMissing(true);
        pushLog(
          `pool fetch failed: ${e instanceof Error ? e.message : String(e)}`,
        );
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

  async function refreshPool(poolPk: PublicKey) {
    const data = await fetchPoolState3(connection, poolPk);
    if (data) {
      setPool(data);
      setPoolMissing(false);
      setPendingInitVaults(null);
    }
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
      setPendingInitVaults(vaults.pubkeys as [PublicKey, PublicKey, PublicKey]);

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
      await refreshPool(poolPk);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  /// Resolve the vaults to use for an op. Pool-on-chain wins; falls back
  /// to the freshly-allocated keypairs from a just-issued InitPool tx
  /// that hasn't surfaced in our pool state read yet.
  function getVaults(): [PublicKey, PublicKey, PublicKey] | null {
    if (pool) return pool.vaults;
    return pendingInitVaults;
  }

  async function addLiquidity() {
    if (!publicKey || !pool) return;
    setStage("addLiq");
    try {
      const m = pool.tokenMints;
      const userTokens = m.map((mint) =>
        getAssociatedTokenAddressSync(mint, publicKey),
      ) as [PublicKey, PublicKey, PublicKey];
      const amounts = pool.tokenMints.map((mint) =>
        uiToBase(liquidityUi, mint.toBase58()),
      ) as [bigint, bigint, bigint];
      const ix = ixAddLiquidity3({
        programId: pfmm,
        payer: publicKey,
        pool: pool.pool,
        vaults: pool.vaults,
        userTokens,
        amounts,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ add_liquidity: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      await refreshPool(pool.pool);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  /// Buy the 3 basket tokens with SOL, weighted to match the pool's
  /// on-chain `weights`. The bps used here come straight from the pool
  /// (converted from the program's u32 micro-units), so subsequent
  /// AddLiquidity can be executed with whatever balances actually land.
  async function seedFromSolForAddLiquidity() {
    if (!publicKey || !pool) return;
    setStage("seedLiq");
    try {
      const microSum = pool.weights.reduce((a, b) => a + b, 0);
      if (microSum === 0) {
        throw new Error("pool weights sum to zero — corrupt state?");
      }
      // Convert micro-units (sum 1_000_000) → bps (sum 10_000) and pin
      // any rounding drift to the last leg so the bps still sum exactly.
      const partial = pool.weights
        .slice(0, -1)
        .map((w) => Math.floor((w * 10_000) / microSum));
      const last = 10_000 - partial.reduce((a, b) => a + b, 0);
      const bpsWeights = [...partial, last];
      const solIn = BigInt(Math.floor(seedSolForLiq * 1_000_000_000));
      pushLog(
        `Jupiter SOL → basket: ${seedSolForLiq} SOL split ${bpsWeights.join("/")} bps; slippage ${jupiterSlippageBps} bps`,
      );
      const plan = await buildJupiterSolSeedPlan({
        conn: connection,
        user: publicKey,
        outputMints: [...pool.tokenMints],
        weights: bpsWeights,
        solIn,
        slippageBps: jupiterSlippageBps,
      });
      pushLog(
        `tx ${plan.txBytes}/1232 b · ${plan.ixCount} ix · CU ${plan.computeUnitLimit} @ ${plan.computeUnitPrice} μL/CU`,
      );
      for (const leg of plan.legs) {
        pushLog(
          `  ${truncatePubkey(leg.mint.toBase58(), 4, 4)}: ${(Number(leg.solLamports) / 1e9).toFixed(6)} SOL → ${leg.expectedOut.toString()} (min ${leg.minOut.toString()}) · ${leg.routeLabel}`,
        );
      }
      const sig = await sendVersionedTx(connection, wallet, plan.versionedTx);
      pushLog(
        `✓ jupiter_seed (basket): ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function swapRequest() {
    if (!publicKey || !pool) return;
    if (swapInIdx === swapOutIdx) {
      pushLog("✗ in_idx == out_idx");
      return;
    }
    setStage("swap");
    try {
      const batchId = pool.currentBatchId;
      pushLog(`Active batch: ${batchId}`);
      const [queue] = findQueue3(pfmm, pool.pool, batchId);
      const [ticket] = findTicket3(pfmm, pool.pool, publicKey, batchId);
      const inMint = pool.tokenMints[swapInIdx];
      const userTokenIn = getAssociatedTokenAddressSync(inMint, publicKey);
      const ix = ixSwapRequest3({
        programId: pfmm,
        user: publicKey,
        pool: pool.pool,
        queue,
        ticket,
        userTokenIn,
        vaultIn: pool.vaults[swapInIdx],
        inIdx: swapInIdx,
        outIdx: swapOutIdx,
        amountIn: uiToBase(swapAmountUi, inMint.toBase58()),
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

  /// Buy the swap-input mint with SOL via Jupiter. After it lands, the
  /// user can run SwapRequest with their fresh balance. Two txs (Jupiter
  /// swap, then PFMM SwapRequest) — bundling is unsafe because Jupiter's
  /// quoted output isn't known at SwapRequest build time without a
  /// post-confirmation balance read.
  async function seedFromSolForSwap() {
    if (!publicKey || !pool) return;
    setStage("seedSwap");
    try {
      const inMint = pool.tokenMints[swapInIdx];
      const solIn = BigInt(Math.floor(seedSolForSwap * 1_000_000_000));
      pushLog(
        `Jupiter SOL → ${truncatePubkey(inMint.toBase58(), 4, 4)} (input idx ${swapInIdx}): ${seedSolForSwap} SOL; slippage ${jupiterSlippageBps} bps`,
      );
      const plan = await buildJupiterSolSeedPlan({
        conn: connection,
        user: publicKey,
        outputMints: [inMint],
        weights: [10_000],
        solIn,
        slippageBps: jupiterSlippageBps,
      });
      pushLog(
        `tx ${plan.txBytes}/1232 b · ${plan.ixCount} ix · CU ${plan.computeUnitLimit} @ ${plan.computeUnitPrice} μL/CU`,
      );
      const leg = plan.legs[0];
      pushLog(
        `  expected out: ${leg.expectedOut.toString()} (min ${leg.minOut.toString()}) · ${leg.routeLabel}`,
      );
      const sig = await sendVersionedTx(connection, wallet, plan.versionedTx);
      pushLog(
        `✓ jupiter_seed (swap input): ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function clearBatch() {
    if (!publicKey || !pool) return;
    setStage("clear");
    try {
      const batchId = pool.currentBatchId;
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
      await refreshPool(pool.pool);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function withdrawFees() {
    if (!publicKey || !pool) return;
    setStage("withdrawFees");
    try {
      const treasury = pool.treasury;
      const m = pool.tokenMints;
      const treasuryTokens = m.map((mint) =>
        getAssociatedTokenAddressSync(mint, treasury, true),
      ) as [PublicKey, PublicKey, PublicKey];
      // Idempotent ATA creates so the on-chain Transfer doesn't fail on
      // a missing destination if the treasury hasn't been touched yet.
      const ataIxs = m.map((mint, i) =>
        createAssociatedTokenAccountIdempotentInstruction(
          publicKey,
          treasuryTokens[i],
          treasury,
          mint,
        ),
      );
      const amounts = [
        uiToBase(feeAmount0Ui, pool.tokenMints[0].toBase58()),
        uiToBase(feeAmount1Ui, pool.tokenMints[1].toBase58()),
        uiToBase(feeAmount2Ui, pool.tokenMints[2].toBase58()),
      ] as [bigint, bigint, bigint];
      const ix = ixWithdrawFees3({
        programId: pfmm,
        authority: publicKey,
        pool: pool.pool,
        vaults: pool.vaults,
        treasuryTokens,
        amounts,
      });
      const sig = await sendTx(connection, wallet, [...ataIxs, ix]);
      pushLog(
        `✓ withdraw_fees [${amounts.join(", ")}]: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      await refreshPool(pool.pool);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function closeBatchHistory() {
    if (!publicKey || !pool) return;
    let batchId: bigint;
    try {
      batchId = BigInt(closeHistoryBatchId);
    } catch {
      pushLog("✗ history batch_id must be a non-negative integer");
      return;
    }
    if (!isAuthority) {
      pushLog(
        "✗ CloseBatchHistory rent_recipient must be the pool authority — connect the authority wallet first.",
      );
      return;
    }
    if (pool.currentBatchId < batchId + CLOSE_HISTORY_DELAY) {
      pushLog(
        `✗ history batch ${batchId} not yet eligible — needs current_batch_id ≥ ${batchId + CLOSE_HISTORY_DELAY} (have ${pool.currentBatchId}).`,
      );
      return;
    }
    setStage("closeHistory");
    try {
      const [history] = findHistory3(pfmm, pool.pool, batchId);
      const existing = await fetchHistory3(connection, history);
      if (!existing) {
        pushLog(`✗ no history account at ${history.toBase58()} (batch ${batchId})`);
        setStage("idle");
        return;
      }
      const ix = ixCloseBatchHistory3({
        programId: pfmm,
        rentRecipient: publicKey,
        pool: pool.pool,
        history,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ close_batch_history batch=${batchId}: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function closeExpiredTicket() {
    if (!publicKey || !pool) return;
    let batchId: bigint;
    try {
      batchId = BigInt(closeTicketBatchId);
    } catch {
      pushLog("✗ ticket batch_id must be a non-negative integer");
      return;
    }
    if (pool.currentBatchId < batchId + TICKET_EXPIRY_BATCHES) {
      pushLog(
        `✗ ticket batch ${batchId} not yet expired — needs current_batch_id ≥ ${batchId + TICKET_EXPIRY_BATCHES} (have ${pool.currentBatchId}).`,
      );
      return;
    }
    setStage("closeTicket");
    try {
      let ownerPk: PublicKey;
      try {
        // Default to caller's wallet if owner field is left blank.
        ownerPk = closeTicketOwner.trim().length === 0
          ? publicKey
          : new PublicKey(closeTicketOwner.trim());
      } catch {
        pushLog("✗ ticket owner must be a base58 pubkey (or blank for self)");
        setStage("idle");
        return;
      }
      const [ticket] = findTicket3(pfmm, pool.pool, ownerPk, batchId);
      const existing = await fetchTicket3(connection, ticket);
      if (!existing) {
        pushLog(`✗ no ticket account at ${ticket.toBase58()} (owner ${truncatePubkey(ownerPk.toBase58(), 4, 4)}, batch ${batchId})`);
        setStage("idle");
        return;
      }
      const ix = ixCloseExpiredTicket3({
        programId: pfmm,
        caller: publicKey,
        pool: pool.pool,
        ticket,
        rentRecipient: ownerPk,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ close_expired_ticket owner=${truncatePubkey(ownerPk.toBase58(), 4, 4)} batch=${batchId}: ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function setPaused(paused: boolean) {
    if (!publicKey || !pool) return;
    setStage(paused ? "pause" : "unpause");
    try {
      const ix = ixSetPaused3({
        programId: pfmm,
        authority: publicKey,
        pool: pool.pool,
        paused,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(
        `✓ set_paused(${paused}): ${sig.slice(0, 12)}…  → ${explorerTx(sig, config.explorerCluster)}`,
      );
      await refreshPool(pool.pool);
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function claim() {
    if (!publicKey || !pool) return;
    setStage("claim");
    try {
      const batchId = pool.currentBatchId;
      // Claim against the most recently CLEARED batch — that's batchId-1
      // of the active queue, or 0 if we're still on batch 0.
      const claimBatch = batchId > 0n ? batchId - 1n : 0n;
      const [history] = findHistory3(pfmm, pool.pool, claimBatch);
      const [ticket] = findTicket3(pfmm, pool.pool, publicKey, claimBatch);
      const m = pool.tokenMints;
      const userTokens = m.map((mint) =>
        getAssociatedTokenAddressSync(mint, publicKey),
      ) as [PublicKey, PublicKey, PublicKey];
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
        vaults: pool.vaults,
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
    pool && currentSlot ? currentSlot < pool.currentWindowEnd : false;
  const slotsLeft =
    pool && currentSlot && currentSlot < pool.currentWindowEnd
      ? Number(pool.currentWindowEnd - currentSlot)
      : 0;

  // PoolView shape that PfmmControls.PoolStatus expects.
  const poolView: PoolView | null = pool
    ? {
        exists: true,
        pool: pool.pool,
        windowEnd: pool.currentWindowEnd,
      }
    : poolMissing && can3 && publicKey
      ? (() => {
          const m = selectedMints.map(
            (s) => new PublicKey(s),
          ) as [PublicKey, PublicKey, PublicKey];
          return { exists: false, pool: findPool3(pfmm, m[0], m[1], m[2])[0] };
        })()
      : null;

  const isAuthority =
    publicKey !== null && pool !== null && pool.authority.equals(publicKey);

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
            pool={poolView}
            currentSlot={currentSlot}
            windowOpen={windowOpen}
            slotsLeft={slotsLeft}
            explorerCluster={config.explorerCluster}
          />

          {pool && (
            <div className="rounded bg-slate-950/40 p-3 text-[11px] text-slate-400">
              <p>
                Authority{" "}
                <span className="font-mono text-slate-300">
                  {truncatePubkey(pool.authority.toBase58(), 6, 6)}
                </span>
                {isAuthority && (
                  <span className="ml-2 rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-300">
                    you
                  </span>
                )}
                {" · "}fee {pool.baseFeeBps} bps · weights{" "}
                {pool.weights.join("/")} · paused {String(pool.paused)}
              </p>
              <p>
                Reserves{" "}
                <span className="font-mono text-slate-300">
                  {pool.reserves.map((r) => r.toString()).join(" / ")}
                </span>
              </p>
            </div>
          )}

          {!pool && poolMissing && (
            <InitPoolForm
              feeBps={feeBps}
              setFeeBps={setFeeBps}
              windowSlots={windowSlots}
              setWindowSlots={setWindowSlots}
              initPool={initPool}
              stage={stage}
            />
          )}

          {pool && (
            <>
              {config.jupiterEnabled && (
                <JupiterSolSeedForm
                  title="Jupiter SOL → basket (seed AddLiquidity)"
                  hint="Buys the 3 basket tokens with SOL using the pool's on-chain weights, then lands them in your basket ATAs. Run AddLiquidity afterwards."
                  solAmount={seedSolForLiq}
                  setSolAmount={setSeedSolForLiq}
                  slippageBps={jupiterSlippageBps}
                  setSlippageBps={setJupiterSlippageBps}
                  onRun={seedFromSolForAddLiquidity}
                  runLabel="Buy basket"
                  busy={stage === "seedLiq"}
                  disabled={stage !== "idle"}
                />
              )}
              <AddLiquidityForm
                liquidityUi={liquidityUi}
                setLiquidityUi={setLiquidityUi}
                addLiquidity={addLiquidity}
                disabled={stage !== "idle" || !getVaults()}
                stage={stage}
              />
              {config.jupiterEnabled && (
                <JupiterSolSeedForm
                  title={`Jupiter SOL → input mint (idx ${swapInIdx})`}
                  hint="Buys just the swap-input token with SOL. Run SwapRequest afterwards with the resulting balance."
                  solAmount={seedSolForSwap}
                  setSolAmount={setSeedSolForSwap}
                  slippageBps={jupiterSlippageBps}
                  setSlippageBps={setJupiterSlippageBps}
                  onRun={seedFromSolForSwap}
                  runLabel="Buy input mint"
                  busy={stage === "seedSwap"}
                  disabled={stage !== "idle"}
                />
              )}
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
              <CrankerForm
                currentBatchId={pool.currentBatchId}
                isAuthority={isAuthority}
                closeHistoryBatchId={closeHistoryBatchId}
                setCloseHistoryBatchId={setCloseHistoryBatchId}
                closeBatchHistory={closeBatchHistory}
                closeTicketBatchId={closeTicketBatchId}
                setCloseTicketBatchId={setCloseTicketBatchId}
                closeTicketOwner={closeTicketOwner}
                setCloseTicketOwner={setCloseTicketOwner}
                closeExpiredTicket={closeExpiredTicket}
                stage={stage}
              />
              {isAuthority && (
                <>
                  <WithdrawFeesForm
                    amount0={feeAmount0Ui}
                    setAmount0={setFeeAmount0Ui}
                    amount1={feeAmount1Ui}
                    setAmount1={setFeeAmount1Ui}
                    amount2={feeAmount2Ui}
                    setAmount2={setFeeAmount2Ui}
                    withdrawFees={withdrawFees}
                    stage={stage}
                    disabled={false}
                  />
                  <PausedToggle
                    paused={pool.paused}
                    setPaused={setPaused}
                    stage={stage}
                  />
                </>
              )}
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

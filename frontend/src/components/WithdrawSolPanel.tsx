import { useEffect, useMemo, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import {
  buildWithdrawSolPlan,
  type WithdrawSolPlan,
} from "../lib/withdrawSolPlan";
import { fetchEtfState, type EtfStateData } from "../lib/etfState";
import { sendVersionedTx, explorerTx, explorerAddr } from "../lib/tx";
import { truncatePubkey } from "../lib/format";
import type { ClusterConfig } from "../lib/programs";

const ETF_DECIMALS = 6;

export function WithdrawSolPanel({ config }: { config: ClusterConfig }) {
  const { connection } = useConnection();
  const wallet = useWallet();
  const { publicKey } = wallet;
  const axisVault = useMemo(
    () => config.programs.find((p) => p.name === "axis-vault")!.address,
    [config],
  );

  const [etfStateAddr, setEtfStateAddr] = useState("");
  const [etf, setEtf] = useState<EtfStateData | null>(null);
  const [etfLoadErr, setEtfLoadErr] = useState<string | null>(null);
  const [userEtfBalance, setUserEtfBalance] = useState<bigint | null>(null);

  const [burnUi, setBurnUi] = useState<string>("0.5");
  const [slippageBps, setSlippageBps] = useState<number>(50);
  const [safetyShrinkBps, setSafetyShrinkBps] = useState<number>(100);

  const [plan, setPlan] = useState<WithdrawSolPlan | null>(null);
  const [stage, setStage] = useState<"idle" | "loading" | "preview" | "send" | "ok" | "err">(
    "idle",
  );
  const [log, setLog] = useState<string[]>([]);

  function pushLog(line: string) {
    setLog((l) => [...l, line]);
  }

  async function loadEtfState() {
    setEtf(null);
    setEtfLoadErr(null);
    setUserEtfBalance(null);
    setPlan(null);
    if (!etfStateAddr) return;
    try {
      const pubkey = new PublicKey(etfStateAddr);
      setStage("loading");
      const data = await fetchEtfState(connection, pubkey);
      setEtf(data);
      setStage("idle");
      // Best-effort balance read; if user doesn't have an ATA we treat
      // the balance as zero rather than surfacing the RPC error.
      if (publicKey) {
        const userAta = getAssociatedTokenAddressSync(data.etfMint, publicKey, false);
        try {
          const bal = await connection.getTokenAccountBalance(userAta, "confirmed");
          setUserEtfBalance(BigInt(bal.value.amount));
        } catch {
          setUserEtfBalance(0n);
        }
      }
    } catch (e) {
      setEtfLoadErr(e instanceof Error ? e.message : String(e));
      setStage("err");
    }
  }

  // Auto-load on paste once it's a valid base58 length. Avoids a load
  // button on the happy path while still tolerating partial paste.
  useEffect(() => {
    if (!etfStateAddr) return;
    if (etfStateAddr.length < 32 || etfStateAddr.length > 44) return;
    void loadEtfState();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [etfStateAddr, publicKey?.toBase58()]);

  const burnAmountBase = useMemo(() => {
    const n = Number(burnUi);
    if (!Number.isFinite(n) || n <= 0) return 0n;
    return BigInt(Math.floor(n * 10 ** ETF_DECIMALS));
  }, [burnUi]);

  const burnExceedsBalance =
    userEtfBalance !== null && burnAmountBase > userEtfBalance;
  const burnExceedsSupply =
    etf !== null && burnAmountBase > etf.totalSupply;
  const canPreview =
    !!publicKey &&
    !!etf &&
    burnAmountBase > 0n &&
    !burnExceedsBalance &&
    !burnExceedsSupply &&
    !etf.paused &&
    stage !== "loading" &&
    stage !== "preview" &&
    stage !== "send";

  async function previewPlan() {
    if (!publicKey || !etf) return;
    setStage("preview");
    setLog([]);
    setPlan(null);
    try {
      const next = await buildWithdrawSolPlan({
        conn: connection,
        user: publicKey,
        programId: axisVault,
        etfState: new PublicKey(etfStateAddr),
        etfStateData: etf,
        burnAmount: burnAmountBase,
        slippageBps,
        safetyShrinkBps,
      });
      setPlan(next);
      pushLog(
        `preview: ${next.legs.length} legs, expected ${(
          Number(next.expectedSolOut) / 1e9
        ).toFixed(6)} SOL, min ${(Number(next.minSolOut) / 1e9).toFixed(6)} SOL`,
      );
      pushLog(
        `mode=${next.mode}; tx ${next.txBytes}/1232 bytes; CU ${next.computeUnitLimit}; priority ${next.computeUnitPrice} μL/CU`,
      );
      if (next.mode === "split") {
        pushLog(
          "split mode: tx0 = axis-vault Withdraw (basket → wallet); tx1 = Jupiter swaps + close wSOL (basket → SOL). " +
            "Wallet will sign twice; if you abort between, basket tokens stay in your wallet.",
        );
      }
      for (const leg of next.legs) {
        pushLog(
          `  ${truncatePubkey(leg.mint.toBase58(), 4, 4)}: basket ${leg.expectedBasketOut.toString()} → ${(
            Number(leg.expectedSolOut) / 1e9
          ).toFixed(6)} SOL · ${leg.routeLabel}`,
        );
      }
      setStage("idle");
    } catch (e) {
      setStage("err");
      pushLog(`✗ preview: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function runPlan() {
    if (!publicKey || !plan) return;
    setStage("send");
    try {
      if (plan.mode === "single") {
        const sig = await sendVersionedTx(connection, wallet, plan.versionedTx);
        pushLog(`✓ withdraw_sol: ${sig.slice(0, 12)}…`);
        pushLog(`See: ${explorerTx(sig, config.explorerCluster)}`);
      } else {
        if (!plan.swapTx) {
          throw new Error("split plan missing swapTx — internal bug");
        }
        pushLog("split: signing tx0 (Withdraw) then tx1 (swaps)…");
        const sig0 = await sendVersionedTx(connection, wallet, plan.versionedTx);
        pushLog(`✓ withdraw: ${sig0.slice(0, 12)}…`);
        pushLog(`See: ${explorerTx(sig0, config.explorerCluster)}`);
        const sig1 = await sendVersionedTx(connection, wallet, plan.swapTx);
        pushLog(`✓ jupiter_swaps: ${sig1.slice(0, 12)}…`);
        pushLog(`See: ${explorerTx(sig1, config.explorerCluster)}`);
      }
      setStage("ok");
      // Re-fetch balance after settlement so the UI reflects the burn.
      try {
        if (etf && publicKey) {
          const userAta = getAssociatedTokenAddressSync(etf.etfMint, publicKey, false);
          const bal = await connection.getTokenAccountBalance(userAta, "confirmed");
          setUserEtfBalance(BigInt(bal.value.amount));
        }
      } catch {
        /* ignore */
      }
      setPlan(null);
    } catch (e) {
      setStage("err");
      pushLog(`✗ withdraw_sol: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Withdraw → SOL (axis-vault + Jupiter)</h2>
        <span className="rounded-full bg-slate-800 px-2 py-0.5 font-mono text-[10px] text-slate-400">
          {truncatePubkey(axisVault.toBase58(), 6, 6)}
        </span>
      </header>

      {!publicKey ? (
        <p className="text-sm text-slate-400">Connect a wallet first.</p>
      ) : !config.jupiterEnabled ? (
        <p className="text-sm text-amber-300">
          Jupiter is disabled on {config.label}. Switch to Mainnet + Jupiter to use this flow.
        </p>
      ) : (
        <div className="space-y-3 text-xs">
          <label className="flex flex-col gap-1">
            <span className="text-slate-400">ETF state PDA (paste from Explorer)</span>
            <input
              value={etfStateAddr}
              onChange={(e) => setEtfStateAddr(e.target.value.trim())}
              placeholder="EtfState PDA address (base58)"
              className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
            />
          </label>

          {etfLoadErr && (
            <p className="break-all text-rose-400">✗ {etfLoadErr}</p>
          )}

          {etf && (
            <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <Metric label="ticker / name" value={`${etf.ticker} · ${etf.name}`} />
                <Metric label="basket size" value={`${etf.tokenCount}`} />
                <Metric
                  label="total supply"
                  value={`${(Number(etf.totalSupply) / 10 ** ETF_DECIMALS).toLocaleString(undefined, { maximumFractionDigits: ETF_DECIMALS })}`}
                />
                <Metric label="fee (bps)" value={`${etf.feeBps}`} />
                <Metric
                  label="status"
                  value={etf.paused ? "PAUSED" : "active"}
                />
                <Metric
                  label="treasury"
                  value={truncatePubkey(etf.treasury.toBase58(), 4, 4)}
                />
              </div>
              <ul className="mt-3 space-y-1">
                {etf.tokenMints.map((m, i) => (
                  <li key={m.toBase58()} className="flex items-center gap-2">
                    <span className="font-mono text-slate-300">
                      {truncatePubkey(m.toBase58(), 6, 6)}
                    </span>
                    <span className="text-slate-500">
                      {(etf.weightsBps[i] / 100).toFixed(1)}%
                    </span>
                    <a
                      href={explorerAddr(etf.tokenVaults[i].toBase58(), config.explorerCluster)}
                      target="_blank"
                      rel="noreferrer"
                      className="ml-auto text-slate-500 underline hover:text-slate-300"
                    >
                      vault
                    </a>
                  </li>
                ))}
              </ul>
              {userEtfBalance !== null && (
                <p className="mt-2 text-slate-400">
                  Your ETF balance:{" "}
                  <span className="font-mono text-slate-200">
                    {(Number(userEtfBalance) / 10 ** ETF_DECIMALS).toLocaleString(undefined, {
                      maximumFractionDigits: ETF_DECIMALS,
                    })}
                  </span>
                </p>
              )}
            </div>
          )}

          {etf && (
            <div className="grid grid-cols-3 gap-3">
              <label className="flex flex-col gap-1">
                <span className="text-slate-400">Burn (ETF tokens)</span>
                <input
                  type="number"
                  min={0}
                  step={0.001}
                  value={burnUi}
                  onChange={(e) => setBurnUi(e.target.value)}
                  className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-slate-400">Jupiter slippage (bps)</span>
                <input
                  type="number"
                  min={1}
                  max={500}
                  value={slippageBps}
                  onChange={(e) => setSlippageBps(Number(e.target.value))}
                  className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-slate-400">
                  Safety shrink (bps, basket dust)
                </span>
                <input
                  type="number"
                  min={0}
                  max={1000}
                  value={safetyShrinkBps}
                  onChange={(e) => setSafetyShrinkBps(Number(e.target.value))}
                  className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                />
              </label>
            </div>
          )}

          {burnExceedsBalance && (
            <p className="text-rose-400">✗ burn exceeds wallet balance</p>
          )}
          {burnExceedsSupply && (
            <p className="text-rose-400">✗ burn exceeds total supply</p>
          )}
          {etf?.paused && (
            <p className="text-amber-400">⚠ ETF is paused — withdraw is disabled.</p>
          )}

          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={previewPlan}
              disabled={!canPreview}
              className="rounded-lg border border-sky-700 px-3 py-1.5 font-medium text-sky-200 hover:border-sky-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {stage === "preview" ? "previewing…" : "Preview"}
            </button>
            <button
              type="button"
              onClick={runPlan}
              disabled={!plan || stage === "send"}
              className="rounded-lg bg-emerald-600 px-3 py-1.5 font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {stage === "send" ? "sending…" : "Run withdraw"}
            </button>
            {plan && (
              <span className="text-slate-400">
                expected{" "}
                <span className="font-mono text-emerald-300">
                  {(Number(plan.expectedSolOut) / 1e9).toFixed(6)} SOL
                </span>{" "}
                · min{" "}
                <span className="font-mono text-amber-300">
                  {(Number(plan.minSolOut) / 1e9).toFixed(6)} SOL
                </span>
              </span>
            )}
          </div>

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

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-slate-800 bg-slate-950/50 px-2 py-1">
      <p className="text-[10px] uppercase tracking-wider text-slate-500">{label}</p>
      <p className="font-mono text-slate-200">{value}</p>
    </div>
  );
}

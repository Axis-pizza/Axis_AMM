import { useEffect, useMemo, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import {
  fetchEtfState,
  fetchRebalanceState,
  fetchVaultBalances,
  type EtfStateData,
  type RebalanceStateData,
} from "../lib/etfState";
import {
  findRebalanceState,
  ixApplyWeights,
  ixProposeWeights,
} from "../lib/ix";
import { sendTx, explorerTx } from "../lib/tx";
import { truncatePubkey } from "../lib/format";
import type { ClusterConfig } from "../lib/programs";

/// Mirrors `contracts/axis-vault/src/constants.rs` v1.2 governance
/// constants so the panel can validate before the program does and show
/// the manager why an action is (un)available.
const MAX_WEIGHT_DELTA_BPS = 2_000;
const WEIGHT_TIMELOCK_SLOTS = 216_000;
const REBALANCE_WINDOW_SLOTS = 9_000;
const MAX_TURNOVER_BPS = 2_000;
const SLOT_SECONDS = 0.4;

/// Admin-only panel for axis-vault v1.2 weight governance + rebalance
/// status. ProposeWeights / ApplyWeights are exact on-chain calls with
/// no external dependency, so they run end-to-end here. The actual
/// Jupiter-swap rebalance is a manager operation that needs a vault-side
/// Jupiter route (the same on-chain CPI path the contract validates
/// post-swap); this panel surfaces the live turnover budget and the
/// recommended trade so an operator can drive it, but does not author
/// the Jupiter route in-browser.
export function RebalancePanel({ config }: { config: ClusterConfig }) {
  const { connection } = useConnection();
  const wallet = useWallet();
  const { publicKey } = wallet;
  const axisVault = useMemo(
    () => config.programs.find((p) => p.name === "axis-vault")!.address,
    [config],
  );

  const [etfStateAddr, setEtfStateAddr] = useState("");
  const [etf, setEtf] = useState<EtfStateData | null>(null);
  const [rebal, setRebal] = useState<RebalanceStateData | null>(null);
  const [vaultBalances, setVaultBalances] = useState<bigint[] | null>(null);
  const [currentSlot, setCurrentSlot] = useState<bigint | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);

  const [weightInputs, setWeightInputs] = useState<string[]>([]);
  const [stage, setStage] = useState<
    "idle" | "loading" | "propose" | "apply" | "ok" | "err"
  >("idle");
  const [log, setLog] = useState<string[]>([]);

  function pushLog(line: string) {
    setLog((l) => [...l, line]);
  }

  const rebalanceStatePda = useMemo(() => {
    if (!etf) return null;
    try {
      const etfPda = new PublicKey(etfStateAddr);
      return findRebalanceState(axisVault, etfPda)[0];
    } catch {
      return null;
    }
  }, [etf, etfStateAddr, axisVault]);

  async function loadAll() {
    setEtf(null);
    setRebal(null);
    setVaultBalances(null);
    setLoadErr(null);
    if (!etfStateAddr) return;
    try {
      const pda = new PublicKey(etfStateAddr);
      setStage("loading");
      const data = await fetchEtfState(connection, pda);
      setEtf(data);
      setWeightInputs(data.weightsBps.map((w) => String(w)));

      const [rebalPda] = findRebalanceState(axisVault, pda);
      const [rebalData, balances, slot] = await Promise.all([
        fetchRebalanceState(connection, rebalPda),
        fetchVaultBalances(connection, data.tokenVaults).catch(() => null),
        connection.getSlot("confirmed").then((s) => BigInt(s)),
      ]);
      setRebal(rebalData);
      setVaultBalances(balances);
      setCurrentSlot(slot);
      setStage("idle");
    } catch (e) {
      setLoadErr(e instanceof Error ? e.message : String(e));
      setStage("err");
    }
  }

  useEffect(() => {
    if (!etfStateAddr) return;
    if (etfStateAddr.length < 32 || etfStateAddr.length > 44) return;
    void loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [etfStateAddr, publicKey?.toBase58()]);

  const isAuthority =
    !!etf && !!publicKey && etf.authority.equals(publicKey);

  // ─── proposed-weights validation (mirrors the program) ───
  const parsedWeights = useMemo(
    () => weightInputs.map((s) => Number(s)),
    [weightInputs],
  );
  const weightsValid = useMemo(() => {
    if (!etf) return false;
    if (parsedWeights.length !== etf.tokenCount) return false;
    let sum = 0;
    for (let i = 0; i < etf.tokenCount; i++) {
      const w = parsedWeights[i];
      if (!Number.isInteger(w) || w <= 0 || w > 10_000) return false;
      sum += w;
    }
    return sum === 10_000;
  }, [parsedWeights, etf]);
  const deltaViolation = useMemo(() => {
    if (!etf || !weightsValid) return null;
    for (let i = 0; i < etf.tokenCount; i++) {
      const delta = Math.abs(parsedWeights[i] - etf.weightsBps[i]);
      if (delta > MAX_WEIGHT_DELTA_BPS) {
        return `token ${i + 1}: Δ${delta} bps exceeds the ${MAX_WEIGHT_DELTA_BPS} bps per-proposal cap`;
      }
    }
    return null;
  }, [parsedWeights, etf, weightsValid]);

  const hasPendingProposal =
    !!rebal && rebal.proposalEtaSlot > 0n;
  const etaSlotsRemaining = useMemo(() => {
    if (!hasPendingProposal || currentSlot === null || !rebal) return null;
    const rem = rebal.proposalEtaSlot - currentSlot;
    return rem > 0n ? rem : 0n;
  }, [hasPendingProposal, currentSlot, rebal]);
  const canApply = hasPendingProposal && etaSlotsRemaining === 0n;

  async function runPropose() {
    if (!publicKey || !etf || !rebalanceStatePda) return;
    setStage("propose");
    setLog([]);
    try {
      const ix = ixProposeWeights({
        programId: axisVault,
        authority: publicKey,
        etfState: new PublicKey(etfStateAddr),
        rebalanceState: rebalanceStatePda,
        newWeights: parsedWeights.slice(0, etf.tokenCount),
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(`✓ propose_weights: ${sig.slice(0, 12)}…`);
      pushLog(`See: ${explorerTx(sig, config.explorerCluster)}`);
      pushLog(
        `Timelock: ~${((WEIGHT_TIMELOCK_SLOTS * SLOT_SECONDS) / 3600).toFixed(0)}h ` +
          `until ApplyWeights becomes valid.`,
      );
      setStage("ok");
      await loadAll();
    } catch (e) {
      setStage("err");
      pushLog(`✗ propose_weights: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function runApply() {
    if (!publicKey || !etf || !rebalanceStatePda) return;
    setStage("apply");
    try {
      const ix = ixApplyWeights({
        programId: axisVault,
        authority: publicKey,
        etfState: new PublicKey(etfStateAddr),
        rebalanceState: rebalanceStatePda,
      });
      const sig = await sendTx(connection, wallet, [ix]);
      pushLog(`✓ apply_weights: ${sig.slice(0, 12)}…`);
      pushLog(`See: ${explorerTx(sig, config.explorerCluster)}`);
      setStage("ok");
      await loadAll();
    } catch (e) {
      setStage("err");
      pushLog(`✗ apply_weights: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-lg font-semibold">
          Rebalance &amp; weights{" "}
          <span className="text-slate-400 text-sm">(manager-only)</span>
        </h2>
        <span className="rounded-full bg-slate-800 px-2 py-0.5 font-mono text-[10px] text-slate-400">
          {truncatePubkey(axisVault.toBase58(), 6, 6)}
        </span>
      </header>

      {!publicKey ? (
        <p className="text-sm text-slate-400">Connect the ETF authority wallet first.</p>
      ) : (
        <div className="space-y-3 text-xs">
          <label className="flex flex-col gap-1">
            <span className="text-slate-400">ETF state PDA</span>
            <input
              value={etfStateAddr}
              onChange={(e) => setEtfStateAddr(e.target.value.trim())}
              placeholder="EtfState PDA address (base58)"
              className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
            />
          </label>

          {loadErr && <p className="break-all text-rose-400">✗ {loadErr}</p>}

          {etf && (
            <>
              <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
                <div className="grid gap-2 sm:grid-cols-2">
                  <Metric label="ticker / name" value={`${etf.ticker} · ${etf.name}`} />
                  <Metric label="basket size" value={`${etf.tokenCount}`} />
                  <Metric
                    label="you are"
                    value={isAuthority ? "the authority ✓" : "NOT the authority"}
                  />
                  <Metric label="status" value={etf.paused ? "PAUSED" : "active"} />
                </div>
                {!isAuthority && (
                  <p className="mt-2 text-amber-400">
                    ⚠ Connected wallet is not the ETF authority. Propose / Apply
                    will be rejected on-chain (OwnerMismatch).
                  </p>
                )}
              </div>

              <WeightTable
                etf={etf}
                vaultBalances={vaultBalances}
                rebal={rebal}
              />

              {/* Turnover window status */}
              {rebal && (
                <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
                  <p className="mb-1 text-[10px] uppercase tracking-wider text-slate-500">
                    Rebalance turnover window ({MAX_TURNOVER_BPS / 100}% per vault /
                    ~{((REBALANCE_WINDOW_SLOTS * SLOT_SECONDS) / 3600).toFixed(1)}h)
                  </p>
                  {rebal.windowStartSlot === 0n ? (
                    <p className="text-slate-400">No window opened yet.</p>
                  ) : (
                    <ul className="space-y-0.5 font-mono text-slate-300">
                      {etf.tokenVaults.map((_, i) => {
                        const snap = rebal.windowSnapshot[i];
                        const sold = rebal.windowSold[i];
                        const cap = (snap * BigInt(MAX_TURNOVER_BPS)) / 10_000n;
                        const remaining = cap > sold ? cap - sold : 0n;
                        return (
                          <li key={i}>
                            token {i + 1}: sold {sold.toString()} / cap{" "}
                            {cap.toString()} · remaining {remaining.toString()}
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </div>
              )}

              {/* Pending proposal status */}
              {hasPendingProposal && rebal && (
                <div className="rounded-lg border border-amber-800/60 bg-amber-950/20 p-3">
                  <p className="mb-1 text-[10px] uppercase tracking-wider text-amber-400">
                    Pending weight proposal
                  </p>
                  <p className="font-mono text-slate-200">
                    {rebal.proposedWeights
                      .slice(0, etf.tokenCount)
                      .map((w) => (w / 100).toFixed(1) + "%")
                      .join(" / ")}
                  </p>
                  <p className="mt-1 text-slate-400">
                    eta slot {rebal.proposalEtaSlot.toString()} ·{" "}
                    {etaSlotsRemaining === null
                      ? "—"
                      : etaSlotsRemaining === 0n
                        ? "ready to apply"
                        : `~${((Number(etaSlotsRemaining) * SLOT_SECONDS) / 3600).toFixed(1)}h remaining (${etaSlotsRemaining.toString()} slots)`}
                  </p>
                  <button
                    type="button"
                    onClick={runApply}
                    disabled={!canApply || !isAuthority || stage === "apply"}
                    className="mt-2 rounded-lg bg-emerald-600 px-3 py-1.5 font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {stage === "apply" ? "applying…" : "Apply weights"}
                  </button>
                </div>
              )}

              {/* Propose new weights */}
              <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
                <p className="mb-2 text-[10px] uppercase tracking-wider text-slate-500">
                  Propose new weights (bps, sum = 10 000, ≤ {MAX_WEIGHT_DELTA_BPS} bps
                  move each)
                </p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                  {etf.tokenMints.map((m, i) => (
                    <label key={m.toBase58()} className="flex flex-col gap-1">
                      <span className="text-slate-400">
                        {truncatePubkey(m.toBase58(), 4, 4)}{" "}
                        <span className="text-slate-600">
                          (now {etf.weightsBps[i]})
                        </span>
                      </span>
                      <input
                        type="number"
                        min={1}
                        max={10000}
                        value={weightInputs[i] ?? ""}
                        onChange={(e) => {
                          const next = [...weightInputs];
                          next[i] = e.target.value;
                          setWeightInputs(next);
                        }}
                        className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                      />
                    </label>
                  ))}
                </div>
                <p className="mt-2 text-slate-500">
                  sum:{" "}
                  <span
                    className={
                      "font-mono " +
                      (weightsValid ? "text-emerald-300" : "text-rose-400")
                    }
                  >
                    {parsedWeights.reduce((s, w) => s + (Number.isFinite(w) ? w : 0), 0)}
                  </span>{" "}
                  / 10 000
                </p>
                {deltaViolation && (
                  <p className="mt-1 text-rose-400">✗ {deltaViolation}</p>
                )}
                <button
                  type="button"
                  onClick={runPropose}
                  disabled={
                    !weightsValid ||
                    !!deltaViolation ||
                    !isAuthority ||
                    stage === "propose"
                  }
                  className="mt-2 rounded-lg border border-sky-700 px-3 py-1.5 font-medium text-sky-200 hover:border-sky-500 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {stage === "propose" ? "proposing…" : "Propose weights"}
                </button>
                {hasPendingProposal && (
                  <p className="mt-1 text-slate-500">
                    A new proposal overwrites the pending one and restarts the
                    timelock.
                  </p>
                )}
              </div>
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

/// Show current weight vs the live vault token balance for each leg.
/// (Balances are raw token amounts, not value — a value view needs
/// per-token prices, which this panel intentionally does not pull.)
function WeightTable({
  etf,
  vaultBalances,
  rebal,
}: {
  etf: EtfStateData;
  vaultBalances: bigint[] | null;
  rebal: RebalanceStateData | null;
}) {
  return (
    <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
      <p className="mb-1 text-[10px] uppercase tracking-wider text-slate-500">
        Basket
      </p>
      <ul className="space-y-1">
        {etf.tokenMints.map((m, i) => (
          <li key={m.toBase58()} className="flex items-center gap-2">
            <span className="font-mono text-slate-300">
              {truncatePubkey(m.toBase58(), 6, 6)}
            </span>
            <span className="text-slate-500">
              target {(etf.weightsBps[i] / 100).toFixed(1)}%
            </span>
            <span className="ml-auto font-mono text-slate-400">
              vault{" "}
              {vaultBalances ? vaultBalances[i].toString() : "—"}
            </span>
          </li>
        ))}
      </ul>
      {rebal === null && (
        <p className="mt-2 text-slate-500">
          No rebalance sidecar yet — it's created on the first ProposeWeights /
          Rebalance call.
        </p>
      )}
    </div>
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

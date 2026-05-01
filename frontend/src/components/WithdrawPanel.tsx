import { useEffect, useMemo, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { ixWithdraw } from "../lib/ix";
import {
  fetchEtfState,
  fetchVaultBalances,
  expectedWithdrawOutputs,
  type EtfStateData,
} from "../lib/etfState";
import { sendTx, explorerTx, explorerAddr } from "../lib/tx";
import { truncatePubkey } from "../lib/format";
import type { ClusterConfig } from "../lib/programs";

const ETF_DECIMALS = 6;

/// Plain Withdraw panel for `axis-vault` — burns ETF tokens, returns
/// each basket mint to the user's ATA proportionally. Mirrors the
/// on-chain order-of-operations: fee is taken FIRST, then the residual
/// `effective_burn` drives the per-vault payout. No Jupiter route here;
/// the basket-tokens-to-SOL flow lives in WithdrawSolPanel.
///
/// Mainnet caveats baked in:
///   • idempotent ATA creates for every user basket mint + treasury ETF
///     ATA, so the on-chain SPL Transfer/MintTo never aborts on missing
///     destinations.
///   • `min_tokens_out` is summed across legs (matches
///     `contracts/axis-vault/src/instructions/withdraw.rs:139-166`); we
///     apply a user-controlled safety shrink to absorb dust.
export function WithdrawPanel({ config }: { config: ClusterConfig }) {
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
  const [vaultBalances, setVaultBalances] = useState<bigint[] | null>(null);
  const [tokenDecimals, setTokenDecimals] = useState<number[] | null>(null);

  const [burnUi, setBurnUi] = useState<string>("0.5");
  const [safetyShrinkBps, setSafetyShrinkBps] = useState<number>(0);
  const [stage, setStage] = useState<"idle" | "loading" | "send" | "ok" | "err">(
    "idle",
  );
  const [log, setLog] = useState<string[]>([]);

  function pushLog(line: string) {
    setLog((l) => [...l, line]);
  }

  async function loadEtf() {
    setEtf(null);
    setEtfLoadErr(null);
    setUserEtfBalance(null);
    setVaultBalances(null);
    setTokenDecimals(null);
    if (!etfStateAddr) return;
    try {
      const pubkey = new PublicKey(etfStateAddr);
      setStage("loading");
      const data = await fetchEtfState(connection, pubkey);
      setEtf(data);

      // Vault balances + per-mint decimals — needed for both the per-leg
      // preview and the slippage floor.
      const balances = await fetchVaultBalances(connection, data.tokenVaults);
      setVaultBalances(balances);
      const mintInfos = await connection.getMultipleAccountsInfo(
        data.tokenMints,
        "confirmed",
      );
      const decimals = mintInfos.map((info, i) => {
        if (!info) throw new Error(`mint ${data.tokenMints[i].toBase58()} not found`);
        // SPL Mint layout: byte 44 = decimals.
        return info.data[44];
      });
      setTokenDecimals(decimals);

      if (publicKey) {
        const userAta = getAssociatedTokenAddressSync(data.etfMint, publicKey, false);
        try {
          const bal = await connection.getTokenAccountBalance(userAta, "confirmed");
          setUserEtfBalance(BigInt(bal.value.amount));
        } catch {
          setUserEtfBalance(0n);
        }
      }
      setStage("idle");
    } catch (e) {
      setEtfLoadErr(e instanceof Error ? e.message : String(e));
      setStage("err");
    }
  }

  useEffect(() => {
    if (!etfStateAddr) return;
    if (etfStateAddr.length < 32 || etfStateAddr.length > 44) return;
    void loadEtf();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [etfStateAddr, publicKey?.toBase58()]);

  const burnAmountBase = useMemo(() => {
    const n = Number(burnUi);
    if (!Number.isFinite(n) || n <= 0) return 0n;
    return BigInt(Math.floor(n * 10 ** ETF_DECIMALS));
  }, [burnUi]);

  const preview = useMemo(() => {
    if (!etf || !vaultBalances || burnAmountBase <= 0n) return null;
    if (burnAmountBase > etf.totalSupply) return null;
    return expectedWithdrawOutputs(
      vaultBalances,
      burnAmountBase,
      etf.totalSupply,
      etf.feeBps,
    );
  }, [etf, vaultBalances, burnAmountBase]);

  const burnExceedsBalance =
    userEtfBalance !== null && burnAmountBase > userEtfBalance;
  const burnExceedsSupply =
    etf !== null && burnAmountBase > etf.totalSupply;

  async function runWithdraw() {
    if (!publicKey || !etf || !preview) return;
    setStage("send");
    try {
      const userEtfAta = getAssociatedTokenAddressSync(etf.etfMint, publicKey, false);
      const treasuryEtfAta = getAssociatedTokenAddressSync(
        etf.etfMint,
        etf.treasury,
        true,
      );
      const userBasketAtas = etf.tokenMints.map((m) =>
        getAssociatedTokenAddressSync(m, publicKey, false),
      );

      const ataIxs = [
        createAssociatedTokenAccountIdempotentInstruction(
          publicKey,
          treasuryEtfAta,
          etf.treasury,
          etf.etfMint,
        ),
        ...etf.tokenMints.map((m, i) =>
          createAssociatedTokenAccountIdempotentInstruction(
            publicKey,
            userBasketAtas[i],
            publicKey,
            m,
          ),
        ),
      ];

      const totalExpected = preview.perLeg.reduce((a, b) => a + b, 0n);
      const minTokensOut =
        (totalExpected * BigInt(10_000 - safetyShrinkBps)) / 10_000n;

      const ix = ixWithdraw({
        programId: axisVault,
        payer: publicKey,
        etfState: new PublicKey(etfStateAddr),
        etfMint: etf.etfMint,
        userEtfAta,
        treasuryEtfAta,
        vaults: etf.tokenVaults,
        userBasketAccounts: userBasketAtas,
        burnAmount: burnAmountBase,
        minTokensOut,
        name: etf.name,
      });

      pushLog(
        `Burn ${(Number(burnAmountBase) / 10 ** ETF_DECIMALS).toFixed(ETF_DECIMALS)} ETF · fee ${preview.feeAmount.toString()} base · effective ${preview.effectiveBurn.toString()}`,
      );
      pushLog(
        `Per-leg expected (sum ${totalExpected.toString()}, min ${minTokensOut.toString()}):`,
      );
      preview.perLeg.forEach((amt, i) => {
        pushLog(
          `  ${truncatePubkey(etf.tokenMints[i].toBase58(), 4, 4)}: ${amt.toString()} base`,
        );
      });

      const sig = await sendTx(connection, wallet, [...ataIxs, ix]);
      pushLog(`✓ withdraw: ${sig.slice(0, 12)}…`);
      pushLog(`See: ${explorerTx(sig, config.explorerCluster)}`);
      setStage("ok");

      // Re-fetch ETF balance + vault balances so the next preview is accurate.
      try {
        const bal = await connection.getTokenAccountBalance(userEtfAta, "confirmed");
        setUserEtfBalance(BigInt(bal.value.amount));
        const fresh = await fetchVaultBalances(connection, etf.tokenVaults);
        setVaultBalances(fresh);
      } catch {
        /* ignore */
      }
    } catch (e) {
      setStage("err");
      pushLog(`✗ withdraw: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Withdraw ETF (axis-vault)</h2>
        <span className="rounded-full bg-slate-800 px-2 py-0.5 font-mono text-[10px] text-slate-400">
          {truncatePubkey(axisVault.toBase58(), 6, 6)}
        </span>
      </header>

      {!publicKey ? (
        <p className="text-sm text-slate-400">Connect a wallet first.</p>
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

          {etfLoadErr && <p className="break-all text-rose-400">✗ {etfLoadErr}</p>}

          {etf && (
            <div className="rounded-lg border border-slate-800 bg-slate-950/50 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <Metric label="ticker / name" value={`${etf.ticker} · ${etf.name}`} />
                <Metric label="basket size" value={`${etf.tokenCount}`} />
                <Metric
                  label="total supply"
                  value={`${(Number(etf.totalSupply) / 10 ** ETF_DECIMALS).toLocaleString(undefined, { maximumFractionDigits: ETF_DECIMALS })}`}
                />
                <Metric label="fee bps" value={`${etf.feeBps}`} />
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
                    {vaultBalances && tokenDecimals && (
                      <span className="ml-2 font-mono text-slate-500">
                        vault{" "}
                        {(Number(vaultBalances[i]) / 10 ** tokenDecimals[i]).toLocaleString(
                          undefined,
                          { maximumFractionDigits: tokenDecimals[i] },
                        )}
                      </span>
                    )}
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
                    {(Number(userEtfBalance) / 10 ** ETF_DECIMALS).toLocaleString(
                      undefined,
                      { maximumFractionDigits: ETF_DECIMALS },
                    )}
                  </span>
                </p>
              )}
            </div>
          )}

          {etf && (
            <div className="grid grid-cols-2 gap-3">
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
            <p className="text-rose-400">✗ burn exceeds wallet ETF balance</p>
          )}
          {burnExceedsSupply && (
            <p className="text-rose-400">✗ burn exceeds total supply</p>
          )}
          {etf?.paused && (
            <p className="text-amber-400">⚠ ETF is paused — withdraw is disabled.</p>
          )}

          {preview && etf && tokenDecimals && (
            <div className="rounded border border-slate-800 bg-slate-950/40 p-3">
              <p className="mb-1 text-slate-400">Preview (per leg, after fee):</p>
              <ul className="space-y-1">
                {preview.perLeg.map((amt, i) => (
                  <li key={etf.tokenMints[i].toBase58()} className="flex items-center gap-2">
                    <span className="font-mono text-slate-300">
                      {truncatePubkey(etf.tokenMints[i].toBase58(), 4, 4)}
                    </span>
                    <span className="ml-auto font-mono text-emerald-300">
                      +{(Number(amt) / 10 ** tokenDecimals[i]).toLocaleString(undefined, {
                        maximumFractionDigits: tokenDecimals[i],
                      })}
                    </span>
                  </li>
                ))}
              </ul>
              <p className="mt-2 text-[11px] text-slate-500">
                fee {preview.feeAmount.toString()} base · effective burn{" "}
                {preview.effectiveBurn.toString()} base
              </p>
            </div>
          )}

          <button
            type="button"
            onClick={runWithdraw}
            disabled={
              !etf ||
              !preview ||
              burnExceedsBalance ||
              burnExceedsSupply ||
              etf.paused ||
              stage === "send" ||
              stage === "loading"
            }
            className="rounded-lg bg-rose-600 px-3 py-1.5 font-medium text-white hover:bg-rose-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {stage === "send" ? "burning…" : "Burn → basket"}
          </button>

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

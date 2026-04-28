import { useEffect, useState, useCallback } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { type WalletToken, fetchWalletTokens } from "../lib/tokens";
import { buildCreateMintWithSupplyIxs } from "../lib/spl";
import { sendTx, explorerAddr, explorerTx } from "../lib/tx";
import { truncatePubkey } from "../lib/format";
import type { ClusterConfig } from "../lib/programs";

const MAINNET_PRESETS = [
  { symbol: "USDC", mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" },
  { symbol: "USDT", mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY8wYb6Fq4jmWZtj" },
  { symbol: "JitoSOL", mint: "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn" },
  { symbol: "mSOL", mint: "mSoLzYCxHdedyZ9E3uXvJxiftf7KqJQkU1XQv6r2fE7" },
  { symbol: "BONK", mint: "DezXAZ8z7PnrnRJjz3B97eywMR4vXv1Yu2fB263wXF5B" },
  { symbol: "WIF", mint: "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzL2jH2Bzv4b5" },
] as const;

/// Token-list panel. Two jobs:
///   1. Show the wallet's current SPL holdings (mint, decimals, ui amount).
///      The mint hash is the source of truth.
///   2. On devnet, one-click "Mint a fresh test token" so the user has
///      something to put into an ETF / pfmm pool without leaving the page.
///
/// The ETF / pfmm panels read from this same wallet token list to let
/// the user pick basket members, so this panel doubles as a token picker.
export function TokensPanel({
  onSelect,
  selectedMints,
  cluster = "devnet",
  explorerCluster = "devnet",
}: {
  onSelect?: (mint: string) => void;
  selectedMints?: string[];
  cluster?: ClusterConfig["cluster"];
  explorerCluster?: ClusterConfig["explorerCluster"];
} = {}) {
  const { connection } = useConnection();
  const wallet = useWallet();
  const { publicKey } = wallet;
  const [tokens, setTokens] = useState<WalletToken[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [mintingState, setMintingState] = useState<"idle" | "pending" | "ok" | "err">(
    "idle",
  );
  const [mintMsg, setMintMsg] = useState<string>("");
  const [mintSig, setMintSig] = useState<string>("");
  const [manualMint, setManualMint] = useState("");
  const [decimals, setDecimals] = useState(6);
  const [supply, setSupply] = useState(100_000); // ui-amount, multiplied by 10^decimals on send

  const refresh = useCallback(async () => {
    if (!publicKey) {
      setTokens(null);
      return;
    }
    setLoading(true);
    try {
      const list = await fetchWalletTokens(connection, publicKey);
      setTokens(list);
    } finally {
      setLoading(false);
    }
  }, [connection, publicKey]);

  useEffect(() => {
    void refresh();
    const id = setInterval(refresh, 12_000);
    return () => clearInterval(id);
  }, [refresh]);

  async function mintTestToken() {
    if (!publicKey) return;
    setMintingState("pending");
    setMintMsg("");
    setMintSig("");
    try {
      const initial = BigInt(Math.floor(supply)) * BigInt(10 ** decimals);
      const bundle = await buildCreateMintWithSupplyIxs(
        connection,
        publicKey,
        decimals,
        initial,
      );
      const sig = await sendTx(connection, wallet, bundle.ixs, bundle.signers);
      setMintingState("ok");
      setMintSig(sig);
      setMintMsg(
        `${truncatePubkey(bundle.mint.toBase58())} · ${sig.slice(0, 10)}…`,
      );
      // Optimistically refresh; balance shows up after ~1 confirm.
      void refresh();
    } catch (e) {
      setMintingState("err");
      setMintMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Tokens</h2>
        <button
          onClick={refresh}
          disabled={!publicKey || loading}
          className="rounded-md border border-slate-700 px-2 py-1 text-xs text-slate-300 hover:border-slate-500 disabled:opacity-50"
        >
          {loading ? "…" : "↻ refresh"}
        </button>
      </header>

      {!publicKey ? (
        <p className="text-sm text-slate-400">Connect a wallet to see SPL holdings.</p>
      ) : (
        <>
          <div className="mb-4 rounded-lg border border-slate-800 bg-slate-950/60 p-4">
            <p className="mb-2 text-xs uppercase tracking-wider text-slate-400">
              {cluster === "devnet" ? "Mint a fresh devnet SPL token" : "Mainnet token source"}
            </p>
            {cluster === "devnet" ? (
            <div className="mb-3 flex items-end gap-3 text-xs">
              <label className="flex flex-col">
                <span className="mb-1 text-slate-400">decimals</span>
                <input
                  type="number"
                  min={0}
                  max={9}
                  value={decimals}
                  onChange={(e) => setDecimals(Number(e.target.value))}
                  className="w-16 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                />
              </label>
              <label className="flex flex-col">
                <span className="mb-1 text-slate-400">initial supply</span>
                <input
                  type="number"
                  min={0}
                  value={supply}
                  onChange={(e) => setSupply(Number(e.target.value))}
                  className="w-32 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                />
              </label>
              <button
                onClick={mintTestToken}
                disabled={mintingState === "pending"}
                className="rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
              >
                {mintingState === "pending" ? "minting…" : "Mint"}
              </button>
            </div>
            ) : (
              <div className="space-y-3 text-xs">
                <p className="text-slate-400">
                  Mainnet minting is disabled. Pick liquid presets or paste a
                  mint; Jupiter SOL-in seed flow can acquire basket tokens
                  during deposit.
                </p>
                {onSelect && (
                  <>
                    <div className="flex flex-wrap gap-2">
                      {MAINNET_PRESETS.map((token) => {
                        const active = selectedMints?.includes(token.mint);
                        return (
                          <button
                            key={token.mint}
                            onClick={() => onSelect(token.mint)}
                            className={
                              "rounded-md border px-2 py-1 font-mono " +
                              (active
                                ? "border-indigo-500 bg-indigo-600/20 text-indigo-300"
                                : "border-slate-700 text-slate-300 hover:border-slate-500")
                            }
                          >
                            {active ? "✓ " : ""}
                            {token.symbol}
                          </button>
                        );
                      })}
                    </div>
                    <div className="flex gap-2">
                      <input
                        value={manualMint}
                        onChange={(e) => setManualMint(e.target.value.trim())}
                        placeholder="Paste SPL mint address"
                        className="min-w-0 flex-1 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
                      />
                      <button
                        onClick={() => {
                          if (manualMint) onSelect(manualMint);
                        }}
                        className="rounded-lg bg-slate-700 px-3 py-1.5 font-medium text-white hover:bg-slate-600"
                      >
                        Add
                      </button>
                    </div>
                  </>
                )}
              </div>
            )}
            {mintingState === "ok" && (
              <p className="text-xs text-emerald-400">✓ {mintMsg}</p>
            )}
            {mintingState === "err" && (
              <p className="break-all text-xs text-rose-400">✗ {mintMsg}</p>
            )}
          </div>

          {tokens && tokens.length === 0 ? (
            <p className="text-sm text-slate-400">
              No SPL tokens in this wallet yet. Mint one above to get started.
            </p>
          ) : tokens === null ? (
            <p className="text-sm text-slate-400">loading…</p>
          ) : (
            <ul className="divide-y divide-slate-800">
              {tokens.map((t) => {
                const mintStr = t.mint.toBase58();
                const isSelected = selectedMints?.includes(mintStr);
                return (
                  <li
                    key={mintStr}
                    className="flex items-center justify-between py-2"
                  >
                    <div className="min-w-0 text-sm">
                      <a
                        href={explorerAddr(mintStr, explorerCluster)}
                        target="_blank"
                        rel="noreferrer"
                        className="font-mono text-slate-200 hover:text-indigo-300"
                      >
                        {truncatePubkey(mintStr, 6, 6)}
                      </a>
                      <span className="ml-2 text-xs text-slate-500">
                        {t.label} · {t.decimals}d
                      </span>
                    </div>
                    <div className="flex items-center gap-3">
                      <span className="font-mono text-sm text-slate-300">
                        {t.uiAmount.toLocaleString(undefined, {
                          maximumFractionDigits: 6,
                        })}
                      </span>
                      {onSelect && (
                        <button
                          onClick={() => onSelect(mintStr)}
                          className={
                            "rounded-md border px-2 py-0.5 text-xs " +
                            (isSelected
                              ? "border-indigo-500 bg-indigo-600/20 text-indigo-300"
                              : "border-slate-700 text-slate-300 hover:border-slate-500")
                          }
                        >
                          {isSelected ? "✓ picked" : "pick"}
                        </button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {mintingState === "ok" && mintSig && (
            <p className="mt-3 text-xs text-slate-500">
              See last mint tx on{" "}
              <a
                href={explorerTx(mintSig, explorerCluster)}
                target="_blank"
                rel="noreferrer"
                className="underline hover:text-slate-300"
              >
                Solana Explorer
              </a>
            </p>
          )}
        </>
      )}
    </section>
  );
}

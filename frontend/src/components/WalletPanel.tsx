import { useEffect, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import { LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { lamportsToSol, truncatePubkey } from "../lib/format";

/// Wallet status card: connect button, current pubkey + SOL balance,
/// one-click "Airdrop 1 SOL" action that proves the wallet→RPC→program
/// path is healthy without touching any of our programs yet.
export function WalletPanel() {
  const { connection } = useConnection();
  const { publicKey, connected } = useWallet();
  const [balance, setBalance] = useState<number | null>(null);
  const [airdropState, setAirdropState] = useState<
    "idle" | "pending" | "ok" | "err"
  >("idle");
  const [airdropMsg, setAirdropMsg] = useState<string>("");

  useEffect(() => {
    if (!publicKey) {
      setBalance(null);
      return;
    }
    let cancelled = false;
    void connection.getBalance(publicKey).then((b) => {
      if (!cancelled) setBalance(b);
    });
    // Poll every 8s while connected so post-airdrop balance refreshes
    // without the user smashing F5.
    const id = setInterval(() => {
      void connection.getBalance(publicKey).then((b) => {
        if (!cancelled) setBalance(b);
      });
    }, 8000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [connection, publicKey]);

  async function airdrop() {
    if (!publicKey) return;
    setAirdropState("pending");
    setAirdropMsg("");
    try {
      const sig = await connection.requestAirdrop(
        publicKey as PublicKey,
        1 * LAMPORTS_PER_SOL,
      );
      // Devnet airdrop confirmations are slow; don't block on full
      // confirmation, just kick off and let the polling loop refresh.
      await connection.confirmTransaction(sig, "confirmed");
      setAirdropState("ok");
      setAirdropMsg(sig.slice(0, 12) + "…");
    } catch (e) {
      setAirdropState("err");
      const m = e instanceof Error ? e.message : String(e);
      // Devnet faucet is heavily rate-limited. Surface the real reason
      // rather than a generic "failed".
      setAirdropMsg(m.includes("rate") ? "rate-limited; try faucet.solana.com" : m);
    }
  }

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Wallet</h2>
        <WalletMultiButton />
      </header>

      {!connected ? (
        <p className="text-sm text-slate-400">
          Connect a Phantom or Solflare wallet (set to{" "}
          <span className="font-mono text-slate-200">Devnet</span>) to read
          on-chain program state.
        </p>
      ) : (
        <div className="space-y-3 text-sm">
          <Field
            label="Address"
            value={truncatePubkey(publicKey!.toBase58(), 8, 8)}
            mono
          />
          <Field
            label="Balance"
            value={
              balance === null ? "loading…" : `${lamportsToSol(balance)} SOL`
            }
          />
          <div className="pt-2">
            <button
              onClick={airdrop}
              disabled={airdropState === "pending"}
              className="rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {airdropState === "pending" ? "Airdropping…" : "Airdrop 1 devnet SOL"}
            </button>
            {airdropState === "ok" && airdropMsg && (
              <span className="ml-3 text-xs text-emerald-400">
                ✓ confirmed: {airdropMsg}
              </span>
            )}
            {airdropState === "err" && airdropMsg && (
              <span className="ml-3 text-xs text-rose-400">✗ {airdropMsg}</span>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

function Field({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="flex justify-between">
      <span className="text-slate-400">{label}</span>
      <span className={mono ? "font-mono text-slate-200" : "text-slate-200"}>
        {value}
      </span>
    </div>
  );
}

import { useEffect, useState } from "react";
import { useConnection } from "@solana/wallet-adapter-react";
import { fetchAllPools, type PoolState3Data } from "../lib/pfmmState";
import { explorerAddr } from "../lib/tx";
import { truncatePubkey } from "../lib/format";
import type { ClusterConfig } from "../lib/programs";

/// Pool discovery — `getProgramAccounts` on pfda-amm-3 with the
/// `pool3st\0` discriminator filter. Hands a click → 3-mint payload back
/// up to App, which loads it into selectedMints in the canonical chain
/// order so PfmmPanel sees the existing pool immediately (the pool PDA
/// is `[b"pool3", mint0, mint1, mint2]` so any other mint order would
/// hash to a different, uninitialized PDA).
///
/// Note on mainnet: vanilla RPC providers throttle `getProgramAccounts`
/// hard. The Helius endpoint configured via VITE_MAINNET_RPC_URL
/// supports it; if you swap to a budget RPC, expect this query to time
/// out or return partial results.
export function PoolsPanel({
  config,
  onPickPool,
}: {
  config: ClusterConfig;
  onPickPool: (mints: [string, string, string]) => void;
}) {
  const { connection } = useConnection();
  const pfmm = config.programs.find((p) => p.name === "pfda-amm-3")!.address;

  const [pools, setPools] = useState<PoolState3Data[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function load() {
    setLoading(true);
    setErr(null);
    try {
      const list = await fetchAllPools(connection, pfmm);
      // Newest pools (highest current_window_end ≈ most recent activity)
      // come first; ties broken by pool pubkey for determinism.
      list.sort((a, b) => {
        if (a.currentWindowEnd > b.currentWindowEnd) return -1;
        if (a.currentWindowEnd < b.currentWindowEnd) return 1;
        return a.pool.toBase58().localeCompare(b.pool.toBase58());
      });
      setPools(list);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pfmm.toBase58(), config.rpcUrl]);

  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-6 shadow-sm">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-lg font-semibold">PFMM pools (discovered)</h2>
        <div className="flex items-center gap-2">
          <span className="rounded-full bg-slate-800 px-2 py-0.5 font-mono text-[10px] text-slate-400">
            {truncatePubkey(pfmm.toBase58(), 6, 6)}
          </span>
          <button
            onClick={load}
            disabled={loading}
            className="rounded border border-slate-700 px-2 py-1 text-[11px] text-slate-300 hover:border-slate-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {loading ? "…" : "refresh"}
          </button>
        </div>
      </header>

      {err && <p className="break-all text-xs text-rose-400">✗ {err}</p>}

      {!err && pools === null && (
        <p className="text-sm text-slate-400">Loading pools…</p>
      )}

      {pools !== null && pools.length === 0 && (
        <p className="text-sm text-slate-400">
          No pools found on {config.label}. Initialize one from the PFMM tab.
        </p>
      )}

      {pools !== null && pools.length > 0 && (
        <ul className="space-y-2 text-xs">
          {pools.map((p) => (
            <PoolRow
              key={p.pool.toBase58()}
              pool={p}
              explorerCluster={config.explorerCluster}
              onPick={() =>
                onPickPool([
                  p.tokenMints[0].toBase58(),
                  p.tokenMints[1].toBase58(),
                  p.tokenMints[2].toBase58(),
                ])
              }
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function PoolRow({
  pool,
  explorerCluster,
  onPick,
}: {
  pool: PoolState3Data;
  explorerCluster: "devnet" | "";
  onPick: () => void;
}) {
  const reservesNonzero = pool.reserves.some((r) => r > 0n);
  return (
    <li className="rounded border border-slate-800 bg-slate-950/40 p-3">
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <a
          href={explorerAddr(pool.pool.toBase58(), explorerCluster)}
          target="_blank"
          rel="noreferrer"
          className="font-mono text-slate-200 hover:text-indigo-300"
        >
          {truncatePubkey(pool.pool.toBase58(), 8, 8)}
        </a>
        <span className="rounded bg-slate-800 px-1.5 py-0.5 text-[10px] text-slate-400">
          fee {pool.baseFeeBps} bps
        </span>
        <span className="rounded bg-slate-800 px-1.5 py-0.5 text-[10px] text-slate-400">
          window {pool.windowSlots.toString()} slots
        </span>
        <span className="rounded bg-slate-800 px-1.5 py-0.5 text-[10px] text-slate-400">
          batch {pool.currentBatchId.toString()}
        </span>
        {pool.paused && (
          <span className="rounded bg-rose-900/50 px-1.5 py-0.5 text-[10px] text-rose-200">
            paused
          </span>
        )}
        {!reservesNonzero && (
          <span className="rounded bg-amber-900/40 px-1.5 py-0.5 text-[10px] text-amber-200">
            empty
          </span>
        )}
        <button
          onClick={onPick}
          className="ml-auto rounded bg-indigo-600 px-3 py-1 text-[11px] font-medium text-white hover:bg-indigo-500"
        >
          Open in PFMM tab
        </button>
      </div>
      <ul className="space-y-0.5">
        {pool.tokenMints.map((m, i) => (
          <li key={m.toBase58()} className="flex items-center gap-2">
            <span className="w-4 text-slate-500">{i}</span>
            <a
              href={explorerAddr(m.toBase58(), explorerCluster)}
              target="_blank"
              rel="noreferrer"
              className="font-mono text-slate-300 hover:text-indigo-300"
            >
              {truncatePubkey(m.toBase58(), 6, 6)}
            </a>
            <span className="text-slate-500">
              w {(pool.weights[i] / 10_000).toFixed(2)}%
            </span>
            <span className="ml-auto font-mono text-slate-400">
              reserve {pool.reserves[i].toString()}
            </span>
          </li>
        ))}
      </ul>
      <div className="mt-2 text-[10px] text-slate-500">
        authority {truncatePubkey(pool.authority.toBase58(), 4, 4)} · treasury{" "}
        {truncatePubkey(pool.treasury.toBase58(), 4, 4)}
      </div>
    </li>
  );
}

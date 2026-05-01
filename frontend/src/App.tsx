import { useCallback, useEffect, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { AppWalletProvider } from "./components/WalletProvider";
import { WalletPanel } from "./components/WalletPanel";
import { ProgramCard } from "./components/ProgramCard";
import { ScopeNote } from "./components/ScopeNote";
import { TokensPanel } from "./components/TokensPanel";
import { CreateEtfPanel } from "./components/CreateEtfPanel";
import { WithdrawPanel } from "./components/WithdrawPanel";
import { WithdrawSolPanel } from "./components/WithdrawSolPanel";
import { PfmmPanel } from "./components/PfmmPanel";
import { PoolsPanel } from "./components/PoolsPanel";
import { getClusterConfig, type Cluster, type ClusterConfig } from "./lib/programs";
import { fetchWalletTokens } from "./lib/tokens";

export default function App() {
  const [cluster, setCluster] = useState<Cluster>("mainnet");
  const config = getClusterConfig(cluster);

  return (
    <AppWalletProvider endpoint={config.rpcUrl}>
      <Shell config={config} onClusterChange={setCluster} />
    </AppWalletProvider>
  );
}

type Tab =
  | "overview"
  | "tokens"
  | "etf"
  | "withdraw_etf"
  | "withdraw_sol"
  | "pools"
  | "pfmm";

function Shell({
  config,
  onClusterChange,
}: {
  config: ClusterConfig;
  onClusterChange: (cluster: Cluster) => void;
}) {
  const { connection } = useConnection();
  const { publicKey } = useWallet();
  const [tab, setTab] = useState<Tab>("overview");

  // Single source of truth for the basket selection — shared between
  // TokensPanel (picker), CreateEtfPanel and PfmmPanel.
  const [selectedMints, setSelectedMints] = useState<string[]>([]);
  const toggleMint = useCallback((mint: string) => {
    setSelectedMints((cur) =>
      cur.includes(mint) ? cur.filter((m) => m !== mint) : [...cur, mint],
    );
  }, []);
  const clearSelection = useCallback(() => setSelectedMints([]), []);
  /// Hand a 3-mint payload from PoolsPanel to PfmmPanel: replace the
  /// selection in the canonical chain order, then jump tabs so PfmmPanel
  /// re-fetches and binds to the existing pool.
  const pickPool = useCallback((mints: [string, string, string]) => {
    setSelectedMints([mints[0], mints[1], mints[2]]);
    setTab("pfmm");
  }, []);

  // Cache decimals-by-mint so PfmmPanel can convert UI amounts → base units.
  const [walletDecimals, setWalletDecimals] = useState<Record<string, number>>({});
  useEffect(() => {
    if (!publicKey) return;
    let cancelled = false;
    void fetchWalletTokens(connection, publicKey).then((tokens) => {
      if (cancelled) return;
      const map: Record<string, number> = {};
      for (const t of tokens) map[t.mint.toBase58()] = t.decimals;
      setWalletDecimals(map);
    });
    return () => {
      cancelled = true;
    };
  }, [connection, publicKey, selectedMints.join(",")]);

  return (
    <main className="min-h-screen px-6 py-12">
      <div className="mx-auto max-w-5xl space-y-8">
        <Header config={config} />
        <ClusterSwitch config={config} onChange={onClusterChange} />
        <ScopeNote cluster={config.cluster} />
        <WalletPanel />

        <Tabs current={tab} onChange={setTab} basketSize={selectedMints.length} />

        {tab === "overview" && (
          <section className="space-y-4">
            <h2 className="text-lg font-semibold">Deployed programs</h2>
            <div className="grid gap-4 md:grid-cols-2">
              {config.programs.map((p) => (
                <ProgramCard
                  key={p.address.toBase58()}
                  program={p}
                  explorerCluster={config.explorerCluster}
                />
              ))}
            </div>
          </section>
        )}

        {tab === "tokens" && (
          <TokensPanel
            onSelect={toggleMint}
            selectedMints={selectedMints}
            cluster={config.cluster}
            explorerCluster={config.explorerCluster}
          />
        )}

        {tab === "etf" && (
          <div className="space-y-6">
            <TokensPanel
              onSelect={toggleMint}
              selectedMints={selectedMints}
              cluster={config.cluster}
              explorerCluster={config.explorerCluster}
            />
            <CreateEtfPanel
              selectedMints={selectedMints}
              onClearSelection={clearSelection}
              config={config}
            />
          </div>
        )}

        {tab === "withdraw_etf" && <WithdrawPanel config={config} />}

        {tab === "withdraw_sol" && <WithdrawSolPanel config={config} />}

        {tab === "pools" && (
          <PoolsPanel config={config} onPickPool={pickPool} />
        )}

        {tab === "pfmm" && (
          <div className="space-y-6">
            <TokensPanel
              onSelect={toggleMint}
              selectedMints={selectedMints}
              cluster={config.cluster}
              explorerCluster={config.explorerCluster}
            />
            <PfmmPanel
              selectedMints={selectedMints}
              walletDecimals={walletDecimals}
              config={config}
            />
          </div>
        )}

        <Footer />
      </div>
    </main>
  );
}

function Header({ config }: { config: ClusterConfig }) {
  return (
    <header className="space-y-2">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold tracking-tight">
          Axis AMM <span className="text-slate-400">— cluster demo</span>
        </h1>
        <span className="rounded-full bg-slate-800 px-3 py-1 font-mono text-xs uppercase tracking-wider text-slate-400">
          {config.label}
        </span>
      </div>
      <p className="text-sm text-slate-400">
        Live state of the Axis mainnet-scope deploys.
      </p>
    </header>
  );
}

function ClusterSwitch({
  config,
  onChange,
}: {
  config: ClusterConfig;
  onChange: (cluster: Cluster) => void;
}) {
  return (
    <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-800 bg-slate-900/50 p-4 text-sm">
      <div>
        <p className="text-xs uppercase tracking-wider text-slate-500">Cluster</p>
        <p className="font-mono text-slate-200">{config.rpcUrl}</p>
      </div>
      <div className="flex rounded-lg border border-slate-700 bg-slate-950/70 p-1">
        {(["devnet", "mainnet"] as const).map((cluster) => {
          const active = config.cluster === cluster;
          return (
            <button
              key={cluster}
              onClick={() => onChange(cluster)}
              className={
                "rounded-md px-3 py-1.5 text-xs font-semibold transition " +
                (active
                  ? "bg-indigo-600 text-white"
                  : "text-slate-300 hover:bg-slate-800")
              }
            >
              {cluster === "mainnet" ? "Mainnet + Jupiter" : "Devnet"}
            </button>
          );
        })}
      </div>
    </section>
  );
}

function Tabs({
  current,
  onChange,
  basketSize,
}: {
  current: Tab;
  onChange: (t: Tab) => void;
  basketSize: number;
}) {
  const tabs: Array<{ id: Tab; label: string; hint?: string }> = [
    { id: "overview", label: "Overview" },
    { id: "tokens", label: "Tokens" },
    { id: "etf", label: "Create ETF" },
    { id: "withdraw_etf", label: "Withdraw ETF" },
    { id: "withdraw_sol", label: "Withdraw → SOL" },
    { id: "pools", label: "Pools" },
    { id: "pfmm", label: "PFMM" },
  ];
  return (
    <nav className="flex flex-wrap items-center gap-2 rounded-lg border border-slate-800 bg-slate-900/40 p-1">
      {tabs.map((t) => {
        const active = current === t.id;
        return (
          <button
            key={t.id}
            onClick={() => onChange(t.id)}
            className={
              "rounded-md px-3 py-1.5 text-xs font-medium transition " +
              (active
                ? "bg-indigo-600 text-white"
                : "text-slate-300 hover:bg-slate-800")
            }
          >
            {t.label}
          </button>
        );
      })}
      {basketSize > 0 && (
        <span className="ml-auto rounded-full bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-300">
          {basketSize} mint{basketSize === 1 ? "" : "s"} picked
        </span>
      )}
    </nav>
  );
}

function Footer() {
  return (
    <footer className="pt-8 text-center text-xs text-slate-500">
      <p>
        Source:{" "}
        <a
          href="https://github.com/Axis-pizza/Axis_AMM"
          target="_blank"
          rel="noopener noreferrer"
          className="underline hover:text-slate-300"
        >
          github.com/Axis-pizza/Axis_AMM
        </a>
        {" "}· Built with{" "}
        <a
          href="https://github.com/anza-xyz/pinocchio"
          target="_blank"
          rel="noopener noreferrer"
          className="underline hover:text-slate-300"
        >
          pinocchio
        </a>
      </p>
    </footer>
  );
}

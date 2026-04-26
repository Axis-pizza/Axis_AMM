import { AppWalletProvider } from "./components/WalletProvider";
import { WalletPanel } from "./components/WalletPanel";
import { ProgramCard } from "./components/ProgramCard";
import { ScopeNote } from "./components/ScopeNote";
import { PROGRAMS, NETWORK, RPC_URL } from "./lib/programs";

export default function App() {
  return (
    <AppWalletProvider>
      <main className="min-h-screen px-6 py-12">
        <div className="mx-auto max-w-5xl space-y-8">
          <Header />
          <ScopeNote />
          <WalletPanel />
          <section className="space-y-4">
            <h2 className="text-lg font-semibold">Deployed programs</h2>
            <div className="grid gap-4 md:grid-cols-2">
              {PROGRAMS.map((p) => (
                <ProgramCard key={p.address.toBase58()} program={p} />
              ))}
            </div>
          </section>
          <Footer />
        </div>
      </main>
    </AppWalletProvider>
  );
}

function Header() {
  return (
    <header className="space-y-2">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold tracking-tight">
          Axis AMM <span className="text-slate-400">— devnet demo</span>
        </h1>
        <span className="rounded-full bg-slate-800 px-3 py-1 font-mono text-xs uppercase tracking-wider text-slate-400">
          {NETWORK}
        </span>
      </div>
      <p className="text-sm text-slate-400">
        Live state of the kidney-owned mainnet-scope deploy. RPC:{" "}
        <span className="font-mono text-slate-300">{RPC_URL}</span>
      </p>
    </header>
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

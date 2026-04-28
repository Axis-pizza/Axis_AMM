/// Static disclaimer + scope summary for visitors. Mirrors the
/// language in `docs/architecture/MAINNET_SCOPE.md` so a casual demo
/// viewer immediately understands what's shipping vs what's research.
export function ScopeNote({ cluster }: { cluster: "devnet" | "mainnet" }) {
  const isMainnet = cluster === "mainnet";

  return (
    <section className={`rounded-xl border p-5 text-sm ${
      isMainnet
        ? "border-rose-900/50 bg-rose-950/20 text-rose-100/90"
        : "border-amber-900/40 bg-amber-950/20 text-amber-100/90"
    }`}>
      <h2 className="mb-2 text-base font-semibold text-amber-200">
        {isMainnet ? "Mainnet demo mode" : "Devnet research demo"}
      </h2>
      <p className="mb-3">
        {isMainnet
          ? "These programs are un-audited. Mainnet mode uses live program IDs and Jupiter routes. Treat every signature as real fund movement."
          : "These programs are un-audited and run on devnet only. Do not deposit real funds."}{" "}
        Mainnet v1 ships the two programs marked{" "}
        <span className="rounded bg-indigo-500/20 px-1.5 py-0.5 font-mono text-[11px] uppercase text-indigo-300">
          MAINNET v1
        </span>{" "}
        below.
      </p>
      <p className="text-xs text-amber-200/70">
        Scope source of truth:{" "}
        <a
          href="https://github.com/Axis-pizza/Axis_AMM/blob/main/docs/architecture/MAINNET_SCOPE.md"
          target="_blank"
          rel="noopener noreferrer"
          className="underline hover:text-amber-200"
        >
          docs/architecture/MAINNET_SCOPE.md
        </a>
      </p>
    </section>
  );
}

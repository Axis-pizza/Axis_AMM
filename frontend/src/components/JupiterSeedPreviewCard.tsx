import { useEffect, useState } from "react";
import {
  buildJupiterSeedPreview,
  createMockJupiterQuoteClient,
  liveJupiterQuoteClient,
  type JupiterQuoteMode,
  type JupiterSeedPreview,
} from "../lib/jupiterSeed";
import { truncatePubkey } from "../lib/format";

interface PreviewBasketRow {
  mint: string;
  weight: number;
}

export function JupiterSeedPreviewCard({
  basket,
  weightsOk,
  solSeed,
  slippageBps,
}: {
  basket: PreviewBasketRow[];
  weightsOk: boolean;
  solSeed: number;
  slippageBps: number;
}) {
  const [quoteMode, setQuoteMode] = useState<JupiterQuoteMode>("mock");
  const [previewState, setPreviewState] = useState<"idle" | "pending" | "ok" | "err">(
    "idle",
  );
  const [previewMsg, setPreviewMsg] = useState("");
  const [jupiterPreview, setJupiterPreview] = useState<JupiterSeedPreview | null>(null);
  const weightsKey = basket.map((r) => `${r.mint}:${r.weight}`).join(",");

  useEffect(() => {
    setJupiterPreview(null);
    setPreviewMsg("");
    setPreviewState("idle");
  }, [weightsKey, solSeed, slippageBps, quoteMode]);

  async function previewJupiterSeed() {
    if (!weightsOk) return;
    setPreviewState("pending");
    setPreviewMsg("");
    setJupiterPreview(null);
    try {
      const preview = await buildJupiterSeedPreview({
        basketMints: basket.map((r) => r.mint),
        weights: basket.map((r) => r.weight),
        solIn: BigInt(Math.floor(solSeed * 1_000_000_000)),
        slippageBps,
        quoteClient:
          quoteMode === "mock" ? createMockJupiterQuoteClient() : liveJupiterQuoteClient,
      });
      setJupiterPreview(preview);
      setPreviewState("ok");
    } catch (e) {
      setPreviewState("err");
      setPreviewMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div className="rounded-lg border border-sky-900/60 bg-sky-950/20 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="font-medium text-sky-200">
            Jupiter → axis-vault seed preview
          </p>
          <p className="text-[11px] text-slate-400">
            Mock preview never signs. Run flow still builds live Jupiter
            instructions for the real tx.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <select
            value={quoteMode}
            onChange={(e) => setQuoteMode(e.target.value as JupiterQuoteMode)}
            className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          >
            <option value="mock">mock quotes</option>
            <option value="live">live Jupiter</option>
          </select>
          <button
            type="button"
            onClick={previewJupiterSeed}
            disabled={!weightsOk || solSeed <= 0 || previewState === "pending"}
            className="rounded-md border border-sky-700 px-2 py-1 font-medium text-sky-200 hover:border-sky-500 disabled:opacity-50"
          >
            {previewState === "pending" ? "previewing…" : "Preview"}
          </button>
        </div>
      </div>

      {previewState === "err" && (
        <p className="mt-2 break-all text-rose-400">✗ {previewMsg}</p>
      )}
      {jupiterPreview && (
        <div className="mt-3 space-y-2">
          <div className="grid gap-2 sm:grid-cols-3">
            <Metric label="mode" value={jupiterPreview.mode} />
            <Metric
              label="SOL in"
              value={`${formatLamports(jupiterPreview.solIn)} SOL`}
            />
            <Metric
              label="deposit floor"
              value={jupiterPreview.depositAmount.toString()}
            />
          </div>
          <ul className="space-y-1">
            {jupiterPreview.legs.map((leg, i) => (
              <li
                key={leg.mint.toBase58()}
                className={
                  "grid gap-2 rounded border px-2 py-1 sm:grid-cols-[1fr_auto_auto] " +
                  (i === jupiterPreview.bottleneckIndex
                    ? "border-amber-500/40 bg-amber-500/10"
                    : "border-slate-800 bg-slate-950/40")
                }
              >
                <span className="font-mono text-slate-300">
                  {truncatePubkey(leg.mint.toBase58(), 6, 6)}
                  <span className="ml-2 text-slate-500">
                    {leg.weightBps / 100}% · {leg.routeLabel}
                  </span>
                </span>
                <span className="font-mono text-slate-400">
                  {formatLamports(leg.solLamports)} SOL
                </span>
                <span className="font-mono text-slate-300">
                  min {leg.minOut.toString()}
                </span>
              </li>
            ))}
          </ul>
        </div>
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

function formatLamports(lamports: bigint): string {
  const whole = lamports / 1_000_000_000n;
  const frac = (lamports % 1_000_000_000n).toString().padStart(9, "0");
  return `${whole}.${frac}`.replace(/\.?0+$/, "");
}

import type { PublicKey } from "@solana/web3.js";
import { explorerAddr } from "../lib/tx";
import { truncatePubkey } from "../lib/format";

export interface PoolView {
  exists: boolean;
  pool: PublicKey;
  windowEnd?: bigint;
}

export function PoolStatus({
  pool,
  currentSlot,
  windowOpen,
  slotsLeft,
  explorerCluster,
}: {
  pool: PoolView | null;
  currentSlot: bigint | null;
  windowOpen: boolean;
  slotsLeft: number;
  explorerCluster: "devnet" | "";
}) {
  return (
    <div className="rounded bg-slate-950/60 p-3 text-xs">
      <p className="mb-1 text-slate-400">Pool PDA</p>
      <a
        href={pool ? explorerAddr(pool.pool.toBase58(), explorerCluster) : "#"}
        target="_blank"
        rel="noreferrer"
        className="font-mono text-slate-200 hover:text-indigo-300"
      >
        {pool ? truncatePubkey(pool.pool.toBase58(), 8, 8) : "..."}
      </a>
      <p className="mt-2 text-slate-400">
        Status:{" "}
        {pool === null
          ? "checking..."
          : pool.exists
            ? `initialized · window ends slot ${pool.windowEnd?.toString()} (${
                windowOpen ? `${slotsLeft} slots left` : "ended - clearable"
              })`
            : "not initialized - call InitPool"}
      </p>
      {currentSlot !== null && (
        <p className="text-slate-500">current slot: {currentSlot.toString()}</p>
      )}
    </div>
  );
}

export function InitPoolForm({
  feeBps,
  setFeeBps,
  windowSlots,
  setWindowSlots,
  initPool,
  stage,
}: {
  feeBps: number;
  setFeeBps: (n: number) => void;
  windowSlots: number;
  setWindowSlots: (n: number) => void;
  initPool: () => void;
  stage: string;
}) {
  return (
    <div className="rounded border border-slate-800 p-3">
      <p className="mb-2 text-xs uppercase text-slate-400">InitializePool</p>
      <div className="flex items-end gap-3 text-xs">
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">fee bps</span>
          <input
            type="number"
            value={feeBps}
            onChange={(e) => setFeeBps(Number(e.target.value))}
            className="w-20 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">window slots</span>
          <input
            type="number"
            value={windowSlots}
            onChange={(e) => setWindowSlots(Number(e.target.value))}
            className="w-24 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <button
          onClick={initPool}
          disabled={stage !== "idle"}
          className="rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
        >
          {stage === "init" ? "init..." : "InitPool"}
        </button>
      </div>
    </div>
  );
}

export function AddLiquidityForm({
  liquidityUi,
  setLiquidityUi,
  addLiquidity,
  disabled,
  stage,
}: {
  liquidityUi: number;
  setLiquidityUi: (n: number) => void;
  addLiquidity: () => void;
  disabled: boolean;
  stage: string;
}) {
  return (
    <div className="rounded border border-slate-800 p-3">
      <p className="mb-2 text-xs uppercase text-slate-400">AddLiquidity</p>
      <div className="flex items-end gap-3 text-xs">
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">amount per side (UI)</span>
          <input
            type="number"
            value={liquidityUi}
            onChange={(e) => setLiquidityUi(Number(e.target.value))}
            className="w-32 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <button
          onClick={addLiquidity}
          disabled={disabled}
          className="rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
        >
          {stage === "addLiq" ? "adding..." : "AddLiquidity"}
        </button>
      </div>
    </div>
  );
}

export function SwapRequestForm({
  swapInIdx,
  setSwapInIdx,
  swapOutIdx,
  setSwapOutIdx,
  swapAmountUi,
  setSwapAmountUi,
  swapRequest,
  stage,
}: {
  swapInIdx: number;
  setSwapInIdx: (n: number) => void;
  swapOutIdx: number;
  setSwapOutIdx: (n: number) => void;
  swapAmountUi: number;
  setSwapAmountUi: (n: number) => void;
  swapRequest: () => void;
  stage: string;
}) {
  return (
    <div className="rounded border border-slate-800 p-3">
      <p className="mb-2 text-xs uppercase text-slate-400">SwapRequest</p>
      <div className="flex items-end gap-3 text-xs">
        <IndexSelect label="in idx" value={swapInIdx} onChange={setSwapInIdx} />
        <IndexSelect label="out idx" value={swapOutIdx} onChange={setSwapOutIdx} />
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">amount in (UI)</span>
          <input
            type="number"
            value={swapAmountUi}
            onChange={(e) => setSwapAmountUi(Number(e.target.value))}
            className="w-24 rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <button
          onClick={swapRequest}
          disabled={stage !== "idle"}
          className="rounded-lg bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-500 disabled:opacity-50"
        >
          {stage === "swap" ? "queueing..." : "SwapRequest"}
        </button>
      </div>
    </div>
  );
}

function IndexSelect({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
}) {
  return (
    <label className="flex flex-col">
      <span className="mb-1 text-slate-400">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="rounded bg-slate-800 px-2 py-1"
      >
        <option value={0}>0</option>
        <option value={1}>1</option>
        <option value={2}>2</option>
      </select>
    </label>
  );
}

export function ClearClaimButtons({
  clearBatch,
  claim,
  windowOpen,
  stage,
}: {
  clearBatch: () => void;
  claim: () => void;
  windowOpen: boolean;
  stage: string;
}) {
  return (
    <div className="flex gap-2">
      <button
        onClick={clearBatch}
        disabled={stage !== "idle" || windowOpen}
        title={windowOpen ? "window not ended yet" : "settles the active batch"}
        className="rounded-lg bg-rose-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-rose-500 disabled:opacity-50"
      >
        {stage === "clear" ? "clearing..." : "ClearBatch"}
      </button>
      <button
        onClick={claim}
        disabled={stage !== "idle"}
        className="rounded-lg bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
      >
        {stage === "claim" ? "claiming..." : "Claim"}
      </button>
    </div>
  );
}

export function WithdrawFeesForm({
  amount0,
  setAmount0,
  amount1,
  setAmount1,
  amount2,
  setAmount2,
  withdrawFees,
  stage,
  disabled,
}: {
  amount0: number;
  setAmount0: (n: number) => void;
  amount1: number;
  setAmount1: (n: number) => void;
  amount2: number;
  setAmount2: (n: number) => void;
  withdrawFees: () => void;
  stage: string;
  disabled?: boolean;
}) {
  const total = amount0 + amount1 + amount2;
  return (
    <div className="rounded border border-amber-900/50 bg-amber-950/20 p-3">
      <p className="mb-2 text-xs font-medium text-amber-200">
        WithdrawFees (authority only)
      </p>
      <p className="mb-2 text-[11px] text-slate-400">
        Pulls vault tokens to the treasury. Decrements <code>pool.reserves</code>{" "}
        in the same ix so the clearing-price math stays consistent — bigger
        than your true accumulated fee withdraws real LP liquidity.
      </p>
      <div className="grid grid-cols-3 gap-2 text-xs">
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">vault[0] (UI)</span>
          <input
            type="number"
            min={0}
            step={0.001}
            value={amount0}
            onChange={(e) => setAmount0(Number(e.target.value))}
            className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">vault[1] (UI)</span>
          <input
            type="number"
            min={0}
            step={0.001}
            value={amount1}
            onChange={(e) => setAmount1(Number(e.target.value))}
            className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
        <label className="flex flex-col">
          <span className="mb-1 text-slate-400">vault[2] (UI)</span>
          <input
            type="number"
            min={0}
            step={0.001}
            value={amount2}
            onChange={(e) => setAmount2(Number(e.target.value))}
            className="rounded bg-slate-800 px-2 py-1 font-mono text-slate-100"
          />
        </label>
      </div>
      <button
        onClick={withdrawFees}
        disabled={disabled || stage !== "idle" || total <= 0}
        className="mt-2 rounded-lg bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-500 disabled:opacity-50"
      >
        {stage === "withdrawFees" ? "withdrawing..." : "WithdrawFees"}
      </button>
    </div>
  );
}

export function PausedToggle({
  paused,
  setPaused,
  stage,
}: {
  paused: boolean;
  setPaused: (p: boolean) => void;
  stage: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-3 rounded border border-rose-900/50 bg-rose-950/20 p-3 text-xs">
      <span className="font-medium text-rose-200">SetPaused (authority only)</span>
      <button
        onClick={() => setPaused(true)}
        disabled={stage !== "idle"}
        className="rounded-lg bg-rose-700 px-3 py-1.5 font-medium text-white hover:bg-rose-600 disabled:opacity-50"
      >
        {stage === "pause" ? "pausing..." : paused ? "(already paused)" : "Pause"}
      </button>
      <button
        onClick={() => setPaused(false)}
        disabled={stage !== "idle"}
        className="rounded-lg bg-emerald-700 px-3 py-1.5 font-medium text-white hover:bg-emerald-600 disabled:opacity-50"
      >
        {stage === "unpause" ? "unpausing..." : "Resume"}
      </button>
      <span className="text-slate-400">
        Halts SwapRequest / ClearBatch / AddLiquidity until resumed.
      </span>
    </div>
  );
}

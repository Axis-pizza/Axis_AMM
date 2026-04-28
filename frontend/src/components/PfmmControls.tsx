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

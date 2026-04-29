import {
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  Connection,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createCloseAccountInstruction,
  createSyncNativeInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  SOL_MINT,
  deserializeIx,
  fetchAltAccounts,
  getQuote,
  getSwapInstructions,
  type JupiterQuoteResponse,
} from "./jupiter";
import { tryCompileV0 } from "./depositSolPlan";

/// Plans a single v0 transaction that takes `solIn` lamports and lands
/// proportional amounts of `outputMints[i]` in the user's basket ATAs
/// via Jupiter. Used by PfmmPanel to seed PFMM AddLiquidity / SwapRequest
/// flows on mainnet, where users are expected to arrive holding only SOL.
///
/// Flow inside the produced tx:
///   1. compute-budget (limit + price)
///   2. idempotent ATA creates (user wSOL + each output mint)
///   3. SystemProgram.transfer SOL → user wSOL ATA + syncNative
///   4. for each non-wSOL leg: Jupiter setupIxs + swap + cleanupIx
///      (de-duplicated across legs so the shared-accounts setup runs once)
///   5. optional close wSOL ATA (returns rent + dust to native SOL)
///
/// wSOL legs are special-cased: we don't route through Jupiter, just wrap
/// directly into the user's basket ATA for that mint. Saves both an ALT
/// lookup and Jupiter's per-leg fee.
const FALLBACK_PRIORITY_MICRO_LAMPORTS = 50_000;
const SOLANA_MAX_TX_CU = 1_400_000;

export interface JupiterSolSeedArgs {
  conn: Connection;
  user: PublicKey;
  /// Output mints (1..3 in practice; tested up to N=3).
  outputMints: PublicKey[];
  /// Allocation in bps; sum must == 10_000. Length must match outputMints.
  weights: number[];
  /// Total SOL to spend (lamports).
  solIn: bigint;
  /// Per-leg slippage. Default 50 bps.
  slippageBps?: number;
  /// Cap on accounts per leg's Jupiter swap ix. Default 16.
  maxAccounts?: number;
  /// If true, close the user's wSOL ATA at the end (only when the user
  /// had no pre-existing wSOL balance). Default true.
  closeWsolAtEnd?: boolean;
  /// Per-CU priority fee in microlamports.
  priorityMicroLamports?: number;
}

export interface JupiterSolSeedLeg {
  mint: PublicKey;
  weightBps: number;
  solLamports: bigint;
  /// Direct wrap (for wSOL output) — no Jupiter quote.
  isDirectWrap: boolean;
  /// Quote from Jupiter (only set on Jupiter legs).
  quote?: JupiterQuoteResponse;
  expectedOut: bigint;
  minOut: bigint;
  routeLabel: string;
  /// User's basket ATA — destination of the swap / wrap.
  userBasketAta: PublicKey;
}

export interface JupiterSolSeedPlan {
  versionedTx: VersionedTransaction;
  altAccounts: AddressLookupTableAccount[];
  legs: JupiterSolSeedLeg[];
  ixCount: number;
  txBytes: number;
  computeUnitLimit: number;
  computeUnitPrice: number;
}

function splitWeightedLamports(solIn: bigint, weights: number[]): bigint[] {
  const legs = weights.map((w) => (solIn * BigInt(w)) / 10_000n);
  const assigned = legs.reduce((sum, lamports) => sum + lamports, 0n);
  legs[legs.length - 1] += solIn - assigned;
  return legs;
}

function extractRouteLabel(quote: JupiterQuoteResponse): string {
  const first = quote.routePlan[0] as
    | { swapInfo?: { label?: string } }
    | undefined;
  return first?.swapInfo?.label ?? "Jupiter";
}

interface DecodedComputeBudget {
  cuLimit: number | null;
  microLamportsPerCu: number | null;
}

function decodeComputeBudgetIx(ix: TransactionInstruction): DecodedComputeBudget {
  const data = ix.data;
  if (data.length === 0) return { cuLimit: null, microLamportsPerCu: null };
  if (data[0] === 0x02 && data.length >= 5) {
    return { cuLimit: data.readUInt32LE(1), microLamportsPerCu: null };
  }
  if (data[0] === 0x03 && data.length >= 9) {
    const lo = data.readUInt32LE(1);
    const hi = data.readUInt32LE(5);
    return { cuLimit: null, microLamportsPerCu: lo + hi * 0x1_0000_0000 };
  }
  return { cuLimit: null, microLamportsPerCu: null };
}

function ixDedupKey(ix: TransactionInstruction): string {
  return [
    ix.programId.toBase58(),
    ix.keys
      .map(
        (k) => `${k.pubkey.toBase58()}:${k.isSigner ? 1 : 0}:${k.isWritable ? 1 : 0}`,
      )
      .join("|"),
    ix.data.toString("base64"),
  ].join("#");
}

export async function buildJupiterSolSeedPlan(
  args: JupiterSolSeedArgs,
): Promise<JupiterSolSeedPlan> {
  const n = args.outputMints.length;
  if (n < 1 || n > 5) {
    throw new Error(`outputMints length must be 1..5, got ${n}`);
  }
  if (n !== args.weights.length) {
    throw new Error("outputMints / weights length mismatch");
  }
  const weightSum = args.weights.reduce((a, b) => a + b, 0);
  if (weightSum !== 10_000) {
    throw new Error(`weights must sum to 10_000, got ${weightSum}`);
  }
  if (args.solIn <= 0n) {
    throw new Error("SOL input must be greater than zero");
  }

  // Pre-flight: balance must cover SOL spend + wSOL ATA rent + tx fee
  // headroom. Mirrors the floor depositSolPlan uses.
  const RENT_WSOL_ATA = 2_039_280n;
  const RESERVE_FOR_FEES = 5_000_000n;
  const balanceLamports = BigInt(
    await args.conn.getBalance(args.user, "confirmed"),
  );
  if (balanceLamports < args.solIn + RENT_WSOL_ATA + RESERVE_FOR_FEES) {
    throw new Error(
      `Insufficient SOL: need ${(
        args.solIn + RENT_WSOL_ATA + RESERVE_FOR_FEES
      ).toString()} lamports (spend + wSOL rent + fee reserve), have ${balanceLamports.toString()}`,
    );
  }

  const slippageBps = args.slippageBps ?? 50;
  const maxAccounts = args.maxAccounts ?? 16;
  const closeWsolAtEnd = args.closeWsolAtEnd ?? true;

  const userWsolAta = getAssociatedTokenAddressSync(
    SOL_MINT,
    args.user,
    false,
  );
  const userBasketAtas = args.outputMints.map((mint) =>
    getAssociatedTokenAddressSync(mint, args.user, false),
  );

  const wsolInfo = await args.conn.getAccountInfo(userWsolAta, "confirmed");
  let preExistingWsolBalance = 0n;
  if (wsolInfo) {
    try {
      const bal = await args.conn.getTokenAccountBalance(userWsolAta, "confirmed");
      preExistingWsolBalance = BigInt(bal.value.amount);
    } catch {
      preExistingWsolBalance = 0n;
    }
  }
  const finalCloseWsol = closeWsolAtEnd && preExistingWsolBalance === 0n;

  const legLamports = splitWeightedLamports(args.solIn, args.weights);

  // Quote per leg, skipping wSOL legs (those are direct wraps and use no
  // Jupiter route; we record a synthetic 1:1 leg for parity in the result).
  const legs: JupiterSolSeedLeg[] = [];
  const jupiterLegIndices: number[] = [];
  for (let i = 0; i < n; i++) {
    const mint = args.outputMints[i];
    const lamports = legLamports[i];
    const userBasketAta = userBasketAtas[i];
    if (mint.equals(SOL_MINT)) {
      legs.push({
        mint,
        weightBps: args.weights[i],
        solLamports: lamports,
        isDirectWrap: true,
        expectedOut: lamports,
        minOut: lamports,
        routeLabel: "wrap",
        userBasketAta,
      });
      continue;
    }
    let quote: JupiterQuoteResponse;
    try {
      quote = await getQuote({
        inputMint: SOL_MINT,
        outputMint: mint,
        amount: lamports,
        slippageBps,
        swapMode: "ExactIn",
        maxAccounts,
      });
    } catch (e) {
      throw new Error(
        `Jupiter quote failed for leg ${i} (${mint.toBase58().slice(0, 8)}…): ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
    legs.push({
      mint,
      weightBps: args.weights[i],
      solLamports: lamports,
      isDirectWrap: false,
      quote,
      expectedOut: BigInt(quote.outAmount),
      minOut: BigInt(quote.otherAmountThreshold),
      routeLabel: extractRouteLabel(quote),
      userBasketAta,
    });
    jupiterLegIndices.push(i);
  }

  // Per-leg swap-instructions for Jupiter legs. wrapAndUnwrapSol must be
  // FALSE because we wrap once into userWsolAta below; otherwise leg N's
  // setup would create a fresh wSOL ATA that conflicts with leg N+1's read.
  const swapBundles = await Promise.all(
    jupiterLegIndices.map(async (i) => {
      const quote = legs[i].quote!;
      try {
        return await getSwapInstructions({
          quote,
          userPublicKey: args.user,
          destinationTokenAccount: userBasketAtas[i],
          wrapAndUnwrapSol: false,
        });
      } catch (e) {
        throw new Error(
          `Jupiter swap-instructions failed for leg ${i} (${args.outputMints[i].toBase58().slice(0, 8)}…): ${
            e instanceof Error ? e.message : String(e)
          }`,
        );
      }
    }),
  );

  const legBudgets = swapBundles.map((b) =>
    b.computeBudgetInstructions.map((raw) =>
      decodeComputeBudgetIx(deserializeIx(raw)),
    ),
  );
  let cuSum = 0;
  let microLamportsMax = 0;
  for (const leg of legBudgets) {
    for (const item of leg) {
      if (item.cuLimit !== null) cuSum += item.cuLimit;
      if (
        item.microLamportsPerCu !== null &&
        item.microLamportsPerCu > microLamportsMax
      ) {
        microLamportsMax = item.microLamportsPerCu;
      }
    }
  }
  const cuLimit = Math.min(
    SOLANA_MAX_TX_CU,
    Math.max(400_000, cuSum + 60_000),
  );
  const cuPrice =
    args.priorityMicroLamports !== undefined
      ? args.priorityMicroLamports
      : Math.max(microLamportsMax, FALLBACK_PRIORITY_MICRO_LAMPORTS);

  const ataCreates: TransactionInstruction[] = [
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      userWsolAta,
      args.user,
      SOL_MINT,
    ),
    ...args.outputMints.map((mint, i) =>
      createAssociatedTokenAccountIdempotentInstruction(
        args.user,
        userBasketAtas[i],
        args.user,
        mint,
      ),
    ),
  ];

  const wrapIxs = [
    SystemProgram.transfer({
      fromPubkey: args.user,
      toPubkey: userWsolAta,
      lamports: Number(args.solIn),
    }),
    createSyncNativeInstruction(userWsolAta, TOKEN_PROGRAM_ID),
  ];

  // For each wSOL output leg, transfer the leg's lamports from the wSOL
  // ATA to that mint's user basket ATA — i.e. just move within the SPL
  // realm. Easier: skip — wSOL ATA == user's basket ATA for SOL_MINT,
  // because both derive from (mint=SOL, owner=user). But we registered
  // userBasketAtas[i] separately, which is the *same* address. So the
  // wrap above already lands the lamports into userBasketAtas[i] for any
  // wSOL leg. Only the leg's `solLamports` needs accounting against the
  // remaining wSOL balance for the Jupiter legs — the SystemProgram.transfer
  // covered the entire `args.solIn`, which already sums correctly.
  // Net effect: no extra ix needed for wSOL output legs.

  const swapIxs: TransactionInstruction[] = [];
  const seen = new Set<string>();
  const pushDedup = (target: TransactionInstruction[], ix: TransactionInstruction) => {
    const key = ixDedupKey(ix);
    if (!seen.has(key)) {
      seen.add(key);
      target.push(ix);
    }
  };
  for (const bundle of swapBundles) {
    for (const raw of bundle.setupInstructions)
      pushDedup(swapIxs, deserializeIx(raw));
    pushDedup(swapIxs, deserializeIx(bundle.swapInstruction));
    if (bundle.cleanupInstruction) {
      pushDedup(swapIxs, deserializeIx(bundle.cleanupInstruction));
    }
  }

  const closeWsolIxs = finalCloseWsol
    ? [
        createCloseAccountInstruction(
          userWsolAta,
          args.user,
          args.user,
          [],
          TOKEN_PROGRAM_ID,
        ),
      ]
    : [];

  const altAccounts = await fetchAltAccounts(
    args.conn,
    swapBundles.flatMap((b) => b.addressLookupTableAddresses),
  );
  const { blockhash } = await args.conn.getLatestBlockhash("confirmed");

  const ixs = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: cuLimit }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: cuPrice }),
    ...ataCreates,
    ...wrapIxs,
    ...swapIxs,
    ...closeWsolIxs,
  ];
  const attempt = tryCompileV0(args.user, blockhash, ixs, altAccounts);
  if (!attempt.ok) {
    throw new Error(
      `Jupiter SOL seed tx blew the 1232-byte wire cap ` +
        `(estimated ${attempt.bytes ?? "?"} bytes; ix count ${ixs.length}; ` +
        `static keys ${attempt.staticKeys ?? "?"}; ALT addresses ${altAccounts.length}). ` +
        `Try a smaller leg count, lower the per-leg \`maxAccounts\` (currently ${maxAccounts}), ` +
        `or pick mints with simpler Jupiter routes. Underlying error: ${attempt.error}`,
    );
  }

  return {
    versionedTx: new VersionedTransaction(attempt.message),
    altAccounts,
    legs,
    ixCount: ixs.length,
    txBytes: attempt.bytes,
    computeUnitLimit: cuLimit,
    computeUnitPrice: cuPrice,
  };
}

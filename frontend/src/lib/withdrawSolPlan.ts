import {
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  Connection,
  PublicKey,
  TransactionInstruction,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createCloseAccountInstruction,
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
import { ixWithdraw } from "./ix";
import {
  expectedWithdrawOutputs,
  fetchEtfState,
  fetchVaultBalances,
  type EtfStateData,
} from "./etfState";
import { SOLANA_MAX_TX_CU, tryCompileV0 } from "./depositSolPlan";

const FALLBACK_PRIORITY_MICRO_LAMPORTS = 50_000;

export interface WithdrawSolPlanArgs {
  conn: Connection;
  user: PublicKey;
  programId: PublicKey;
  etfState: PublicKey;
  /// Pre-fetched state. Skip the network fetch when the panel already
  /// has it cached (e.g. on second click). Pass `undefined` and we
  /// load it via `fetchEtfState`.
  etfStateData?: EtfStateData;
  burnAmount: bigint;
  /// Bps to shrink the per-leg Jupiter quote amount by. Protects against
  /// minor `total_supply` / vault-balance drift between plan-build and
  /// settlement; leftover basket dust stays in the user's basket ATA
  /// for them to handle manually. Default 100 bps (1 %).
  safetyShrinkBps?: number;
  slippageBps?: number;
  /// Cap on accounts per leg's Jupiter swap ix. Default 16; lower
  /// values force simpler routes that are more likely to fit.
  maxAccounts?: number;
  priorityMicroLamports?: number;
}

export interface WithdrawSolLegPreview {
  mint: PublicKey;
  vault: PublicKey;
  expectedBasketOut: bigint;
  quotedBasketIn: bigint;
  quote: JupiterQuoteResponse;
  expectedSolOut: bigint;
  minSolOut: bigint;
  routeLabel: string;
}

export interface WithdrawSolPlan {
  /// "single" — one v0 tx covering Withdraw + swaps + close.
  /// "split" — Withdraw alone in tx0 lands basket tokens in user ATAs;
  /// tx1 runs the per-leg swaps + close. If the user aborts between,
  /// basket tokens stay in the user's wallet (recoverable).
  mode: "single" | "split";
  versionedTx: VersionedTransaction;
  /// Set on `mode === "split"` only: tx1 (Jupiter swaps + close wSOL).
  swapTx?: VersionedTransaction;
  altAccounts: AddressLookupTableAccount[];
  legs: WithdrawSolLegPreview[];
  feeAmount: bigint;
  effectiveBurn: bigint;
  /// Sum of `expectedBasketOut` across all legs. Used as the on-chain
  /// `min_tokens_out` argument to axis-vault Withdraw — it's a lower
  /// bound on the total basket-token amount the program will hand
  /// back, so we set it tight to catch unexpected divergence.
  totalExpectedBasketOut: bigint;
  expectedSolOut: bigint;
  minSolOut: bigint;
  ixCount: number;
  txBytes: number;
  computeUnitLimit: number;
  computeUnitPrice: number;
}

function extractRouteLabel(quote: JupiterQuoteResponse): string {
  const first = quote.routePlan[0];
  if (
    typeof first === "object" &&
    first !== null &&
    "swapInfo" in first &&
    typeof first.swapInfo === "object" &&
    first.swapInfo !== null &&
    "label" in first.swapInfo &&
    typeof first.swapInfo.label === "string"
  ) {
    return first.swapInfo.label;
  }
  return "Jupiter";
}

export async function buildWithdrawSolPlan(
  args: WithdrawSolPlanArgs,
): Promise<WithdrawSolPlan> {
  if (args.burnAmount <= 0n) throw new Error("burnAmount must be > 0");

  const etf = args.etfStateData ?? (await fetchEtfState(args.conn, args.etfState));
  if (etf.paused) throw new Error("ETF is paused — Withdraw is disabled");
  if (etf.totalSupply === 0n) {
    throw new Error("ETF has no supply — nothing to withdraw");
  }
  if (args.burnAmount > etf.totalSupply) {
    throw new Error(
      `burnAmount ${args.burnAmount} > totalSupply ${etf.totalSupply}`,
    );
  }

  const safetyShrinkBps = args.safetyShrinkBps ?? 100;
  const slippageBps = args.slippageBps ?? 50;
  const maxAccounts = args.maxAccounts ?? 16;

  const vaults = etf.tokenVaults;
  const mints = etf.tokenMints;

  const vaultBalances = await fetchVaultBalances(args.conn, vaults);
  const { feeAmount, effectiveBurn, perLeg } = expectedWithdrawOutputs(
    vaultBalances,
    args.burnAmount,
    etf.totalSupply,
    etf.feeBps,
  );
  const totalExpectedBasketOut = perLeg.reduce((s, v) => s + v, 0n);
  if (totalExpectedBasketOut === 0n) {
    throw new Error("expected basket output is zero — burnAmount too small");
  }

  const userBasketAtas = mints.map((m) => getAssociatedTokenAddressSync(m, args.user, false));
  const userEtfAta = getAssociatedTokenAddressSync(etf.etfMint, args.user, false);
  const userWsolAta = getAssociatedTokenAddressSync(SOL_MINT, args.user, false);
  const treasuryEtfAta = getAssociatedTokenAddressSync(etf.etfMint, etf.treasury, true);

  // Skip the wSOL ATA close when the user already had a non-zero wSOL
  // balance — closing would unwrap their pre-existing wSOL too. The
  // proceeds from this withdraw stay wrapped; the user can manually
  // unwrap later. (Surfaced in the panel so the UX isn't surprising.)
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
  const closeWsolAtEnd = preExistingWsolBalance === 0n;

  // Quote each leg. We shrink the swap input by `safetyShrinkBps` so a
  // tiny drift in totalSupply between now and settlement still leaves
  // enough basket tokens in the user ATA for the swap. Leftover stays
  // for the user to sweep later.
  const legs: WithdrawSolLegPreview[] = [];
  let totalExpectedSol = 0n;
  let totalMinSol = 0n;
  for (let i = 0; i < perLeg.length; i++) {
    const expected = perLeg[i];
    const quotedAmount = (expected * BigInt(10_000 - safetyShrinkBps)) / 10_000n;
    if (quotedAmount === 0n) {
      throw new Error(
        `leg ${i} (${mints[i].toBase58().slice(0, 8)}…) quote amount is zero ` +
          "after safety shrink — burnAmount too small for this basket",
      );
    }
    let quote: JupiterQuoteResponse;
    try {
      quote = await getQuote({
        inputMint: mints[i],
        outputMint: SOL_MINT,
        amount: quotedAmount,
        slippageBps,
        swapMode: "ExactIn",
        maxAccounts,
      });
    } catch (e) {
      throw new Error(
        `Jupiter quote failed on leg ${i} (${mints[i].toBase58().slice(0, 8)}…): ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
    const expectedSolOut = BigInt(quote.outAmount);
    const minSolOut = BigInt(quote.otherAmountThreshold);
    legs.push({
      mint: mints[i],
      vault: vaults[i],
      expectedBasketOut: expected,
      quotedBasketIn: quotedAmount,
      quote,
      expectedSolOut,
      minSolOut,
      routeLabel: extractRouteLabel(quote),
    });
    totalExpectedSol += expectedSolOut;
    totalMinSol += minSolOut;
  }

  // Per-leg swap instructions. Destination is the user's wSOL ATA so
  // the close at the end unwraps the proceeds back to native SOL.
  const swapBundles = await Promise.all(
    legs.map((leg, i) =>
      getSwapInstructions({
        quote: leg.quote,
        userPublicKey: args.user,
        destinationTokenAccount: userWsolAta,
        wrapAndUnwrapSol: false,
      }).catch((e) => {
        throw new Error(
          `Jupiter swap-instructions failed on leg ${i} (${mints[i].toBase58().slice(0, 8)}…): ${
            e instanceof Error ? e.message : String(e)
          }`,
        );
      }),
    ),
  );

  // Compute budget — same logic as deposit plan but expressed inline to
  // avoid a circular import. Sum Jupiter's per-leg CU + headroom for
  // axis Withdraw + ATA creates + close, cap at protocol max.
  let cuSum = 0;
  let microLamportsMax = 0;
  for (const bundle of swapBundles) {
    for (const raw of bundle.computeBudgetInstructions) {
      const ix = deserializeIx(raw);
      if (ix.data[0] === 0x02 && ix.data.length >= 5) {
        cuSum += ix.data.readUInt32LE(1);
      } else if (ix.data[0] === 0x03 && ix.data.length >= 9) {
        const lo = ix.data.readUInt32LE(1);
        const hi = ix.data.readUInt32LE(5);
        const fee = lo + hi * 0x1_0000_0000;
        if (fee > microLamportsMax) microLamportsMax = fee;
      }
    }
  }
  const cuLimit = Math.min(SOLANA_MAX_TX_CU, Math.max(400_000, cuSum + 150_000));
  const cuPrice =
    args.priorityMicroLamports !== undefined
      ? args.priorityMicroLamports
      : Math.max(microLamportsMax, FALLBACK_PRIORITY_MICRO_LAMPORTS);

  // Build ix sequence in segments so we can try the single-tx path
  // first and re-split if it overflows the 1232-byte wire cap.
  const ataIxs = [
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      userWsolAta,
      args.user,
      SOL_MINT,
    ),
    ...mints.map((mint, i) =>
      createAssociatedTokenAccountIdempotentInstruction(
        args.user,
        userBasketAtas[i],
        args.user,
        mint,
      ),
    ),
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      userEtfAta,
      args.user,
      etf.etfMint,
    ),
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      treasuryEtfAta,
      etf.treasury,
      etf.etfMint,
    ),
  ];

  // axis-vault Withdraw — fee transfer + burn + vault → user
  // basket-ATA transfers. min_tokens_out is the SUM of basket outputs;
  // we set it to the precomputed total so the program's pre-transfer
  // guard fires if the on-chain math diverges from our snapshot.
  const withdrawIx = ixWithdraw({
    programId: args.programId,
    payer: args.user,
    etfState: args.etfState,
    etfMint: etf.etfMint,
    userEtfAta,
    treasuryEtfAta,
    vaults,
    userBasketAccounts: userBasketAtas,
    burnAmount: args.burnAmount,
    minTokensOut: totalExpectedBasketOut,
    name: etf.name,
  });

  // Per-leg Jupiter swap basket → wSOL, with a dedup pass so identical
  // shared-account setup ixs don't show up twice.
  const swapIxs: TransactionInstruction[] = [];
  const seen = new Set<string>();
  const pushDedup = (target: TransactionInstruction[], ix: TransactionInstruction) => {
    const key = [
      ix.programId.toBase58(),
      ix.keys
        .map((k) => `${k.pubkey.toBase58()}:${k.isSigner ? 1 : 0}:${k.isWritable ? 1 : 0}`)
        .join("|"),
      ix.data.toString("base64"),
    ].join("#");
    if (!seen.has(key)) {
      seen.add(key);
      target.push(ix);
    }
  };
  for (const bundle of swapBundles) {
    for (const raw of bundle.setupInstructions) pushDedup(swapIxs, deserializeIx(raw));
    pushDedup(swapIxs, deserializeIx(bundle.swapInstruction));
    if (bundle.cleanupInstruction) pushDedup(swapIxs, deserializeIx(bundle.cleanupInstruction));
  }

  const closeWsolIxs = closeWsolAtEnd
    ? [createCloseAccountInstruction(userWsolAta, args.user, args.user, [], TOKEN_PROGRAM_ID)]
    : [];

  const altAccounts = await fetchAltAccounts(
    args.conn,
    swapBundles.flatMap((b) => b.addressLookupTableAddresses),
  );
  const { blockhash } = await args.conn.getLatestBlockhash("confirmed");

  const cbIxs = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: cuLimit }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: cuPrice }),
  ];

  // Try the single-tx path: cb + ATAs + Withdraw + swaps + close.
  const singleIxs = [...cbIxs, ...ataIxs, withdrawIx, ...swapIxs, ...closeWsolIxs];
  const singleAttempt = tryCompileV0(args.user, blockhash, singleIxs, altAccounts);
  if (singleAttempt.ok) {
    return {
      mode: "single",
      versionedTx: new VersionedTransaction(singleAttempt.message),
      altAccounts,
      legs,
      feeAmount,
      effectiveBurn,
      totalExpectedBasketOut,
      expectedSolOut: totalExpectedSol,
      minSolOut: totalMinSol,
      ixCount: singleIxs.length,
      txBytes: singleAttempt.bytes,
      computeUnitLimit: cuLimit,
      computeUnitPrice: cuPrice,
    };
  }

  // Fallback: split.
  // tx0 = cb + (user-side ATA prep) + Withdraw → lands basket tokens
  //   in the user's basket ATAs. No Jupiter accounts, so no ALT need.
  // tx1 = cb + Jupiter swaps + close wSOL → unwraps proceeds to SOL.
  //   The user's basket ATA balance from tx0 funds these swaps.
  // If the user aborts between, basket tokens stay in their wallet
  // (recoverable: re-run withdraw, the panel will see no ETF balance
  // anymore but the basket tokens are spendable / swappable manually).
  const withdrawTxIxs = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: cuPrice }),
    // Only the ATAs Withdraw actually reads (user ETF, basket, treasury).
    // wSOL ATA is created in tx1 where it's actually used.
    ...ataIxs.filter((ix) =>
      // Drop the wSOL ATA create from tx0; it lives in tx1 alongside
      // the swaps that need it.
      !ix.keys.some(
        (k) =>
          k.pubkey.equals(userWsolAta) && k.isWritable && !k.isSigner,
      ),
    ),
    withdrawIx,
  ];
  const swapTxIxs = [
    ...cbIxs,
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      userWsolAta,
      args.user,
      SOL_MINT,
    ),
    ...swapIxs,
    ...closeWsolIxs,
  ];

  const withdrawAttempt = tryCompileV0(args.user, blockhash, withdrawTxIxs, []);
  if (!withdrawAttempt.ok) {
    throw new Error(
      `Even after splitting, the Withdraw half failed to compile: ${withdrawAttempt.error}`,
    );
  }
  const swapAttempt = tryCompileV0(args.user, blockhash, swapTxIxs, altAccounts);
  if (!swapAttempt.ok) {
    throw new Error(
      `Even after splitting, the Jupiter swap half blew the 1232-byte wire cap ` +
        `(estimated ${swapAttempt.bytes ?? "?"} bytes; ix count ${swapTxIxs.length}; ` +
        `static keys ${swapAttempt.staticKeys ?? "?"}; ALT addresses ${altAccounts.length}). ` +
        `Try a smaller basket (2 mints), lower per-leg \`maxAccounts\` (currently ${maxAccounts}), ` +
        `or pick mints with simpler Jupiter routes. Underlying error: ${swapAttempt.error}`,
    );
  }

  return {
    mode: "split",
    versionedTx: new VersionedTransaction(withdrawAttempt.message),
    swapTx: new VersionedTransaction(swapAttempt.message),
    altAccounts,
    legs,
    feeAmount,
    effectiveBurn,
    totalExpectedBasketOut,
    expectedSolOut: totalExpectedSol,
    minSolOut: totalMinSol,
    ixCount: withdrawTxIxs.length + swapTxIxs.length,
    txBytes: withdrawAttempt.bytes,
    computeUnitLimit: cuLimit,
    computeUnitPrice: cuPrice,
  };
}

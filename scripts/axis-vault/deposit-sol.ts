/**
 * Axis Vault — SOL-in Deposit helper (#36).
 *
 * Builds a single versioned transaction that, atomically:
 *   1. Splits `sol_in` lamports across the basket by weight.
 *   2. For each leg, Jupiter-swaps SOL → basket mint (client-side route).
 *   3. Calls the existing `Deposit` instruction to move basket tokens
 *      into the vaults and mint ETF tokens to the user.
 *
 * Atomicity: any leg (Jupiter or the final Deposit) failing reverts the
 * entire tx. No on-chain Jupiter CPI — all route construction happens
 * client-side, which keeps the program surface tiny and stays within
 * CU / account-list budgets even for a 5-token basket (Jupiter routes
 * can each carry 20+ accounts; a single versioned tx with ALT gives us
 * the room for ~5 legs).
 *
 * Slippage is enforced at two layers (belt + suspenders):
 *   - Jupiter's `otherAmountThreshold` on each swap (client-selected bps)
 *   - axis-vault's `min_mint_out` on the final Deposit
 *
 * Non-goals for this iteration:
 *   - Passing exact per-leg amounts to Deposit. Jupiter's actual outputs
 *     for a given SOL input generally don't land on the exact weight
 *     ratio we need, so we compute a safe `amount` floor from the
 *     quotes' `otherAmountThreshold` and any dust stays in the user's
 *     basket ATAs. An on-chain DepositExact variant (fee-aware) can
 *     land in a follow-up PR if dust becomes material.
 */
import {
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
} from "@solana/spl-token";
import {
  SOL_MINT,
  deserializeIx,
  fetchAltAccounts,
  getQuote,
  getSwapInstructions,
  type JupiterQuoteResponse,
} from "./jupiter";

export interface DepositSolArgs {
  conn: Connection;
  /** User / wallet that will sign and receive ETF tokens. */
  user: PublicKey;
  programId: PublicKey;
  /** Name seed used when the ETF was created (PDA + instruction data). */
  etfName: string;
  /** Derived `EtfState` PDA. */
  etfState: PublicKey;
  /** Derived / stored ETF mint. */
  etfMint: PublicKey;
  /** Treasury-owned ETF ATA (receives fee tokens; see issue #34). */
  treasuryEtfAta: PublicKey;
  /** Basket mint pubkeys in the same order as `weights` / `vaults`. */
  basketMints: PublicKey[];
  /** Per-basket-token weight in bps (must sum to 10_000). */
  weights: number[];
  /** Program-owned vault ATAs in the same order as `basketMints`. */
  vaults: PublicKey[];
  /** Total SOL to spend, in lamports. */
  solIn: bigint;
  /** Minimum ETF tokens the user is willing to accept. */
  minEtfOut: bigint;
  /** Jupiter slippage per leg (e.g. 50 = 0.5 %). Default 50. */
  slippageBps?: number;
  /** Override the Jupiter API host (mainnet-fork testing). */
  jupiterHost?: string;
  /** Override the recent blockhash (useful for pre-flight simulation). */
  recentBlockhash?: string;
}

export interface DepositSolPlan {
  /** Ready-to-sign versioned tx (unsigned). */
  versionedTx: VersionedTransaction;
  /** ALTs the caller must also include if re-compiling. */
  altAccounts: AddressLookupTableAccount[];
  /** Jupiter quotes indexed by leg (for logging / diagnostics). */
  quotes: JupiterQuoteResponse[];
  /** The `amount` base unit passed to the Axis Deposit IX. */
  depositAmount: bigint;
  /** Expected per-leg swap output (Jupiter's `outAmount`). */
  expectedBasketAmounts: bigint[];
  /** Total IX count in the tx (sanity-check for size limits). */
  ixCount: number;
}

function u64Le(n: bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
}

/**
 * Build the axis-vault Deposit instruction. Matches the account layout
 * in `contracts/axis-vault/src/instructions/deposit.rs`:
 *   [signer, etf_state, etf_mint, user_etf_ata, token_program,
 *    treasury_etf_ata, ...user_basket_atas, ...vaults]
 */
function buildAxisDepositIx(
  programId: PublicKey,
  user: PublicKey,
  etfState: PublicKey,
  etfMint: PublicKey,
  userEtfAta: PublicKey,
  treasuryEtfAta: PublicKey,
  userBasketAtas: PublicKey[],
  vaults: PublicKey[],
  etfName: string,
  amount: bigint,
  minMintOut: bigint,
): TransactionInstruction {
  const nameBytes = Buffer.from(etfName);
  const data = Buffer.concat([
    Buffer.from([1]), // disc = Deposit
    u64Le(amount),
    u64Le(minMintOut),
    Buffer.from([nameBytes.length]),
    nameBytes,
  ]);
  return new TransactionInstruction({
    programId,
    keys: [
      { pubkey: user, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMint, isSigner: false, isWritable: true },
      { pubkey: userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: treasuryEtfAta, isSigner: false, isWritable: true },
      ...userBasketAtas.map(a => ({ pubkey: a, isSigner: false, isWritable: true })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ],
    data,
  });
}

export async function buildDepositSolPlan(
  args: DepositSolArgs,
): Promise<DepositSolPlan> {
  const n = args.basketMints.length;
  if (n !== args.weights.length || n !== args.vaults.length) {
    throw new Error("basketMints / weights / vaults length mismatch");
  }
  const weightSum = args.weights.reduce((a, b) => a + b, 0);
  if (weightSum !== 10_000) {
    throw new Error(`weights must sum to 10_000, got ${weightSum}`);
  }

  const slippageBps = args.slippageBps ?? 50;

  // 1. Split SOL by weights.
  const perLegLamports: bigint[] = args.weights.map(
    w => (args.solIn * BigInt(w)) / 10_000n,
  );

  // 2. User's basket ATAs — these are where Jupiter deposits the output
  //    AND where axis-vault reads from during Deposit. We create them
  //    idempotently as part of the setup so a first-time user doesn't
  //    need a pre-flight tx.
  const userBasketAtas: PublicKey[] = args.basketMints.map(m =>
    getAssociatedTokenAddressSync(m, args.user, false),
  );
  const userEtfAta = getAssociatedTokenAddressSync(args.etfMint, args.user, false);

  // 3. Get a Jupiter quote + IXes for each leg. We route SOL
  //    (WSOL-in-the-API-sense) → basket mint with `wrapAndUnwrapSol`
  //    on the FIRST call only, so Jupiter wraps our SOL once and every
  //    subsequent leg pulls from the shared wSOL account.
  const quotes: JupiterQuoteResponse[] = [];
  const swapBundles = [] as Awaited<ReturnType<typeof getSwapInstructions>>[];
  for (let i = 0; i < n; i++) {
    const q = await getQuote({
      inputMint: SOL_MINT,
      outputMint: args.basketMints[i],
      amount: perLegLamports[i],
      slippageBps,
      swapMode: "ExactIn",
      jupiterHost: args.jupiterHost,
    });
    quotes.push(q);
    const bundle = await getSwapInstructions({
      quote: q,
      userPublicKey: args.user,
      destinationTokenAccount: userBasketAtas[i],
      // Wrap on the first leg, unwrap on the last leg. The middle legs
      // reuse the shared wSOL ATA without wrap/unwrap overhead.
      wrapAndUnwrapSol: i === 0 || i === n - 1,
      jupiterHost: args.jupiterHost,
    });
    swapBundles.push(bundle);
  }

  // 4. Compute a safe Deposit `amount`:
  //    amount * weight_i / 10_000 ≤ otherAmountThreshold_i for every i.
  //    Use the floor over legs to guarantee the on-chain transfers
  //    don't fail on insufficient balance. Dust stays in the user's
  //    basket ATAs and can be cleaned up by a follow-up tx.
  const expectedBasketAmounts = quotes.map(q => BigInt(q.outAmount));
  const minBasketAmounts = quotes.map(q => BigInt(q.otherAmountThreshold));
  let depositAmount = minBasketAmounts[0] * 10_000n / BigInt(args.weights[0]);
  for (let i = 1; i < n; i++) {
    const candidate = minBasketAmounts[i] * 10_000n / BigInt(args.weights[i]);
    if (candidate < depositAmount) depositAmount = candidate;
  }

  // 5. Assemble the instruction list. De-dupe setup IXes across legs
  //    (Jupiter often emits a "create shared wSOL ATA" IX in every
  //    response, which would fail the second time through).
  const ixes: TransactionInstruction[] = [];
  const seen = new Set<string>();
  const pushDedup = (ix: TransactionInstruction) => {
    const key = JSON.stringify([
      ix.programId.toBase58(),
      ix.keys.map(k => `${k.pubkey.toBase58()}:${k.isSigner}:${k.isWritable}`),
      ix.data.toString("base64"),
    ]);
    if (!seen.has(key)) {
      seen.add(key);
      ixes.push(ix);
    }
  };

  // Compute budget — take from the first response; Jupiter sizes it to
  // the widest leg, which is a safe upper bound for the bundled tx.
  for (const raw of swapBundles[0].computeBudgetInstructions) {
    pushDedup(deserializeIx(raw));
  }

  // Idempotent creation of user's basket ATAs and ETF ATA. Axis Deposit
  // reads the user ATA balances, and Jupiter needs a destination ATA,
  // so creating them up-front is simplest.
  for (let i = 0; i < n; i++) {
    pushDedup(
      createAssociatedTokenAccountIdempotentInstruction(
        args.user,
        userBasketAtas[i],
        args.user,
        args.basketMints[i],
      ),
    );
  }
  pushDedup(
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      userEtfAta,
      args.user,
      args.etfMint,
    ),
  );

  // Per-leg Jupiter swap + its setup/cleanup.
  for (const bundle of swapBundles) {
    for (const raw of bundle.setupInstructions) pushDedup(deserializeIx(raw));
    pushDedup(deserializeIx(bundle.swapInstruction));
    if (bundle.cleanupInstruction) {
      pushDedup(deserializeIx(bundle.cleanupInstruction));
    }
  }

  // Final axis-vault Deposit — mints ETF tokens, transfers basket
  // tokens from user ATAs to vaults. Revert here undoes every Jupiter
  // swap above thanks to versioned-tx atomicity.
  ixes.push(
    buildAxisDepositIx(
      args.programId,
      args.user,
      args.etfState,
      args.etfMint,
      userEtfAta,
      args.treasuryEtfAta,
      userBasketAtas,
      args.vaults,
      args.etfName,
      depositAmount,
      args.minEtfOut,
    ),
  );

  // 6. Collect ALT addresses from all Jupiter responses.
  const altAddresses = swapBundles.flatMap(b => b.addressLookupTableAddresses);
  const altAccounts = await fetchAltAccounts(args.conn, altAddresses);

  const blockhash =
    args.recentBlockhash ??
    (await args.conn.getLatestBlockhash("finalized")).blockhash;
  const message = new TransactionMessage({
    payerKey: args.user,
    recentBlockhash: blockhash,
    instructions: ixes,
  }).compileToV0Message(altAccounts);
  const versionedTx = new VersionedTransaction(message);

  return {
    versionedTx,
    altAccounts,
    quotes,
    depositAmount,
    expectedBasketAmounts,
    ixCount: ixes.length,
  };
}

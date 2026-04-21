/**
 * Axis Vault — SOL-out Withdraw helper (#36).
 *
 * Symmetric to `deposit-sol.ts`: builds a single versioned tx that
 * atomically burns ETF tokens, receives basket tokens, Jupiter-swaps
 * each leg back to SOL (via wSOL), and closes the wSOL account so the
 * user ends up with native SOL.
 *
 * Tx layout:
 *   [ComputeBudget]
 *   [create user basket ATAs idempotent]     ← destinations for Withdraw
 *   [create user wSOL ATA idempotent]        ← destination for swaps
 *   [axis-vault Withdraw]                    ← burn ETF, basket tokens out
 *   [Jupiter swap × N]                       ← basket tokens → wSOL
 *   [close wSOL ATA]                         ← wSOL → native SOL
 *
 * Slippage layers:
 *   - axis-vault `min_tokens_out` on the Withdraw (catches vault drift)
 *   - Jupiter `otherAmountThreshold` per leg
 *   - A client-side `minSolOut` check before submitting (not on-chain;
 *     on-chain enforcement would need a bespoke WithdrawSol IX — left
 *     as follow-up).
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
  NATIVE_MINT,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
  createCloseAccountInstruction,
} from "@solana/spl-token";
import {
  SOL_MINT,
  deserializeIx,
  fetchAltAccounts,
  getQuote,
  getSwapInstructions,
  type JupiterQuoteResponse,
} from "./jupiter";

export interface WithdrawSolArgs {
  conn: Connection;
  user: PublicKey;
  programId: PublicKey;
  etfName: string;
  etfState: PublicKey;
  etfMint: PublicKey;
  treasuryEtfAta: PublicKey;
  basketMints: PublicKey[];
  weights: number[];
  vaults: PublicKey[];
  /** ETF tokens to burn, in mint atoms (6 dp). */
  burnAmount: bigint;
  /**
   * Client-side minimum SOL out — purely informational today (we assert
   * against quote sum before returning the tx). On-chain enforcement
   * lives with the on-chain DepositSol/WithdrawSol follow-up.
   */
  minSolOut?: bigint;
  slippageBps?: number;
  jupiterHost?: string;
  recentBlockhash?: string;
}

export interface WithdrawSolPlan {
  versionedTx: VersionedTransaction;
  altAccounts: AddressLookupTableAccount[];
  quotes: JupiterQuoteResponse[];
  /** Expected per-leg basket output from axis-vault Withdraw. */
  expectedBasketFromVault: bigint[];
  /** Expected SOL out after Jupiter leg swaps (sum of outAmount). */
  expectedSolOut: bigint;
  /** Conservative floor: sum of `otherAmountThreshold`. */
  minSolOutGuaranteed: bigint;
  ixCount: number;
}

function u64Le(n: bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
}

/**
 * Axis-vault Withdraw instruction. Mirrors `withdraw.rs` account order:
 *   [signer, etf_state, etf_mint, user_etf_ata, token_program,
 *    treasury_etf_ata, ...vaults, ...user_basket_atas]
 * Note vault / user-basket order is flipped vs Deposit — funds flow the
 * other way.
 */
function buildAxisWithdrawIx(
  programId: PublicKey,
  user: PublicKey,
  etfState: PublicKey,
  etfMint: PublicKey,
  userEtfAta: PublicKey,
  treasuryEtfAta: PublicKey,
  vaults: PublicKey[],
  userBasketAtas: PublicKey[],
  etfName: string,
  burnAmount: bigint,
  minTokensOut: bigint,
): TransactionInstruction {
  const nameBytes = Buffer.from(etfName);
  const data = Buffer.concat([
    Buffer.from([2]), // disc = Withdraw
    u64Le(burnAmount),
    u64Le(minTokensOut),
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
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ...userBasketAtas.map(a => ({ pubkey: a, isSigner: false, isWritable: true })),
    ],
    data,
  });
}

/**
 * Estimate what Withdraw will deliver per leg, off-chain, so we can
 * quote Jupiter up-front. Reads current vault balances and total
 * supply; applies the 30 bps fee split and the proportional math the
 * on-chain code uses. The on-chain tx re-derives these exactly, so
 * any minor drift between estimate and actual is absorbed by Jupiter's
 * per-swap slippage.
 */
async function estimateWithdrawBasketAmounts(
  conn: Connection,
  etfState: PublicKey,
  vaults: PublicKey[],
  burnAmount: bigint,
): Promise<bigint[]> {
  // total_supply offset: see `contracts/axis-vault/src/state/etf.rs`
  // (stable at 408 bytes under both pre- and post-#37 layouts).
  const TOTAL_SUPPLY_OFFSET = 408;
  // fee_bps offset: 448. (see print_sizes test in lib.rs)
  const FEE_BPS_OFFSET = 448;
  const stateInfo = await conn.getAccountInfo(etfState);
  if (!stateInfo) throw new Error(`EtfState not found: ${etfState.toBase58()}`);
  const totalSupply = stateInfo.data.readBigUInt64LE(TOTAL_SUPPLY_OFFSET);
  const feeBps = BigInt(stateInfo.data.readUInt16LE(FEE_BPS_OFFSET));

  if (totalSupply === 0n) {
    throw new Error("Vault is empty (total_supply == 0)");
  }

  const feeAmount = (burnAmount * feeBps) / 10_000n;
  const effectiveBurn = burnAmount - feeAmount;

  const out: bigint[] = [];
  const vaultInfos = await conn.getMultipleAccountsInfo(vaults);
  for (let i = 0; i < vaults.length; i++) {
    const info = vaultInfos[i];
    if (!info) throw new Error(`Vault ${i} missing: ${vaults[i].toBase58()}`);
    const balance = info.data.readBigUInt64LE(64);
    out.push((balance * effectiveBurn) / totalSupply);
  }
  return out;
}

export async function buildWithdrawSolPlan(
  args: WithdrawSolArgs,
): Promise<WithdrawSolPlan> {
  const n = args.basketMints.length;
  if (n !== args.weights.length || n !== args.vaults.length) {
    throw new Error("basketMints / weights / vaults length mismatch");
  }
  const slippageBps = args.slippageBps ?? 50;

  // 1. Estimate what Withdraw will deliver per leg so we can pre-quote
  //    Jupiter with realistic ExactIn amounts.
  const expectedBasketFromVault = await estimateWithdrawBasketAmounts(
    args.conn,
    args.etfState,
    args.vaults,
    args.burnAmount,
  );
  for (let i = 0; i < n; i++) {
    if (expectedBasketFromVault[i] === 0n) {
      throw new Error(
        `Leg ${i} would round to 0 — burn amount too small for this vault size`,
      );
    }
  }

  // 2. User ATAs (basket + ETF + wSOL). All created idempotently.
  const userBasketAtas = args.basketMints.map(m =>
    getAssociatedTokenAddressSync(m, args.user, false),
  );
  const userEtfAta = getAssociatedTokenAddressSync(args.etfMint, args.user, false);
  const userWsolAta = getAssociatedTokenAddressSync(NATIVE_MINT, args.user, false);

  // 3. Jupiter quotes: each leg is basket_mint_i → wSOL for the
  //    expected output amount from the Withdraw. Destination on every
  //    leg is the shared wSOL ATA so `closeAccount` at the end dumps
  //    the whole balance back as SOL.
  const quotes: JupiterQuoteResponse[] = [];
  const swapBundles = [] as Awaited<ReturnType<typeof getSwapInstructions>>[];
  for (let i = 0; i < n; i++) {
    const q = await getQuote({
      inputMint: args.basketMints[i],
      outputMint: SOL_MINT,
      amount: expectedBasketFromVault[i],
      slippageBps,
      swapMode: "ExactIn",
      jupiterHost: args.jupiterHost,
    });
    quotes.push(q);
    const bundle = await getSwapInstructions({
      quote: q,
      userPublicKey: args.user,
      destinationTokenAccount: userWsolAta,
      // We explicitly manage the wSOL ATA — don't let Jupiter
      // auto-unwrap mid-flight (it would close the ATA between legs).
      wrapAndUnwrapSol: false,
      jupiterHost: args.jupiterHost,
    });
    swapBundles.push(bundle);
  }

  // 4. Assemble the tx.
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

  // Compute budget from the first Jupiter response.
  for (const raw of swapBundles[0].computeBudgetInstructions) {
    pushDedup(deserializeIx(raw));
  }

  // Create user basket ATAs + wSOL ATA idempotently. Note: user ETF
  // ATA is the *source* of the burn, so the caller must already have
  // ETF tokens (we don't create it here — burning from a non-existent
  // account would fail).
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
      userWsolAta,
      args.user,
      NATIVE_MINT,
    ),
  );

  // Withdraw first: burns the ETF tokens, transfers basket tokens to
  // userBasketAtas. `min_tokens_out = 0` at the axis-vault layer so we
  // don't duplicate the slippage check that Jupiter is already doing.
  // If the caller wanted on-chain enforcement they'd use the (future)
  // native WithdrawSol instruction.
  ixes.push(
    buildAxisWithdrawIx(
      args.programId,
      args.user,
      args.etfState,
      args.etfMint,
      userEtfAta,
      args.treasuryEtfAta,
      args.vaults,
      userBasketAtas,
      args.etfName,
      args.burnAmount,
      0n,
    ),
  );

  for (const bundle of swapBundles) {
    for (const raw of bundle.setupInstructions) pushDedup(deserializeIx(raw));
    pushDedup(deserializeIx(bundle.swapInstruction));
    if (bundle.cleanupInstruction) {
      pushDedup(deserializeIx(bundle.cleanupInstruction));
    }
  }

  // Close the wSOL ATA to return all wrapped lamports as native SOL to
  // the user. Rent-exempt minimum also returns.
  ixes.push(
    createCloseAccountInstruction(userWsolAta, args.user, args.user),
  );

  // 5. Size / slippage checks before returning.
  const expectedSolOut = quotes.reduce((s, q) => s + BigInt(q.outAmount), 0n);
  const minSolOutGuaranteed = quotes.reduce(
    (s, q) => s + BigInt(q.otherAmountThreshold),
    0n,
  );
  if (args.minSolOut !== undefined && minSolOutGuaranteed < args.minSolOut) {
    throw new Error(
      `Quoted SOL out ${minSolOutGuaranteed} < minSolOut ${args.minSolOut}. ` +
      `Bump slippageBps or reduce burnAmount.`,
    );
  }

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
    expectedBasketFromVault,
    expectedSolOut,
    minSolOutGuaranteed,
    ixCount: ixes.length,
  };
}

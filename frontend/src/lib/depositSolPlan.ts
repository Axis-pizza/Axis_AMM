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
  createAssociatedTokenAccountIdempotentInstruction,
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
import { u64Le } from "./ix";

export interface DepositSolPlanArgs {
  conn: Connection;
  user: PublicKey;
  programId: PublicKey;
  etfName: string;
  etfState: PublicKey;
  etfMint: PublicKey;
  treasury: PublicKey;
  treasuryEtfAta: PublicKey;
  basketMints: PublicKey[];
  weights: number[];
  vaults: PublicKey[];
  solIn: bigint;
  minEtfOut: bigint;
  slippageBps?: number;
}

export interface DepositSolPlan {
  versionedTx: VersionedTransaction;
  altAccounts: AddressLookupTableAccount[];
  quotes: JupiterQuoteResponse[];
  depositAmount: bigint;
  expectedBasketAmounts: bigint[];
  ixCount: number;
}

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
  return new TransactionInstruction({
    programId,
    keys: [
      { pubkey: user, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMint, isSigner: false, isWritable: true },
      { pubkey: userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: treasuryEtfAta, isSigner: false, isWritable: true },
      ...userBasketAtas.map((a) => ({ pubkey: a, isSigner: false, isWritable: true })),
      ...vaults.map((v) => ({ pubkey: v, isSigner: false, isWritable: true })),
    ],
    data: Buffer.concat([
      Buffer.from([1]),
      u64Le(amount),
      u64Le(minMintOut),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]),
  });
}

export async function buildDepositSolPlan(
  args: DepositSolPlanArgs,
): Promise<DepositSolPlan> {
  const n = args.basketMints.length;
  if (n !== args.weights.length || n !== args.vaults.length) {
    throw new Error("basketMints / weights / vaults length mismatch");
  }
  const weightSum = args.weights.reduce((a, b) => a + b, 0);
  if (weightSum !== 10_000) {
    throw new Error(`weights must sum to 10_000, got ${weightSum}`);
  }
  if (args.solIn <= 0n) {
    throw new Error("SOL input must be greater than zero");
  }

  const slippageBps = args.slippageBps ?? 50;
  const perLegLamports = args.weights.map(
    (w) => (args.solIn * BigInt(w)) / 10_000n,
  );
  const userBasketAtas = args.basketMints.map((m) =>
    getAssociatedTokenAddressSync(m, args.user, false),
  );
  const userEtfAta = getAssociatedTokenAddressSync(args.etfMint, args.user, false);

  const quotes = await Promise.all(
    args.basketMints.map((mint, i) => getQuote({
      inputMint: SOL_MINT,
      outputMint: mint,
      amount: perLegLamports[i],
      slippageBps,
      swapMode: "ExactIn",
    })),
  );
  const swapBundles = await Promise.all(
    quotes.map((quote, i) =>
      getSwapInstructions({
        quote,
        userPublicKey: args.user,
        destinationTokenAccount: userBasketAtas[i],
        wrapAndUnwrapSol: true,
      }),
    ),
  );

  const minBasketAmounts = quotes.map((q) => BigInt(q.otherAmountThreshold));
  let depositAmount = (minBasketAmounts[0] * 10_000n) / BigInt(args.weights[0]);
  for (let i = 1; i < n; i++) {
    const candidate = (minBasketAmounts[i] * 10_000n) / BigInt(args.weights[i]);
    if (candidate < depositAmount) depositAmount = candidate;
  }

  const ixs: TransactionInstruction[] = [];
  const seen = new Set<string>();
  const pushDedup = (ix: TransactionInstruction) => {
    const key = JSON.stringify([
      ix.programId.toBase58(),
      ix.keys.map((k) => `${k.pubkey.toBase58()}:${k.isSigner}:${k.isWritable}`),
      ix.data.toString("base64"),
    ]);
    if (!seen.has(key)) {
      seen.add(key);
      ixs.push(ix);
    }
  };

  for (const raw of swapBundles[0].computeBudgetInstructions) {
    pushDedup(deserializeIx(raw));
  }
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
  pushDedup(
    createAssociatedTokenAccountIdempotentInstruction(
      args.user,
      args.treasuryEtfAta,
      args.treasury,
      args.etfMint,
    ),
  );

  for (const bundle of swapBundles) {
    for (const raw of bundle.setupInstructions) pushDedup(deserializeIx(raw));
    pushDedup(deserializeIx(bundle.swapInstruction));
    if (bundle.cleanupInstruction) pushDedup(deserializeIx(bundle.cleanupInstruction));
  }

  ixs.push(
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

  const altAccounts = await fetchAltAccounts(
    args.conn,
    swapBundles.flatMap((b) => b.addressLookupTableAddresses),
  );
  const { blockhash } = await args.conn.getLatestBlockhash("confirmed");
  const message = new TransactionMessage({
    payerKey: args.user,
    recentBlockhash: blockhash,
    instructions: ixs,
  }).compileToV0Message(altAccounts);

  return {
    versionedTx: new VersionedTransaction(message),
    altAccounts,
    quotes,
    depositAmount,
    expectedBasketAmounts: quotes.map((q) => BigInt(q.outAmount)),
    ixCount: ixs.length,
  };
}

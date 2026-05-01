import {
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Buffer } from "buffer";

/// Hand-rolled instruction builders for axis-vault + pfda-amm-3.
/// Mirrors test/e2e/axis-vault/axis-vault.local.e2e.ts and
/// test/e2e/pfda-amm-3/pfda-amm-3.local.e2e.ts byte-for-byte so the
/// frontend can drive the same on-chain flows the e2e suite exercises.
///
/// We keep these as pure builders (no Connection, no signers) so the
/// React components can compose them into Transactions and hand off
/// signing to the wallet adapter.

// ───────── encoding helpers ─────────

export function u64Le(n: bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
}

export function u32Le(n: number): Buffer {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
}

export function u16Le(n: number): Buffer {
  const b = Buffer.alloc(2);
  b.writeUInt16LE(n);
  return b;
}

// ───────── axis-vault PDAs + builders ─────────

export function findEtfState(
  programId: PublicKey,
  payer: PublicKey,
  name: string,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("etf"), payer.toBuffer(), Buffer.from(name)],
    programId,
  );
}

/// Metaplex Token Metadata Program (mainnet + devnet share this ID).
export const METAPLEX_TOKEN_METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
);

/// Derive the Metaplex Token Metadata PDA for a given mint:
/// `[b"metadata", METAPLEX_TOKEN_METADATA_PROGRAM_ID, mint]`.
export function findMetadataPda(mint: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("metadata"),
      METAPLEX_TOKEN_METADATA_PROGRAM_ID.toBuffer(),
      mint.toBuffer(),
    ],
    METAPLEX_TOKEN_METADATA_PROGRAM_ID,
  );
}

export interface CreateEtfArgs {
  programId: PublicKey;
  payer: PublicKey;
  etfState: PublicKey;
  etfMint: PublicKey;
  treasury: PublicKey;
  basketMints: PublicKey[];
  vaults: PublicKey[];
  weightsBps: number[]; // length = basketMints.length, sums to 10_000
  ticker: string;
  name: string;
  /// v1.1: off-chain JSON metadata URI (max 200 bytes). Empty string is
  /// valid and emits a metadata account with no URI.
  uri: string;
  /// v1.1: optional override for the Metaplex metadata PDA; defaults to
  /// `findMetadataPda(etfMint)`. Pass-through is mainly for tests that
  /// want to assert the program rejects mis-derived PDAs.
  metadataPda?: PublicKey;
}

export function ixCreateEtf(args: CreateEtfArgs): TransactionInstruction {
  if (args.basketMints.length !== args.vaults.length)
    throw new Error("basketMints / vaults length mismatch");
  if (args.basketMints.length !== args.weightsBps.length)
    throw new Error("basketMints / weights length mismatch");
  if (args.ticker.length > 10)
    throw new Error("ticker exceeds Metaplex MAX_SYMBOL_LENGTH (10)");
  const uriBytes = Buffer.from(args.uri);
  if (uriBytes.length > 200)
    throw new Error("uri exceeds Metaplex MAX_URI_LENGTH (200)");

  const tokenCount = args.basketMints.length;
  const weightsBuf = Buffer.alloc(tokenCount * 2);
  for (let i = 0; i < tokenCount; i++) {
    weightsBuf.writeUInt16LE(args.weightsBps[i], i * 2);
  }
  const tickerBytes = Buffer.from(args.ticker);
  const nameBytes = Buffer.from(args.name);

  const data = Buffer.concat([
    Buffer.from([0]), // disc = CreateEtf
    Buffer.from([tokenCount]),
    weightsBuf,
    Buffer.from([tickerBytes.length]),
    tickerBytes,
    Buffer.from([nameBytes.length]),
    nameBytes,
    Buffer.from([uriBytes.length]), // v1.1
    uriBytes,                        // v1.1
  ]);

  const metadataPda = args.metadataPda ?? findMetadataPda(args.etfMint)[0];

  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.payer, isSigner: true, isWritable: true },
      { pubkey: args.etfState, isSigner: false, isWritable: true },
      { pubkey: args.etfMint, isSigner: false, isWritable: true },
      { pubkey: args.treasury, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...args.basketMints.map((m) => ({
        pubkey: m,
        isSigner: false,
        isWritable: false,
      })),
      ...args.vaults.map((v) => ({
        pubkey: v,
        isSigner: false,
        isWritable: true,
      })),
      // v1.1
      { pubkey: metadataPda, isSigner: false, isWritable: true },
      {
        pubkey: METAPLEX_TOKEN_METADATA_PROGRAM_ID,
        isSigner: false,
        isWritable: false,
      },
    ],
    data,
  });
}

export interface DepositArgs {
  programId: PublicKey;
  payer: PublicKey;
  etfState: PublicKey;
  etfMint: PublicKey;
  userEtfAta: PublicKey;
  treasuryEtfAta: PublicKey;
  userBasketAccounts: PublicKey[];
  vaults: PublicKey[];
  amount: bigint;
  minMintOut: bigint;
  name: string;
}

export function ixDeposit(args: DepositArgs): TransactionInstruction {
  const nameBytes = Buffer.from(args.name);
  const data = Buffer.concat([
    Buffer.from([1]),
    u64Le(args.amount),
    u64Le(args.minMintOut),
    Buffer.from([nameBytes.length]),
    nameBytes,
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.payer, isSigner: true, isWritable: true },
      { pubkey: args.etfState, isSigner: false, isWritable: true },
      { pubkey: args.etfMint, isSigner: false, isWritable: true },
      { pubkey: args.userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: args.treasuryEtfAta, isSigner: false, isWritable: true },
      ...args.userBasketAccounts.map((u) => ({
        pubkey: u,
        isSigner: false,
        isWritable: true,
      })),
      ...args.vaults.map((v) => ({
        pubkey: v,
        isSigner: false,
        isWritable: true,
      })),
    ],
    data,
  });
}

export interface WithdrawArgs {
  programId: PublicKey;
  payer: PublicKey;
  etfState: PublicKey;
  etfMint: PublicKey;
  userEtfAta: PublicKey;
  treasuryEtfAta: PublicKey;
  vaults: PublicKey[];
  userBasketAccounts: PublicKey[];
  burnAmount: bigint;
  /// Minimum SUM of basket-token outputs across all legs. Mirrors the
  /// on-chain pre-transfer slippage guard in
  /// `contracts/axis-vault/src/instructions/withdraw.rs:139-166`.
  minTokensOut: bigint;
  name: string;
}

/// Withdraw — burn ETF tokens, return proportional basket tokens.
/// Account layout differs from Deposit: vaults come BEFORE user basket
/// ATAs because funds flow vault → user (the reverse of Deposit).
/// On-chain reads vault[i] at index `6+i` and user_dest[i] at `6+N+i`.
export function ixWithdraw(args: WithdrawArgs): TransactionInstruction {
  if (args.vaults.length !== args.userBasketAccounts.length) {
    throw new Error("vaults / userBasketAccounts length mismatch");
  }
  const nameBytes = Buffer.from(args.name);
  const data = Buffer.concat([
    Buffer.from([2]),
    u64Le(args.burnAmount),
    u64Le(args.minTokensOut),
    Buffer.from([nameBytes.length]),
    nameBytes,
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.payer, isSigner: true, isWritable: true },
      { pubkey: args.etfState, isSigner: false, isWritable: true },
      { pubkey: args.etfMint, isSigner: false, isWritable: true },
      { pubkey: args.userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: args.treasuryEtfAta, isSigner: false, isWritable: true },
      ...args.vaults.map((v) => ({
        pubkey: v,
        isSigner: false,
        isWritable: true,
      })),
      ...args.userBasketAccounts.map((u) => ({
        pubkey: u,
        isSigner: false,
        isWritable: true,
      })),
    ],
    data,
  });
}

// ───────── pfda-amm-3 PDAs + builders ─────────

export function findPool3(
  programId: PublicKey,
  m0: PublicKey,
  m1: PublicKey,
  m2: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("pool3"), m0.toBuffer(), m1.toBuffer(), m2.toBuffer()],
    programId,
  );
}

export function findQueue3(
  programId: PublicKey,
  pool: PublicKey,
  batchId: bigint,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("queue3"), pool.toBuffer(), u64Le(batchId)],
    programId,
  );
}

export function findHistory3(
  programId: PublicKey,
  pool: PublicKey,
  batchId: bigint,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("history3"), pool.toBuffer(), u64Le(batchId)],
    programId,
  );
}

export function findTicket3(
  programId: PublicKey,
  pool: PublicKey,
  user: PublicKey,
  batchId: bigint,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("ticket3"), pool.toBuffer(), user.toBuffer(), u64Le(batchId)],
    programId,
  );
}

export interface InitPoolArgs {
  programId: PublicKey;
  payer: PublicKey;
  pool: PublicKey;
  queue: PublicKey;
  mints: [PublicKey, PublicKey, PublicKey];
  vaults: [PublicKey, PublicKey, PublicKey];
  treasury: PublicKey;
  feeBps: number;
  windowSlots: bigint;
  weights: [number, number, number]; // sum = 1_000_000
}

export function ixInitPool3(args: InitPoolArgs): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([0]),
    u16Le(args.feeBps),
    u64Le(args.windowSlots),
    u32Le(args.weights[0]),
    u32Le(args.weights[1]),
    u32Le(args.weights[2]),
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.payer, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: true },
      { pubkey: args.queue, isSigner: false, isWritable: true },
      { pubkey: args.mints[0], isSigner: false, isWritable: false },
      { pubkey: args.mints[1], isSigner: false, isWritable: false },
      { pubkey: args.mints[2], isSigner: false, isWritable: false },
      { pubkey: args.vaults[0], isSigner: false, isWritable: true },
      { pubkey: args.vaults[1], isSigner: false, isWritable: true },
      { pubkey: args.vaults[2], isSigner: false, isWritable: true },
      { pubkey: args.treasury, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

export interface AddLiquidityArgs {
  programId: PublicKey;
  payer: PublicKey;
  pool: PublicKey;
  vaults: [PublicKey, PublicKey, PublicKey];
  userTokens: [PublicKey, PublicKey, PublicKey];
  amounts: [bigint, bigint, bigint];
}

export function ixAddLiquidity3(args: AddLiquidityArgs): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([4]),
    u64Le(args.amounts[0]),
    u64Le(args.amounts[1]),
    u64Le(args.amounts[2]),
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.payer, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: true },
      { pubkey: args.vaults[0], isSigner: false, isWritable: true },
      { pubkey: args.vaults[1], isSigner: false, isWritable: true },
      { pubkey: args.vaults[2], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[0], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[1], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[2], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

export interface SwapRequestArgs {
  programId: PublicKey;
  user: PublicKey;
  pool: PublicKey;
  queue: PublicKey;
  ticket: PublicKey;
  userTokenIn: PublicKey;
  vaultIn: PublicKey;
  inIdx: number;
  outIdx: number;
  amountIn: bigint;
  minOut: bigint;
}

export function ixSwapRequest3(args: SwapRequestArgs): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([1]),
    Buffer.from([args.inIdx]),
    u64Le(args.amountIn),
    Buffer.from([args.outIdx]),
    u64Le(args.minOut),
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.user, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: false },
      { pubkey: args.queue, isSigner: false, isWritable: true },
      { pubkey: args.ticket, isSigner: false, isWritable: true },
      { pubkey: args.userTokenIn, isSigner: false, isWritable: true },
      { pubkey: args.vaultIn, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

export interface ClearBatchArgs {
  programId: PublicKey;
  cranker: PublicKey;
  pool: PublicKey;
  queue: PublicKey;
  history: PublicKey;
  nextQueue: PublicKey;
}

export function ixClearBatch3(args: ClearBatchArgs): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.cranker, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: true },
      { pubkey: args.queue, isSigner: false, isWritable: true },
      { pubkey: args.history, isSigner: false, isWritable: true },
      { pubkey: args.nextQueue, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([2]),
  });
}

export interface ClaimArgs {
  programId: PublicKey;
  user: PublicKey;
  pool: PublicKey;
  history: PublicKey;
  ticket: PublicKey;
  vaults: [PublicKey, PublicKey, PublicKey];
  userTokens: [PublicKey, PublicKey, PublicKey];
}

export function ixClaim3(args: ClaimArgs): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.user, isSigner: true, isWritable: false },
      { pubkey: args.pool, isSigner: false, isWritable: true },
      { pubkey: args.history, isSigner: false, isWritable: false },
      { pubkey: args.ticket, isSigner: false, isWritable: true },
      { pubkey: args.vaults[0], isSigner: false, isWritable: true },
      { pubkey: args.vaults[1], isSigner: false, isWritable: true },
      { pubkey: args.vaults[2], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[0], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[1], isSigner: false, isWritable: true },
      { pubkey: args.userTokens[2], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  });
}

export interface WithdrawFees3Args {
  programId: PublicKey;
  authority: PublicKey;
  pool: PublicKey;
  vaults: [PublicKey, PublicKey, PublicKey];
  treasuryTokens: [PublicKey, PublicKey, PublicKey];
  amounts: [bigint, bigint, bigint];
}

/// WithdrawFees (disc=5) — only the pool authority may transfer
/// `amounts[i]` from `vaults[i]` to `treasuryTokens[i]`. On-chain
/// asserts each vault matches `pool.vaults[i]`, decrements
/// `pool.reserves[i]` in the same ix to keep clearing-price math
/// in sync with real vault balances (#33). See
/// `contracts/pfda-amm-3/src/instructions/withdraw_fees.rs`.
export function ixWithdrawFees3(args: WithdrawFees3Args): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([5]),
    u64Le(args.amounts[0]),
    u64Le(args.amounts[1]),
    u64Le(args.amounts[2]),
  ]);
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.authority, isSigner: true, isWritable: false },
      { pubkey: args.pool, isSigner: false, isWritable: true },
      { pubkey: args.vaults[0], isSigner: false, isWritable: true },
      { pubkey: args.vaults[1], isSigner: false, isWritable: true },
      { pubkey: args.vaults[2], isSigner: false, isWritable: true },
      { pubkey: args.treasuryTokens[0], isSigner: false, isWritable: true },
      { pubkey: args.treasuryTokens[1], isSigner: false, isWritable: true },
      { pubkey: args.treasuryTokens[2], isSigner: false, isWritable: true },
    ],
    data,
  });
}

export interface CloseBatchHistory3Args {
  programId: PublicKey;
  /// Must be the pool authority — the program rejects any other signer
  /// to prevent rent siphoning by whoever wins the close-delay race.
  rentRecipient: PublicKey;
  pool: PublicKey;
  history: PublicKey;
}

/// CloseBatchHistory (disc=7) — reclaim rent from a ClearedBatchHistory3
/// PDA after `current_batch_id ≥ history.batch_id + 100` (CLOSE_DELAY).
/// Rent goes to `rentRecipient`, which the program asserts equals
/// `pool.authority`.
export function ixCloseBatchHistory3(
  args: CloseBatchHistory3Args,
): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.rentRecipient, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: false },
      { pubkey: args.history, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([7]),
  });
}

export interface CloseExpiredTicket3Args {
  programId: PublicKey;
  /// Anyone can crank — the rent itself returns to `rentRecipient`,
  /// which the program asserts equals `ticket.owner`.
  caller: PublicKey;
  pool: PublicKey;
  ticket: PublicKey;
  rentRecipient: PublicKey;
}

/// CloseExpiredTicket (disc=8) — reclaim rent from an unclaimed
/// UserOrderTicket3 after `current_batch_id ≥ ticket.batch_id + 200`
/// (TICKET_EXPIRY_BATCHES). Anyone can sign; rent goes to the original
/// ticket owner.
export function ixCloseExpiredTicket3(
  args: CloseExpiredTicket3Args,
): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.caller, isSigner: true, isWritable: true },
      { pubkey: args.pool, isSigner: false, isWritable: false },
      { pubkey: args.ticket, isSigner: false, isWritable: true },
      { pubkey: args.rentRecipient, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([8]),
  });
}

export interface SetPaused3Args {
  programId: PublicKey;
  authority: PublicKey;
  pool: PublicKey;
  paused: boolean;
}

/// SetPaused (disc=6) — flip `pool.paused`. Authority-gated, single
/// account mutation. Mirrors axis-vault's SetPaused but on
/// `PoolState3`. See `contracts/pfda-amm-3/src/instructions/set_paused.rs`.
export function ixSetPaused3(args: SetPaused3Args): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.programId,
    keys: [
      { pubkey: args.authority, isSigner: true, isWritable: false },
      { pubkey: args.pool, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([6, args.paused ? 1 : 0]),
  });
}

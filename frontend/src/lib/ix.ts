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
}

export function ixCreateEtf(args: CreateEtfArgs): TransactionInstruction {
  if (args.basketMints.length !== args.vaults.length)
    throw new Error("basketMints / vaults length mismatch");
  if (args.basketMints.length !== args.weightsBps.length)
    throw new Error("basketMints / weights length mismatch");

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
  ]);

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

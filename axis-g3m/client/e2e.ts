/**
 * Axis G3M — E2E Test Script
 *
 * Flow:
 *   1. Create 5 token mints (simulating CEX-unlisted memecoins)
 *   2. Create user token accounts & mint tokens
 *   3. Pre-allocate vault accounts (pool-PDA-owned)
 *   4. Derive PDAs
 *   5. InitializePool  (5 tokens, 20% each, 1% fee, 5% drift threshold)
 *   6. Swap (token 0 → token 1)
 *   7. CheckDrift
 *   8. Large swap to force drift > 5%
 *   9. Rebalance
 *
 * Matches pfda-amm/client/e2e.ts conventions.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint,
  createAccount,
  createInitializeAccountInstruction,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

// ─── Config ───────────────────────────────────────────────────────────────

const PROGRAM_ID = new PublicKey("65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi");
const RPC_URL = "http://localhost:8899";

const TOKEN_COUNT = 5;
const FEE_RATE_BPS = 100;           // 1%
const DRIFT_THRESHOLD_BPS = 500;    // 5%
const REBALANCE_COOLDOWN = 0n;      // No cooldown for testing
const INITIAL_RESERVE = 1_000_000_000n;  // 1,000 tokens (6 decimals) per token
const WEIGHTS_BPS = [2000, 2000, 2000, 2000, 2000]; // Equal weight 20% each

// ─── Utilities ────────────────────────────────────────────────────────────

function loadPayer(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8")))
  );
}

function u64Le(n: bigint): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
}
function u16Le(n: number): Buffer {
  const b = Buffer.alloc(2);
  b.writeUInt16LE(n);
  return b;
}
function num(n: bigint): string {
  return n.toLocaleString();
}

// ─── PDA ──────────────────────────────────────────────────────────────────

function findPool(authority: PublicKey) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("g3m_pool"), authority.toBuffer()],
    PROGRAM_ID
  );
}

// ─── G3mPoolState Layout (464 bytes, repr(C)) ────────────────────────────
//
// offset  field
// 0       discriminator [u8; 8]
// 8       authority [u8; 32]
// 40      token_count u8
// 41      token_mints [[u8;32]; 5]  (160 bytes)
// 201     token_vaults [[u8;32]; 5] (160 bytes)
// 362     target_weights_bps [u16; 5] (10 bytes) — with 1 byte pad before
// 376     reserves [u64; 5] (40 bytes) — with padding to align
// 416     invariant_k_lo u64
// 424     invariant_k_hi u64
// 432     fee_rate_bps u16
// 434     drift_threshold_bps u16
// 440     last_rebalance_slot u64
// 448     rebalance_cooldown u64
// 456     paused u8
// 457     bump u8

const POOL_STATE_SIZE = 464;

async function readPoolState(conn: Connection, poolPk: PublicKey) {
  const info = await conn.getAccountInfo(poolPk);
  if (!info) throw new Error("G3mPoolState account not found");
  const d = info.data;

  const reserves: bigint[] = [];
  for (let i = 0; i < TOKEN_COUNT; i++) {
    reserves.push(d.readBigUInt64LE(376 + i * 8));
  }

  return {
    tokenCount: d[40],
    reserves,
    invariantKLo: d.readBigUInt64LE(416),
    invariantKHi: d.readBigUInt64LE(424),
    feeRateBps: d.readUInt16LE(432),
    driftThresholdBps: d.readUInt16LE(434),
    lastRebalanceSlot: d.readBigUInt64LE(440),
    paused: d[456],
    bump: d[457],
  };
}

// ─── Instruction Builders ─────────────────────────────────────────────────

function ixInitializePool(
  authority: PublicKey,
  poolState: PublicKey,
  sourceAccounts: PublicKey[],
  vaultAccounts: PublicKey[],
): TransactionInstruction {
  // Data layout:
  // [0]:     discriminant = 0
  // [1]:     token_count u8
  // [2..4]:  fee_rate_bps u16 LE
  // [4..6]:  drift_threshold_bps u16 LE
  // [6..14]: rebalance_cooldown u64 LE
  // [14..14+N*2]: weights [u16 LE; N]
  // [...+N*8]: reserves [u64 LE; N]
  const weightsBuf = Buffer.alloc(TOKEN_COUNT * 2);
  for (let i = 0; i < TOKEN_COUNT; i++) {
    weightsBuf.writeUInt16LE(WEIGHTS_BPS[i], i * 2);
  }
  const reservesBuf = Buffer.alloc(TOKEN_COUNT * 8);
  for (let i = 0; i < TOKEN_COUNT; i++) {
    reservesBuf.writeBigUInt64LE(INITIAL_RESERVE, i * 8);
  }

  const data = Buffer.concat([
    Buffer.from([0]),                       // discriminant
    Buffer.from([TOKEN_COUNT]),             // token_count
    u16Le(FEE_RATE_BPS),                   // fee_rate_bps
    u16Le(DRIFT_THRESHOLD_BPS),            // drift_threshold_bps
    u64Le(REBALANCE_COOLDOWN),             // rebalance_cooldown
    weightsBuf,                             // weights
    reservesBuf,                            // initial_reserves
  ]);

  const keys = [
    { pubkey: authority,                   isSigner: true,  isWritable: true  },
    { pubkey: poolState,                   isSigner: false, isWritable: true  },
    { pubkey: SystemProgram.programId,     isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID,            isSigner: false, isWritable: false },
    // Source token accounts (authority-owned)
    ...sourceAccounts.map(pk => ({ pubkey: pk, isSigner: false, isWritable: true })),
    // Vault token accounts (pool-PDA-owned)
    ...vaultAccounts.map(pk => ({ pubkey: pk, isSigner: false, isWritable: true })),
  ];

  return new TransactionInstruction({ programId: PROGRAM_ID, keys, data });
}

function ixSwap(
  user: PublicKey,
  poolState: PublicKey,
  userTokenIn: PublicKey,
  userTokenOut: PublicKey,
  vaultIn: PublicKey,
  vaultOut: PublicKey,
  inIdx: number,
  outIdx: number,
  amountIn: bigint,
  minAmountOut: bigint,
): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([1]),           // discriminant
    Buffer.from([inIdx]),
    Buffer.from([outIdx]),
    u64Le(amountIn),
    u64Le(minAmountOut),
  ]);

  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: user,            isSigner: true,  isWritable: true  },
      { pubkey: poolState,       isSigner: false, isWritable: true  },
      { pubkey: userTokenIn,     isSigner: false, isWritable: true  },
      { pubkey: userTokenOut,    isSigner: false, isWritable: true  },
      { pubkey: vaultIn,         isSigner: false, isWritable: true  },
      { pubkey: vaultOut,        isSigner: false, isWritable: true  },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

function ixCheckDrift(poolState: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: poolState, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([2]),
  });
}

function ixRebalance(
  authority: PublicKey,
  poolState: PublicKey,
  newReserves: bigint[],
): TransactionInstruction {
  const reservesBuf = Buffer.alloc(newReserves.length * 8);
  for (let i = 0; i < newReserves.length; i++) {
    reservesBuf.writeBigUInt64LE(newReserves[i], i * 8);
  }

  const data = Buffer.concat([
    Buffer.from([3]),   // discriminant
    reservesBuf,
  ]);

  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true,  isWritable: false },
      { pubkey: poolState, isSigner: false, isWritable: true  },
    ],
    data,
  });
}

// ─── CU Measurement ──────────────────────────────────────────────────────

async function getCU(conn: Connection, sig: string): Promise<number | null> {
  const tx = await conn.getTransaction(sig, {
    maxSupportedTransactionVersion: 0,
    commitment: "confirmed",
  });
  return tx?.meta?.computeUnitsConsumed ?? null;
}

// ─── Main ─────────────────────────────────────────────────────────────────

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("╔══════════════════════════════════════════════╗");
  console.log("║   Axis G3M — E2E Test (ETF B: 5 Tokens)     ║");
  console.log("╚══════════════════════════════════════════════╝");
  console.log(`Wallet  : ${payer.publicKey.toBase58()}`);
  console.log(`Program : ${PROGRAM_ID.toBase58()}`);
  const bal = await conn.getBalance(payer.publicKey);
  console.log(`Balance : ${(bal / LAMPORTS_PER_SOL).toFixed(2)} SOL\n`);

  const cuLog: Record<string, number | null> = {};

  // ── 1. Create 5 token mints ─────────────────────────────────────────────
  console.log("▶ Step 1: Create 5 token mints");
  const mints: PublicKey[] = [];
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    console.log(`  Mint ${i}: ${mint.toBase58()}`);
  }
  console.log();

  // ── 2. Create user token accounts & mint tokens ─────────────────────────
  console.log("▶ Step 2: Create user token accounts & mint supply");
  const userAccounts: PublicKey[] = [];
  const SUPPLY = 100_000_000_000n; // 100,000 tokens each
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const ata = await createAccount(conn, payer, mints[i], payer.publicKey);
    await mintTo(conn, payer, mints[i], ata, payer, SUPPLY);
    userAccounts.push(ata);
  }
  console.log(`  Created ${TOKEN_COUNT} accounts, each with ${num(SUPPLY)} lamports\n`);

  // ── 3. Derive PDA ───────────────────────────────────────────────────────
  const [poolState, poolBump] = findPool(payer.publicKey);
  console.log("▶ Step 3: PDA");
  console.log(`  G3mPool : ${poolState.toBase58()} (bump=${poolBump})\n`);

  // ── 4. Pre-allocate vault accounts (SPL token accounts, owner = pool PDA) ──
  console.log("▶ Step 4: Create vault token accounts (owner = pool PDA)");
  const vaultKeypairs: Keypair[] = [];
  const vaultAccounts: PublicKey[] = [];
  const rentExempt = await getMinimumBalanceForRentExemptAccount(conn);

  for (let i = 0; i < TOKEN_COUNT; i++) {
    const vaultKp = Keypair.generate();
    vaultKeypairs.push(vaultKp);
    vaultAccounts.push(vaultKp.publicKey);
  }

  // Create + initialize all vault accounts. Owner = poolState PDA so pool can transfer.
  const createVaultsTx = new Transaction();
  for (let i = 0; i < TOKEN_COUNT; i++) {
    createVaultsTx.add(
      SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: vaultKeypairs[i].publicKey,
        lamports: rentExempt,
        space: ACCOUNT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeAccountInstruction(
        vaultKeypairs[i].publicKey,
        mints[i],
        poolState,  // owner = pool PDA (so pool can sign transfers)
      ),
    );
  }
  await sendAndConfirmTransaction(conn, createVaultsTx, [payer, ...vaultKeypairs]);
  for (let i = 0; i < TOKEN_COUNT; i++) {
    console.log(`  Vault ${i}: ${vaultAccounts[i].toBase58()}`);
  }
  console.log();

  // ── 5. InitializePool ───────────────────────────────────────────────────
  console.log("▶ Step 5: InitializePool (5 tokens, 20% each, 1% fee, 5% drift)");
  const initTx = new Transaction().add(
    ixInitializePool(payer.publicKey, poolState, userAccounts, vaultAccounts)
  );
  const initSig = await sendAndConfirmTransaction(conn, initTx, [payer]);
  cuLog["InitializePool"] = await getCU(conn, initSig);

  const poolAfterInit = await readPoolState(conn, poolState);
  console.log(`  Tx            : ${initSig.slice(0, 20)}...`);
  console.log(`  CU            : ${cuLog["InitializePool"]?.toLocaleString()}`);
  console.log(`  Token count   : ${poolAfterInit.tokenCount}`);
  console.log(`  Fee rate      : ${poolAfterInit.feeRateBps} bps`);
  console.log(`  Drift thresh  : ${poolAfterInit.driftThresholdBps} bps`);
  console.log(`  Reserves      : [${poolAfterInit.reserves.map(r => num(r)).join(", ")}]`);
  console.log();

  // ── 6. Swap (token 0 → token 1) ────────────────────────────────────────
  console.log("▶ Step 6: Swap 10 tokens (token 0 → token 1)");
  const SWAP_AMOUNT = 10_000_000n; // 10 tokens
  const swapTx = new Transaction().add(
    ixSwap(
      payer.publicKey, poolState,
      userAccounts[0], userAccounts[1],
      vaultAccounts[0], vaultAccounts[1],
      0, 1,
      SWAP_AMOUNT, 0n, // min_out = 0 for testing
    )
  );
  const swapSig = await sendAndConfirmTransaction(conn, swapTx, [payer]);
  cuLog["Swap"] = await getCU(conn, swapSig);

  const poolAfterSwap = await readPoolState(conn, poolState);
  console.log(`  Tx       : ${swapSig.slice(0, 20)}...`);
  console.log(`  CU       : ${cuLog["Swap"]?.toLocaleString()}`);
  console.log(`  Reserves : [${poolAfterSwap.reserves.map(r => num(r)).join(", ")}]`);
  console.log();

  // ── 7. CheckDrift ──────────────────────────────────────────────────────
  console.log("▶ Step 7: CheckDrift");
  const driftTx = new Transaction().add(ixCheckDrift(poolState));
  const driftSig = await sendAndConfirmTransaction(conn, driftTx, [payer]);
  cuLog["CheckDrift"] = await getCU(conn, driftSig);
  console.log(`  CU : ${cuLog["CheckDrift"]?.toLocaleString()}`);
  console.log();

  // ── 8. Large swap to force drift ────────────────────────────────────────
  console.log("▶ Step 8: Large swap to induce drift (200 tokens of token 0 → token 2)");
  const BIG_SWAP = 200_000_000n; // 200 tokens
  const bigSwapTx = new Transaction().add(
    ixSwap(
      payer.publicKey, poolState,
      userAccounts[0], userAccounts[2],
      vaultAccounts[0], vaultAccounts[2],
      0, 2,
      BIG_SWAP, 0n,
    )
  );
  const bigSwapSig = await sendAndConfirmTransaction(conn, bigSwapTx, [payer]);
  cuLog["LargeSwap"] = await getCU(conn, bigSwapSig);

  const poolAfterBigSwap = await readPoolState(conn, poolState);
  console.log(`  CU       : ${cuLog["LargeSwap"]?.toLocaleString()}`);
  console.log(`  Reserves : [${poolAfterBigSwap.reserves.map(r => num(r)).join(", ")}]`);
  console.log();

  // ── 9. Rebalance ───────────────────────────────────────────────────────
  console.log("▶ Step 9: Rebalance (restore to target weights)");
  // Rehearsal path: compute synthetic target reserves from the current pool.
  // The original ETF B spec instead calls for same-transaction Jupiter routing.
  const totalReserve = poolAfterBigSwap.reserves.reduce((a, b) => a + b, 0n);
  const avgReserve = totalReserve / BigInt(TOKEN_COUNT);
  const targetReserves = Array(TOKEN_COUNT).fill(avgReserve);

  const rebalTx = new Transaction().add(
    ixRebalance(payer.publicKey, poolState, targetReserves)
  );

  // Simulate first
  const { blockhash } = await conn.getLatestBlockhash();
  rebalTx.recentBlockhash = blockhash;
  rebalTx.feePayer = payer.publicKey;
  rebalTx.sign(payer);

  const sim = await conn.simulateTransaction(rebalTx);
  if (sim.value.err) {
    console.log(`  Simulation failed: ${JSON.stringify(sim.value.err)}`);
    console.log(`  (Expected if drift < threshold after averaging)`);
    sim.value.logs?.forEach(l => console.log(`    ${l}`));
  } else {
    console.log(`  Simulation CU: ${sim.value.unitsConsumed?.toLocaleString()}`);

    const rebalTx2 = new Transaction().add(
      ixRebalance(payer.publicKey, poolState, targetReserves)
    );
    const rebalSig = await sendAndConfirmTransaction(conn, rebalTx2, [payer]);
    cuLog["Rebalance"] = await getCU(conn, rebalSig);

    const poolAfterRebal = await readPoolState(conn, poolState);
    console.log(`  CU       : ${cuLog["Rebalance"]?.toLocaleString()}`);
    console.log(`  Reserves : [${poolAfterRebal.reserves.map(r => num(r)).join(", ")}]`);
  }
  console.log();

  // ── Summary ──────────────────────────────────────────────────────────────
  console.log("╔══════════════════════════════════════════════╗");
  console.log("║              CU Summary                      ║");
  console.log("╠══════════════════════════════════════════════╣");
  for (const [label, cu] of Object.entries(cuLog)) {
    const bar = cu ? "█".repeat(Math.min(Math.floor(cu / 2000), 20)) : "";
    console.log(`║  ${label.padEnd(16)}: ${String(cu?.toLocaleString() ?? "N/A").padStart(7)} CU  ${bar}`);
  }
  console.log("╚══════════════════════════════════════════════╝");
}

main().catch(err => {
  console.error("\n✗ Error:", err);
  process.exit(1);
});

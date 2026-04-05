/**
 * Axis Protocol — Legacy PFDA vs G3M CU Benchmark
 *
 * NOTE: This benchmark uses the legacy 2-token PFDA program (pfda-amm),
 * NOT the canonical 3-token ETF A path (pfda-amm-3).
 * It is retained for CU regression testing of the original AMM design.
 *
 * For the canonical A/B test, use:
 *   ETF A: pfda-amm/programs/pfda-amm-3/client/oracle-bid-e2e.ts
 *   ETF B: axis-g3m/client/e2e-devnet.ts
 *
 * Deploys and benchmarks on a local validator:
 *   ETF A (legacy): PFDA batch auction (pfda-amm) — 2-token pool
 *   ETF B: G3M continuous (axis-g3m) — 5-token memecoin pool
 *
 * Prerequisites:
 *   solana-test-validator running with both programs loaded:
 *   solana-test-validator \
 *     --bpf-program CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk ../pfda-amm/target/deploy/pfda_amm.so \
 *     --bpf-program 65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi ../axis-g3m/target/deploy/axis_g3m.so \
 *     --reset
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

// ─── Program IDs ──────────────────────────────────────────────────────────

const PFDA_PROGRAM_ID = new PublicKey("CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk");
const G3M_PROGRAM_ID = new PublicKey("65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi");
const RPC_URL = "http://localhost:8899";

// ─── Shared Config ────────────────────────────────────────────────────────

const PFDA_WINDOW_SLOTS = 10n;
const PFDA_BASE_FEE_BPS = 30;
const PFDA_FEE_DISCOUNT = 10;
const PFDA_WEIGHT_A = 500_000; // 50/50

const G3M_TOKEN_COUNT = 5;
const G3M_FEE_BPS = 100;
const G3M_DRIFT_THRESHOLD = 500;
const G3M_COOLDOWN = 0n;
const G3M_WEIGHTS = [2000, 2000, 2000, 2000, 2000];

// ─── Utilities ────────────────────────────────────────────────────────────

function loadPayer(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }

async function getCU(conn: Connection, sig: string): Promise<number> {
  const tx = await conn.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
  return tx?.meta?.computeUnitsConsumed ?? 0;
}

async function waitForSlot(conn: Connection, targetSlot: bigint) {
  while (true) {
    const s = BigInt(await conn.getSlot("confirmed"));
    if (s >= targetSlot) return;
    await new Promise(r => setTimeout(r, 300));
  }
}

// ─── PFDA PDA Helpers ─────────────────────────────────────────────────────

function findPfdaPool(mintA: PublicKey, mintB: PublicKey) {
  return PublicKey.findProgramAddressSync([Buffer.from("pool"), mintA.toBuffer(), mintB.toBuffer()], PFDA_PROGRAM_ID);
}
function findPfdaQueue(pool: PublicKey, batchId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("queue"), pool.toBuffer(), u64Le(batchId)], PFDA_PROGRAM_ID);
}
function findPfdaHistory(pool: PublicKey, batchId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("history"), pool.toBuffer(), u64Le(batchId)], PFDA_PROGRAM_ID);
}
function findPfdaTicket(pool: PublicKey, user: PublicKey, batchId: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("ticket"), pool.toBuffer(), user.toBuffer(), u64Le(batchId)], PFDA_PROGRAM_ID);
}

// ─── G3M PDA Helpers ──────────────────────────────────────────────────────

function findG3mPool(authority: PublicKey) {
  return PublicKey.findProgramAddressSync([Buffer.from("g3m_pool"), authority.toBuffer()], G3M_PROGRAM_ID);
}

// ─── PFDA Instruction Builders ────────────────────────────────────────────

function ixPfdaInit(payer: PublicKey, pool: PublicKey, queue: PublicKey, mintA: PublicKey, mintB: PublicKey, vA: PublicKey, vB: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: PFDA_PROGRAM_ID,
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue, isSigner: false, isWritable: true },
      { pubkey: mintA, isSigner: false, isWritable: false },
      { pubkey: mintB, isSigner: false, isWritable: false },
      { pubkey: vA, isSigner: false, isWritable: true },
      { pubkey: vB, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0]), u16Le(PFDA_BASE_FEE_BPS), u16Le(PFDA_FEE_DISCOUNT), u64Le(PFDA_WINDOW_SLOTS), u32Le(PFDA_WEIGHT_A)]),
  });
}

function ixPfdaAddLiq(user: PublicKey, pool: PublicKey, vA: PublicKey, vB: PublicKey, uA: PublicKey, uB: PublicKey, amtA: bigint, amtB: bigint): TransactionInstruction {
  return new TransactionInstruction({
    programId: PFDA_PROGRAM_ID,
    keys: [
      { pubkey: user, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: vA, isSigner: false, isWritable: true },
      { pubkey: vB, isSigner: false, isWritable: true },
      { pubkey: uA, isSigner: false, isWritable: true },
      { pubkey: uB, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([4]), u64Le(amtA), u64Le(amtB)]),
  });
}

function ixPfdaSwap(user: PublicKey, pool: PublicKey, queue: PublicKey, ticket: PublicKey, uA: PublicKey, uB: PublicKey, vA: PublicKey, vB: PublicKey, amtA: bigint): TransactionInstruction {
  return new TransactionInstruction({
    programId: PFDA_PROGRAM_ID,
    keys: [
      { pubkey: user, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: queue, isSigner: false, isWritable: true },
      { pubkey: ticket, isSigner: false, isWritable: true },
      { pubkey: uA, isSigner: false, isWritable: true },
      { pubkey: uB, isSigner: false, isWritable: true },
      { pubkey: vA, isSigner: false, isWritable: true },
      { pubkey: vB, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([1]), u64Le(amtA), u64Le(0n), u64Le(0n)]),
  });
}

function ixPfdaClear(cranker: PublicKey, pool: PublicKey, queue: PublicKey, history: PublicKey, nextQueue: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: PFDA_PROGRAM_ID,
    keys: [
      { pubkey: cranker, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue, isSigner: false, isWritable: true },
      { pubkey: history, isSigner: false, isWritable: true },
      { pubkey: nextQueue, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([2]),
  });
}

function ixPfdaClaim(user: PublicKey, pool: PublicKey, history: PublicKey, ticket: PublicKey, vA: PublicKey, vB: PublicKey, uA: PublicKey, uB: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: PFDA_PROGRAM_ID,
    keys: [
      { pubkey: user, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: history, isSigner: false, isWritable: false },
      { pubkey: ticket, isSigner: false, isWritable: true },
      { pubkey: vA, isSigner: false, isWritable: true },
      { pubkey: vB, isSigner: false, isWritable: true },
      { pubkey: uA, isSigner: false, isWritable: true },
      { pubkey: uB, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  });
}

// ─── G3M Instruction Builders ─────────────────────────────────────────────

function ixG3mInit(authority: PublicKey, pool: PublicKey, sources: PublicKey[], vaults: PublicKey[], reserves: bigint[]): TransactionInstruction {
  const wBuf = Buffer.alloc(G3M_TOKEN_COUNT * 2);
  for (let i = 0; i < G3M_TOKEN_COUNT; i++) wBuf.writeUInt16LE(G3M_WEIGHTS[i], i * 2);
  const rBuf = Buffer.alloc(G3M_TOKEN_COUNT * 8);
  for (let i = 0; i < G3M_TOKEN_COUNT; i++) rBuf.writeBigUInt64LE(reserves[i], i * 8);

  return new TransactionInstruction({
    programId: G3M_PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...sources.map(pk => ({ pubkey: pk, isSigner: false, isWritable: true })),
      ...vaults.map(pk => ({ pubkey: pk, isSigner: false, isWritable: true })),
    ],
    data: Buffer.concat([Buffer.from([0, G3M_TOKEN_COUNT]), u16Le(G3M_FEE_BPS), u16Le(G3M_DRIFT_THRESHOLD), u64Le(G3M_COOLDOWN), wBuf, rBuf]),
  });
}

function ixG3mSwap(user: PublicKey, pool: PublicKey, uIn: PublicKey, uOut: PublicKey, vIn: PublicKey, vOut: PublicKey, inIdx: number, outIdx: number, amt: bigint): TransactionInstruction {
  return new TransactionInstruction({
    programId: G3M_PROGRAM_ID,
    keys: [
      { pubkey: user, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: uIn, isSigner: false, isWritable: true },
      { pubkey: uOut, isSigner: false, isWritable: true },
      { pubkey: vIn, isSigner: false, isWritable: true },
      { pubkey: vOut, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([1, inIdx, outIdx]), u64Le(amt), u64Le(0n)]),
  });
}

function ixG3mDrift(pool: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: G3M_PROGRAM_ID,
    keys: [{ pubkey: pool, isSigner: false, isWritable: false }],
    data: Buffer.from([2]),
  });
}

// ─── Main Benchmark ───────────────────────────────────────────────────────

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("╔══════════════════════════════════════════════════════════╗");
  console.log("║   Axis Protocol — Legacy PFDA vs G3M CU Benchmark        ║");
  console.log("║   PFDA: Legacy 2-token (NOT canonical ETF A)             ║");
  console.log("║   G3M:  5-token (canonical ETF B)                        ║");
  console.log("╚══════════════════════════════════════════════════════════╝");

  const bal = await conn.getBalance(payer.publicKey);
  console.log(`Wallet : ${payer.publicKey.toBase58()}`);
  console.log(`Balance: ${(bal / LAMPORTS_PER_SOL).toFixed(0)} SOL\n`);

  const cuA: Record<string, number> = {};
  const cuB: Record<string, number> = {};

  // ═════════════════════ ETF A: PFDA ═════════════════════════════════════
  console.log("━━━ ETF A: PFDA Batch Auction ━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  const mintA = await createMint(conn, payer, payer.publicKey, null, 6);
  const mintB = await createMint(conn, payer, payer.publicKey, null, 6);
  const userTA = await createAccount(conn, payer, mintA, payer.publicKey);
  const userTB = await createAccount(conn, payer, mintB, payer.publicKey);
  await mintTo(conn, payer, mintA, userTA, payer, 10_000_000_000n);
  await mintTo(conn, payer, mintB, userTB, payer, 10_000_000_000n);

  const [poolA] = findPfdaPool(mintA, mintB);
  const [queue0] = findPfdaQueue(poolA, 0n);
  const [history0] = findPfdaHistory(poolA, 0n);
  const [queue1] = findPfdaQueue(poolA, 1n);
  const [ticket] = findPfdaTicket(poolA, payer.publicKey, 0n);

  // Create vaults
  const rentExempt = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultAKp = Keypair.generate();
  const vaultBKp = Keypair.generate();
  const createVaultsTx = new Transaction().add(
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: vaultAKp.publicKey, lamports: rentExempt, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: vaultBKp.publicKey, lamports: rentExempt, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
  );
  await sendAndConfirmTransaction(conn, createVaultsTx, [payer, vaultAKp, vaultBKp]);

  // InitializePool
  let sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixPfdaInit(payer.publicKey, poolA, queue0, mintA, mintB, vaultAKp.publicKey, vaultBKp.publicKey)
  ), [payer]);
  cuA["InitPool"] = await getCU(conn, sig);
  console.log(`  InitPool     : ${cuA["InitPool"].toLocaleString()} CU`);

  // Read window end
  const poolData = (await conn.getAccountInfo(poolA))!.data;
  const windowEnd = poolData.readBigUInt64LE(192);

  // AddLiquidity
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixPfdaAddLiq(payer.publicKey, poolA, vaultAKp.publicKey, vaultBKp.publicKey, userTA, userTB, 1_000_000_000n, 1_000_000_000n)
  ), [payer]);
  cuA["AddLiquidity"] = await getCU(conn, sig);
  console.log(`  AddLiquidity : ${cuA["AddLiquidity"].toLocaleString()} CU`);

  // SwapRequest
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixPfdaSwap(payer.publicKey, poolA, queue0, ticket, userTA, userTB, vaultAKp.publicKey, vaultBKp.publicKey, 10_000_000n)
  ), [payer]);
  cuA["Deposit"] = await getCU(conn, sig);
  console.log(`  Deposit      : ${cuA["Deposit"].toLocaleString()} CU`);

  // Wait for window
  await waitForSlot(conn, windowEnd);

  // ClearBatch
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixPfdaClear(payer.publicKey, poolA, queue0, history0, queue1)
  ), [payer]);
  cuA["ClearBatch"] = await getCU(conn, sig);
  console.log(`  ClearBatch   : ${cuA["ClearBatch"].toLocaleString()} CU  ★`);

  // Claim
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixPfdaClaim(payer.publicKey, poolA, history0, ticket, vaultAKp.publicKey, vaultBKp.publicKey, userTA, userTB)
  ), [payer]);
  cuA["Claim"] = await getCU(conn, sig);
  console.log(`  Claim        : ${cuA["Claim"].toLocaleString()} CU`);

  // ═════════════════════ ETF B: G3M ══════════════════════════════════════
  console.log("\n━━━ ETF B: G3M Continuous AMM ━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  const g3mMints: PublicKey[] = [];
  const g3mUserAccounts: PublicKey[] = [];
  for (let i = 0; i < G3M_TOKEN_COUNT; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    g3mMints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 100_000_000_000n);
    g3mUserAccounts.push(ata);
  }

  const [g3mPool] = findG3mPool(payer.publicKey);

  // Create vault accounts
  const g3mVaultKps: Keypair[] = [];
  const g3mVaults: PublicKey[] = [];
  const createG3mVaultsTx = new Transaction();
  for (let i = 0; i < G3M_TOKEN_COUNT; i++) {
    const kp = Keypair.generate();
    g3mVaultKps.push(kp);
    g3mVaults.push(kp.publicKey);
    createG3mVaultsTx.add(
      SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: rentExempt, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
      createInitializeAccountInstruction(kp.publicKey, g3mMints[i], g3mPool),
    );
  }
  await sendAndConfirmTransaction(conn, createG3mVaultsTx, [payer, ...g3mVaultKps]);

  // InitializePool
  const INIT_RESERVE = 1_000_000_000n;
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixG3mInit(payer.publicKey, g3mPool, g3mUserAccounts, g3mVaults, Array(G3M_TOKEN_COUNT).fill(INIT_RESERVE))
  ), [payer]);
  cuB["InitPool"] = await getCU(conn, sig);
  console.log(`  InitPool     : ${cuB["InitPool"].toLocaleString()} CU`);

  // Swap (small)
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixG3mSwap(payer.publicKey, g3mPool, g3mUserAccounts[0], g3mUserAccounts[1], g3mVaults[0], g3mVaults[1], 0, 1, 10_000_000n)
  ), [payer]);
  cuB["Swap"] = await getCU(conn, sig);
  console.log(`  Swap         : ${cuB["Swap"].toLocaleString()} CU`);

  // CheckDrift
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixG3mDrift(g3mPool)
  ), [payer]);
  cuB["CheckDrift"] = await getCU(conn, sig);
  console.log(`  CheckDrift   : ${cuB["CheckDrift"].toLocaleString()} CU`);

  // Large swap to induce drift
  sig = await sendAndConfirmTransaction(conn, new Transaction().add(
    ixG3mSwap(payer.publicKey, g3mPool, g3mUserAccounts[0], g3mUserAccounts[2], g3mVaults[0], g3mVaults[2], 0, 2, 200_000_000n)
  ), [payer]);
  cuB["LargeSwap"] = await getCU(conn, sig);
  console.log(`  LargeSwap    : ${cuB["LargeSwap"].toLocaleString()} CU`);

  // ═══════════════════════ Comparison ═══════════════════════════════════════
  console.log("\n╔════════════════════════════════════════════════════════════════╗");
  console.log("║              A/B Test CU Comparison                            ║");
  console.log("╠════════════════════════════════════════════════════════════════╣");
  console.log("║  Instruction     │  ETF A (PFDA)  │  ETF B (G3M)  │  Target  ║");
  console.log("╠════════════════════════════════════════════════════════════════╣");

  const rows: [string, string, string, string][] = [
    ["InitPool",     fmt(cuA["InitPool"]),     fmt(cuB["InitPool"]),     "—"],
    ["Deposit/Swap", fmt(cuA["Deposit"]),       fmt(cuB["Swap"]),         "< 5k / 30k"],
    ["ClearBatch",   fmt(cuA["ClearBatch"]),    "N/A",                    "< 40k"],
    ["Claim",        fmt(cuA["Claim"]),          "N/A",                    "< 16k"],
    ["CheckDrift",   "N/A",                      fmt(cuB["CheckDrift"]),   "< 10k"],
    ["LargeSwap",    "N/A",                      fmt(cuB["LargeSwap"]),    "< 30k"],
    ["AddLiquidity", fmt(cuA["AddLiquidity"]),   "N/A",                    "—"],
  ];

  for (const [label, a, b, target] of rows) {
    console.log(`║  ${label.padEnd(15)}│  ${a.padStart(12)}  │  ${b.padStart(11)}  │  ${target.padStart(6)}  ║`);
  }

  console.log("╚════════════════════════════════════════════════════════════════╝");

  // Budget check
  console.log("\n━━━ Budget Check ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  check("ETF A: Deposit    < 5,000 CU",   cuA["Deposit"],    5000);
  check("ETF A: ClearBatch < 40,000 CU",  cuA["ClearBatch"], 40000);
  check("ETF A: Claim      < 16,000 CU",  cuA["Claim"],      16000);
  check("ETF B: Swap       < 30,000 CU",  cuB["Swap"],       30000);
  check("ETF B: CheckDrift < 10,000 CU",  cuB["CheckDrift"], 10000);
}

function fmt(n: number | undefined): string {
  return n !== undefined ? n.toLocaleString() : "—";
}

function check(label: string, actual: number | undefined, limit: number) {
  if (actual === undefined) { console.log(`  ? ${label} — no data`); return; }
  const ok = actual <= limit;
  const icon = ok ? "✓" : "✗";
  console.log(`  ${icon} ${label}  →  ${actual.toLocaleString()} CU  ${ok ? "" : "OVER BUDGET"}`);
}

main().catch(err => {
  console.error("\n✗ Error:", err);
  process.exit(1);
});

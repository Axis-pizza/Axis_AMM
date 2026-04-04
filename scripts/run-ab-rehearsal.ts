/**
 * Axis Protocol — A/B Test Rehearsal Script
 *
 * One-command orchestrator that runs the ETF A canonical devnet flow and the
 * current ETF B rehearsal flow, then prints a compact summary.
 *
 * Usage:
 *   npx ts-node run-ab-rehearsal.ts [--collect]
 *
 * With --collect, also starts the metrics collector in the background.
 *
 * Output can be pasted directly into a status update.
 */

import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, createInitializeAccountInstruction,
  mintTo, getAccount, TOKEN_PROGRAM_ID, ACCOUNT_SIZE,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";
import { execSync, spawn } from "child_process";

// ─── Config ──────────────────────────────────────────────────────────────

const PFDA3_PROGRAM_ID = new PublicKey("DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf");
const G3M_PROGRAM_ID = new PublicKey("65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi");
const RPC_URL = "https://api.devnet.solana.com";
const SWITCHBOARD_FEED = new PublicKey("BV9mGAy5MJLYWJT5HF74izYKjF9CmL4BqkswfTu9gW2w");

function loadPayer(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }
function tokenAmount(value: bigint | number): bigint {
  return typeof value === "bigint" ? value : BigInt(value);
}

type DriftMetrics = {
  maxDriftBps: bigint;
  maxDriftIdx: number;
  thresholdBps: number;
  needsRebalance: boolean;
  invariantKLo: bigint;
};

function decodeDriftMetrics(buf: Buffer): DriftMetrics {
  return {
    maxDriftBps: buf.readBigUInt64LE(0),
    maxDriftIdx: buf[8],
    thresholdBps: buf.readUInt16LE(9),
    needsRebalance: buf[11] !== 0,
    invariantKLo: buf.readBigUInt64LE(12),
  };
}

function decodeDriftFromLogs(programId: PublicKey, logs: string[] | null | undefined): DriftMetrics | null {
  if (!logs) return null;

  const prefix = `Program return: ${programId.toBase58()} `;
  for (const line of logs) {
    if (!line.startsWith(prefix)) continue;
    const encoded = line.slice(prefix.length).trim();
    if (!encoded) continue;
    return decodeDriftMetrics(Buffer.from(encoded, "base64"));
  }

  return null;
}

async function readDriftMetricsFromSignature(
  conn: Connection,
  programId: PublicKey,
  sig: string,
): Promise<DriftMetrics | null> {
  const tx = await conn.getTransaction(sig, {
    maxSupportedTransactionVersion: 0,
    commitment: "confirmed",
  });
  const meta = tx?.meta as
    | ({
        returnData?: { data?: [string, string] };
        logMessages?: string[] | null;
      } & Record<string, unknown>)
    | undefined;
  if (!meta) return null;

  if (meta.returnData?.data?.[0]) {
    return decodeDriftMetrics(Buffer.from(meta.returnData.data[0], "base64"));
  }

  return decodeDriftFromLogs(programId, meta.logMessages);
}

async function getCU(conn: Connection, sig: string): Promise<number | null> {
  const tx = await conn.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
  return tx?.meta?.computeUnitsConsumed ?? null;
}

async function waitForSlot(conn: Connection, target: bigint) {
  while (true) {
    const s = BigInt(await conn.getSlot("confirmed"));
    if (s >= target) return;
    await new Promise(r => setTimeout(r, 400));
  }
}

// ─── ETF A: 3-Token PFDA ────────────────────────────────────────────────

interface EtfAResult {
  pass: boolean;
  txSigs: Record<string, string>;
  cu: Record<string, number | null>;
  tokensOut: bigint;
  treasuryDelta: number;
  oracleUsed: boolean;
  bidPaid: boolean;
}

async function runEtfA(conn: Connection, payer: Keypair): Promise<EtfAResult> {
  const result: EtfAResult = {
    pass: false, txSigs: {}, cu: {},
    tokensOut: 0n, treasuryDelta: 0, oracleUsed: false, bidPaid: false,
  };

  const WINDOW = 100n;
  const FEE_BPS = 30;
  const WEIGHTS = [333_333, 333_333, 333_334];
  const BID = 1_000_000n;

  // Treasury
  const treasury = Keypair.generate();
  await sendAndConfirmTransaction(conn,
    new Transaction().add(SystemProgram.transfer({
      fromPubkey: payer.publicKey, toPubkey: treasury.publicKey, lamports: 1_000_000,
    })), [payer]);
  const treasBefore = await conn.getBalance(treasury.publicKey);

  // Mints + accounts
  const mints: PublicKey[] = [];
  const userAccts: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 10_000_000_000n);
    userAccts.push(ata);
  }

  // PDAs
  const [pool] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool3"), mints[0].toBuffer(), mints[1].toBuffer(), mints[2].toBuffer()], PFDA3_PROGRAM_ID);
  const [queue0] = PublicKey.findProgramAddressSync(
    [Buffer.from("queue3"), pool.toBuffer(), u64Le(0n)], PFDA3_PROGRAM_ID);
  const [history0] = PublicKey.findProgramAddressSync(
    [Buffer.from("history3"), pool.toBuffer(), u64Le(0n)], PFDA3_PROGRAM_ID);
  const [queue1] = PublicKey.findProgramAddressSync(
    [Buffer.from("queue3"), pool.toBuffer(), u64Le(1n)], PFDA3_PROGRAM_ID);
  const [ticket] = PublicKey.findProgramAddressSync(
    [Buffer.from("ticket3"), pool.toBuffer(), payer.publicKey.toBuffer(), u64Le(0n)], PFDA3_PROGRAM_ID);

  // Vaults
  const rentExempt = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultKps: Keypair[] = [];
  const vaults: PublicKey[] = [];
  const vTx = new Transaction();
  for (let i = 0; i < 3; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    vaults.push(kp.publicKey);
    vTx.add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey,
      lamports: rentExempt, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
    }));
  }
  await sendAndConfirmTransaction(conn, vTx, [payer, ...vaultKps]);

  // InitPool
  const initData = Buffer.concat([Buffer.from([0]), u16Le(FEE_BPS), u64Le(WINDOW),
    u32Le(WEIGHTS[0]), u32Le(WEIGHTS[1]), u32Le(WEIGHTS[2])]);
  const initSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      { pubkey: treasury.publicKey, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: initData })), [payer]);
  result.txSigs["InitPool"] = initSig;
  result.cu["InitPool"] = await getCU(conn, initSig);

  const poolData = (await conn.getAccountInfo(pool))!.data;
  const windowEnd = poolData.readBigUInt64LE(256);

  // Add liquidity
  const LIQ = 1_000_000_000n;
  for (let i = 0; i < 3; i++) {
    await sendAndConfirmTransaction(conn, new Transaction().add(
      new TransactionInstruction({ programId: TOKEN_PROGRAM_ID, keys: [
        { pubkey: userAccts[i], isSigner: false, isWritable: true },
        { pubkey: vaults[i], isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: true, isWritable: false },
      ], data: Buffer.concat([Buffer.from([3]), u64Le(LIQ)]) })), [payer]);
  }

  // Swap
  const swapData = Buffer.concat([Buffer.from([1, 0]), u64Le(10_000_000n), Buffer.from([1]), u64Le(0n)]);
  const swapSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: ticket, isSigner: false, isWritable: true },
      { pubkey: userAccts[0], isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], data: swapData })), [payer]);
  result.txSigs["Swap"] = swapSig;
  result.cu["Swap"] = await getCU(conn, swapSig);

  // Wait
  await waitForSlot(conn, windowEnd);

  // ClearBatch with oracle + bid
  const feedInfo = await conn.getAccountInfo(SWITCHBOARD_FEED);
  let clearSig: string;
  if (feedInfo) {
    try {
      const clearData = Buffer.concat([Buffer.from([2]), u64Le(BID)]);
      clearSig = await sendAndConfirmTransaction(conn, new Transaction().add(
        new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
          { pubkey: payer.publicKey, isSigner: true, isWritable: true },
          { pubkey: pool, isSigner: false, isWritable: true },
          { pubkey: queue0, isSigner: false, isWritable: true },
          { pubkey: history0, isSigner: false, isWritable: true },
          { pubkey: queue1, isSigner: false, isWritable: true },
          { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
          { pubkey: SWITCHBOARD_FEED, isSigner: false, isWritable: false },
          { pubkey: SWITCHBOARD_FEED, isSigner: false, isWritable: false },
          { pubkey: SWITCHBOARD_FEED, isSigner: false, isWritable: false },
          { pubkey: treasury.publicKey, isSigner: false, isWritable: true },
        ], data: clearData })), [payer]);
      result.oracleUsed = true;
      result.bidPaid = true;
    } catch {
      // Fallback
      clearSig = await sendAndConfirmTransaction(conn, new Transaction().add(
        new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
          { pubkey: payer.publicKey, isSigner: true, isWritable: true },
          { pubkey: pool, isSigner: false, isWritable: true },
          { pubkey: queue0, isSigner: false, isWritable: true },
          { pubkey: history0, isSigner: false, isWritable: true },
          { pubkey: queue1, isSigner: false, isWritable: true },
          { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        ], data: Buffer.from([2]) })), [payer]);
    }
  } else {
    clearSig = await sendAndConfirmTransaction(conn, new Transaction().add(
      new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: pool, isSigner: false, isWritable: true },
        { pubkey: queue0, isSigner: false, isWritable: true },
        { pubkey: history0, isSigner: false, isWritable: true },
        { pubkey: queue1, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ], data: Buffer.from([2]) })), [payer]);
  }
  result.txSigs["ClearBatch"] = clearSig;
  result.cu["ClearBatch"] = await getCU(conn, clearSig);

  // Claim
  const beforeBal = tokenAmount((await getAccount(conn, userAccts[1])).amount);
  const claimSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: PFDA3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: history0, isSigner: false, isWritable: false },
      { pubkey: ticket, isSigner: false, isWritable: true },
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ...userAccts.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: Buffer.from([3]) })), [payer]);
  result.txSigs["Claim"] = claimSig;
  result.cu["Claim"] = await getCU(conn, claimSig);
  result.tokensOut = tokenAmount((await getAccount(conn, userAccts[1])).amount) - beforeBal;

  // Treasury check
  const treasAfter = await conn.getBalance(treasury.publicKey);
  result.treasuryDelta = treasAfter - treasBefore;

  result.pass = result.cu["ClearBatch"] !== null && result.tokensOut > 0n;
  return result;
}

// ─── ETF B: 5-Token G3M ─────────────────────────────────────────────────

interface EtfBResult {
  pass: boolean;
  txSigs: Record<string, string>;
  cu: Record<string, number | null>;
  driftBps: number | null;
  driftToken: number | null;
  needsRebalance: boolean | null;
}

async function runEtfB(conn: Connection, payer: Keypair): Promise<EtfBResult> {
  const result: EtfBResult = {
    pass: false, txSigs: {}, cu: {},
    driftBps: null, driftToken: null, needsRebalance: null,
  };

  const TC = 5;
  const FEE = 100;
  const DRIFT = 500;
  const COOLDOWN = 0n;
  const WEIGHTS = [2000, 2000, 2000, 2000, 2000];
  const INIT_RESERVE = 1_000_000_000n;
  const authority = Keypair.generate();

  await sendAndConfirmTransaction(conn,
    new Transaction().add(SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: authority.publicKey,
      lamports: 1_000_000_000,
    })), [payer]);

  // Mints + accounts
  const mints: PublicKey[] = [];
  const userAccts: PublicKey[] = [];
  for (let i = 0; i < TC; i++) {
    const mint = await createMint(conn, authority, authority.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, authority, mint, authority.publicKey);
    await mintTo(conn, authority, mint, ata, authority, 100_000_000_000n);
    userAccts.push(ata);
  }

  const [poolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("g3m_pool"), authority.publicKey.toBuffer()], G3M_PROGRAM_ID);

  // Vaults
  const rentExempt = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultKps: Keypair[] = [];
  const vaults: PublicKey[] = [];
  const vTx = new Transaction();
  for (let i = 0; i < TC; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    vaults.push(kp.publicKey);
    vTx.add(
      SystemProgram.createAccount({
        fromPubkey: authority.publicKey, newAccountPubkey: kp.publicKey,
        lamports: rentExempt, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeAccountInstruction(kp.publicKey, mints[i], poolState),
    );
  }
  await sendAndConfirmTransaction(conn, vTx, [authority, ...vaultKps]);

  // InitPool
  const wBuf = Buffer.alloc(TC * 2);
  for (let i = 0; i < TC; i++) wBuf.writeUInt16LE(WEIGHTS[i], i * 2);
  const rBuf = Buffer.alloc(TC * 8);
  for (let i = 0; i < TC; i++) rBuf.writeBigUInt64LE(INIT_RESERVE, i * 8);

  const initData = Buffer.concat([
    Buffer.from([0, TC]), u16Le(FEE), u16Le(DRIFT), u64Le(COOLDOWN), wBuf, rBuf,
  ]);
  const initSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: G3M_PROGRAM_ID, keys: [
      { pubkey: authority.publicKey, isSigner: true, isWritable: true },
      { pubkey: poolState, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...userAccts.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ], data: initData })), [authority]);
  result.txSigs["InitPool"] = initSig;
  result.cu["InitPool"] = await getCU(conn, initSig);

  // Swap
  const swapData = Buffer.concat([Buffer.from([1, 0, 1]), u64Le(10_000_000n), u64Le(0n)]);
  const swapSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: G3M_PROGRAM_ID, keys: [
      { pubkey: authority.publicKey, isSigner: true, isWritable: true },
      { pubkey: poolState, isSigner: false, isWritable: true },
      { pubkey: userAccts[0], isSigner: false, isWritable: true },
      { pubkey: userAccts[1], isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: vaults[1], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: swapData })), [authority]);
  result.txSigs["Swap"] = swapSig;
  result.cu["Swap"] = await getCU(conn, swapSig);

  // Big swap to force drift
  const bigSwapData = Buffer.concat([Buffer.from([1, 0, 2]), u64Le(200_000_000n), u64Le(0n)]);
  const bigSig = await sendAndConfirmTransaction(conn, new Transaction().add(
    new TransactionInstruction({ programId: G3M_PROGRAM_ID, keys: [
      { pubkey: authority.publicKey, isSigner: true, isWritable: true },
      { pubkey: poolState, isSigner: false, isWritable: true },
      { pubkey: userAccts[0], isSigner: false, isWritable: true },
      { pubkey: userAccts[2], isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: vaults[2], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: bigSwapData })), [authority]);
  result.txSigs["LargeSwap"] = bigSig;
  result.cu["LargeSwap"] = await getCU(conn, bigSig);

  // CheckDrift via confirmed transaction metadata/logs
  const driftIx = new TransactionInstruction({ programId: G3M_PROGRAM_ID,
    keys: [{ pubkey: poolState, isSigner: false, isWritable: false }],
    data: Buffer.from([2]) });
  const driftSig = await sendAndConfirmTransaction(conn, new Transaction().add(driftIx), [authority]);
  result.txSigs["CheckDrift"] = driftSig;
  result.cu["CheckDrift"] = await getCU(conn, driftSig);

  const driftMetrics = await readDriftMetricsFromSignature(conn, G3M_PROGRAM_ID, driftSig);
  if (driftMetrics) {
    result.driftBps = Number(driftMetrics.maxDriftBps);
    result.driftToken = driftMetrics.maxDriftIdx;
    result.needsRebalance = driftMetrics.needsRebalance;
  }

  result.pass = result.cu["Swap"] !== null && (result.driftBps ?? 0) > 0;
  return result;
}

// ─── Main ────────────────────────────────────────────────────────────────

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("╔══════════════════════════════════════════════════════════╗");
  console.log("║     Axis Protocol — A/B Test Rehearsal (Devnet)          ║");
  console.log("╚══════════════════════════════════════════════════════════╝");
  console.log(`Wallet  : ${payer.publicKey.toBase58()}`);
  console.log(`RPC     : ${RPC_URL}`);
  const bal = await conn.getBalance(payer.publicKey);
  console.log(`Balance : ${(bal / LAMPORTS_PER_SOL).toFixed(2)} SOL`);
  console.log(`Time    : ${new Date().toISOString()}\n`);

  // Optionally start collector
  if (process.argv.includes("--collect")) {
    console.log("Starting metrics collector in background...");
    const child = spawn("npx", ["ts-node", "collect-ab-metrics.ts"], {
      cwd: __dirname, detached: true, stdio: "ignore",
    });
    child.unref();
    console.log(`  PID: ${child.pid}\n`);
  }

  // ─── ETF A ──────────────────────────────────────────────────────────────
  console.log("━━━ ETF A: 3-Token PFDA (oracle + bid) ━━━━━━━━━━━━━━━━━━");
  console.log(`Program: ${PFDA3_PROGRAM_ID.toBase58()}`);
  const etfA = await runEtfA(conn, payer);

  for (const [k, v] of Object.entries(etfA.cu)) {
    console.log(`  ${k.padEnd(12)}: ${String(v?.toLocaleString() ?? "N/A").padStart(8)} CU`);
  }
  console.log(`  Oracle     : ${etfA.oracleUsed ? "YES" : "NO (fallback)"}`);
  console.log(`  Bid paid   : ${etfA.bidPaid ? "YES" : "NO"}`);
  console.log(`  Treasury   : +${etfA.treasuryDelta} lamports`);
  console.log(`  Tokens out : ${etfA.tokensOut.toLocaleString()}`);
  console.log(`  Result     : ${etfA.pass ? "PASS" : "FAIL"}\n`);

  // ─── ETF B ──────────────────────────────────────────────────────────────
  console.log("━━━ ETF B: 5-Token G3M (current rehearsal path) ━━━━━━━━━");
  console.log(`Program: ${G3M_PROGRAM_ID.toBase58()}`);
  const etfB = await runEtfB(conn, payer);

  for (const [k, v] of Object.entries(etfB.cu)) {
    console.log(`  ${k.padEnd(12)}: ${String(v?.toLocaleString() ?? "N/A").padStart(8)} CU`);
  }
  console.log(`  Max drift  : ${etfB.driftBps ?? "N/A"} bps (token ${etfB.driftToken ?? "?"})`);
  console.log(`  Needs rebal: ${etfB.needsRebalance ?? "N/A"}`);
  console.log(`  Result     : ${etfB.pass ? "PASS" : "FAIL"}\n`);

  // ─── Summary ────────────────────────────────────────────────────────────
  console.log("╔══════════════════════════════════════════════════════════╗");
  console.log("║                  A/B REHEARSAL SUMMARY                    ║");
  console.log("╠══════════════════════════════════════════════════════════╣");
  console.log(`║  ETF A (PFDA-3) : ${etfA.pass ? "PASS" : "FAIL"}                                     ║`);
  console.log(`║    ClearBatch CU: ${String(etfA.cu["ClearBatch"]?.toLocaleString() ?? "N/A").padEnd(10)}                            ║`);
  console.log(`║    Oracle       : ${etfA.oracleUsed ? "YES " : "NO  "}                                    ║`);
  console.log(`║    Treasury +   : ${String(etfA.treasuryDelta).padEnd(10)} lamports                   ║`);
  console.log("╠══════════════════════════════════════════════════════════╣");
  console.log(`║  ETF B (G3M)    : ${etfB.pass ? "PASS" : "FAIL"}                                     ║`);
  console.log(`║    Max drift    : ${String(etfB.driftBps ?? "N/A").padEnd(6)} bps                           ║`);
  console.log(`║    Threshold    : ${etfB.needsRebalance ? "EXCEEDED" : "OK      "}                              ║`);
  console.log("╠══════════════════════════════════════════════════════════╣");

  const overall = etfA.pass && etfB.pass;
  console.log(`║  OVERALL        : ${overall ? "PASS" : "FAIL"}                                     ║`);
  console.log("╚══════════════════════════════════════════════════════════╝");

  // Tx sigs for reference
  console.log("\nTransaction signatures:");
  console.log("  ETF A:");
  for (const [k, v] of Object.entries(etfA.txSigs)) {
    console.log(`    ${k}: ${v}`);
  }
  console.log("  ETF B:");
  for (const [k, v] of Object.entries(etfB.txSigs)) {
    console.log(`    ${k}: ${v}`);
  }

  if (!overall) process.exit(1);
}

main().catch(err => {
  console.error("\nError:", err);
  process.exit(1);
});

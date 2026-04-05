/**
 * O(1) Scalability Proof — ClearBatch CU across N=1, 5, 10, 50 intents
 * Proves ClearBatch cost is constant regardless of batch size.
 * Runs on local validator for speed.
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo,
  TOKEN_PROGRAM_ID, ACCOUNT_SIZE, getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf");
const RPC_URL = "http://localhost:8899";

function loadPayer(): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8"))));
}
function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }

function findPool(m0: PublicKey, m1: PublicKey, m2: PublicKey) {
  return PublicKey.findProgramAddressSync([Buffer.from("pool3"), m0.toBuffer(), m1.toBuffer(), m2.toBuffer()], PROGRAM_ID);
}
function findQueue(pool: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("queue3"), pool.toBuffer(), u64Le(id)], PROGRAM_ID);
}
function findHistory(pool: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("history3"), pool.toBuffer(), u64Le(id)], PROGRAM_ID);
}
function findTicket(pool: PublicKey, user: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("ticket3"), pool.toBuffer(), user.toBuffer(), u64Le(id)], PROGRAM_ID);
}

async function getCU(conn: Connection, sig: string): Promise<number> {
  const tx = await conn.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
  return tx?.meta?.computeUnitsConsumed ?? 0;
}

async function runBenchmark(conn: Connection, payer: Keypair, N: number): Promise<number> {
  // Fresh pool for each run
  const mints: PublicKey[] = [];
  const userTokens: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const m = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(m);
    const a = await createAccount(conn, payer, m, payer.publicKey);
    await mintTo(conn, payer, m, a, payer, 100_000_000_000_000n);
    userTokens.push(a);
  }

  const [pool] = findPool(mints[0], mints[1], mints[2]);
  const [queue0] = findQueue(pool, 0n);
  const [history0] = findHistory(pool, 0n);
  const [queue1] = findQueue(pool, 1n);

  const rent = await getMinimumBalanceForRentExemptAccount(conn);
  const vKps: Keypair[] = []; const vaults: PublicKey[] = [];
  const vtx = new Transaction();
  for (let i = 0; i < 3; i++) {
    const kp = Keypair.generate(); vKps.push(kp); vaults.push(kp.publicKey);
    vtx.add(SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: rent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }));
  }
  await sendAndConfirmTransaction(conn, vtx, [payer, ...vKps]);

  // InitPool (window = 300 slots to give time for many intents)
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      { pubkey: payer.publicKey, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0]), u16Le(30), u64Le(300n), u32Le(333333), u32Le(333333), u32Le(333334)]),
  })), [payer]);

  // AddLiquidity
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([4]), u64Le(10_000_000_000n), u64Le(10_000_000_000n), u64Le(10_000_000_000n)]),
  })), [payer]);

  // Submit N swap intents (each from a different keypair)
  for (let i = 0; i < N; i++) {
    const user = Keypair.generate();
    // Fund user
    await sendAndConfirmTransaction(conn, new Transaction().add(
      SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: user.publicKey, lamports: 10_000_000 })
    ), [payer]);

    // Create token account for user
    const userToken = await createAccount(conn, payer, mints[0], user.publicKey);
    await mintTo(conn, payer, mints[0], userToken, payer, 100_000_000n);

    const [ticket] = findTicket(pool, user.publicKey, 0n);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: user.publicKey, isSigner: true, isWritable: true },
        { pubkey: pool, isSigner: false, isWritable: false },
        { pubkey: queue0, isSigner: false, isWritable: true },
        { pubkey: ticket, isSigner: false, isWritable: true },
        { pubkey: userToken, isSigner: false, isWritable: true },
        { pubkey: vaults[0], isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: Buffer.concat([Buffer.from([1, 0]), u64Le(1_000_000n), Buffer.from([1]), u64Le(0n)]),
    })), [user]);
  }

  // Wait for window end
  const poolData = (await conn.getAccountInfo(pool))!.data;
  const windowEnd = poolData.readBigUInt64LE(256);
  while (BigInt(await conn.getSlot("confirmed")) < windowEnd) {
    await new Promise(r => setTimeout(r, 200));
  }

  // ClearBatch — THIS IS WHAT WE MEASURE
  const clearSig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: history0, isSigner: false, isWritable: true },
      { pubkey: queue1, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([2]),
  })), [payer]);

  return getCU(conn, clearSig);
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("╔══════════════════════════════════════════════════╗");
  console.log("║  O(1) Scalability Proof — ClearBatch CU          ║");
  console.log("╚══════════════════════════════════════════════════╝\n");

  const Ns = [1, 5, 10, 50];
  const results: { n: number; cu: number }[] = [];

  for (const n of Ns) {
    process.stdout.write(`  N=${n} intents... `);
    const cu = await runBenchmark(conn, payer, n);
    results.push({ n, cu });
    console.log(`${cu.toLocaleString()} CU`);
  }

  console.log("\n╔═══════════════════════════════════════════╗");
  console.log("║   N Intents  │  ClearBatch CU  │  Delta   ║");
  console.log("╠═══════════════════════════════════════════╣");
  const baseCU = results[0].cu;
  for (const r of results) {
    const delta = r.cu - baseCU;
    const bar = "█".repeat(Math.round(r.cu / 1000));
    console.log(`║   ${String(r.n).padStart(3)}        │  ${String(r.cu.toLocaleString()).padStart(12)}  │  ${delta >= 0 ? "+" : ""}${String(delta.toLocaleString()).padStart(5)}  ║  ${bar}`);
  }
  console.log("╚═══════════════════════════════════════════╝");

  const maxDelta = Math.max(...results.map(r => Math.abs(r.cu - baseCU)));
  if (maxDelta < 1000) {
    console.log(`\n✓ O(1) PROVEN: CU variation is only ${maxDelta} across N=1 to N=${Ns[Ns.length - 1]}`);
  } else {
    console.log(`\n⚠ CU variation of ${maxDelta} — investigate if O(1) holds`);
  }
}
main().catch(err => { console.error("Error:", err); process.exit(1); });

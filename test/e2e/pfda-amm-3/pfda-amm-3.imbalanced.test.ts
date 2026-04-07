import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  createMint, createAccount, createInitializeAccountInstruction,
  mintTo, getAccount, TOKEN_PROGRAM_ID, ACCOUNT_SIZE,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf");
const RPC_URL = "https://api.devnet.solana.com";
const WINDOW_SLOTS = 100n;
const WEIGHTS = [333_333, 333_333, 333_334];

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

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();
  console.log("=== Imbalanced Pool Test (with AddLiquidity) ===\n");

  const mints: PublicKey[] = [];
  const userAccounts: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 50_000_000_000n);
    userAccounts.push(ata);
  }

  const [pool] = findPool(mints[0], mints[1], mints[2]);
  const [queue0] = findQueue(pool, 0n);
  const [history0] = findHistory(pool, 0n);
  const [queue1] = findQueue(pool, 1n);
  const [ticket] = findTicket(pool, payer.publicKey, 0n);

  const rent = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultKps: Keypair[] = [];
  const vaults: PublicKey[] = [];
  const createVaultsTx = new Transaction();
  for (let i = 0; i < 3; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    vaults.push(kp.publicKey);
    createVaultsTx.add(
      SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: rent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
    );
  }
  await sendAndConfirmTransaction(conn, createVaultsTx, [payer, ...vaultKps]);

  // InitPool
  console.log("> InitPool");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      { pubkey: payer.publicKey, isSigner: false, isWritable: false }, // treasury
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0]), u16Le(30), u64Le(WINDOW_SLOTS), u32Le(WEIGHTS[0]), u32Le(WEIGHTS[1]), u32Le(WEIGHTS[2])]),
  })), [payer]);

  // AddLiquidity with imbalanced amounts: [3000, 1000, 1000]
  console.log("> AddLiquidity: [3000, 1000, 1000] tokens (imbalanced)");
  const liq = [3_000_000_000n, 1_000_000_000n, 1_000_000_000n];
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ...userAccounts.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([4]), u64Le(liq[0]), u64Le(liq[1]), u64Le(liq[2])]),
  })), [payer]);

  // Verify reserves are set
  const poolData = (await conn.getAccountInfo(pool))!.data;
  const r0 = poolData.readBigUInt64LE(200);
  const r1 = poolData.readBigUInt64LE(208);
  const r2 = poolData.readBigUInt64LE(216);
  console.log(`  Reserves: [${r0}, ${r1}, ${r2}]`);

  // Swap 10 tokens of token 0 (abundant) → token 1 (scarce)
  console.log("\n> SwapRequest: 10 tokens of token 0 → token 1");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: ticket, isSigner: false, isWritable: true },
      { pubkey: userAccounts[0], isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([1, 0]), u64Le(10_000_000n), Buffer.from([1]), u64Le(0n)]),
  })), [payer]);

  // Wait
  const windowEnd = poolData.readBigUInt64LE(256);
  console.log("> Waiting for slot", windowEnd.toString());
  while (BigInt(await conn.getSlot("confirmed")) < windowEnd) {
    await new Promise(r => setTimeout(r, 400));
  }

  // ClearBatch
  console.log("> ClearBatch");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
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

  // Claim
  console.log("> Claim");
  const beforeBal = (await getAccount(conn, userAccounts[1])).amount;
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: history0, isSigner: false, isWritable: false },
      { pubkey: ticket, isSigner: false, isWritable: true },
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ...userAccounts.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  })), [payer]);

  const afterBal = (await getAccount(conn, userAccounts[1])).amount;
  const received = afterBal - beforeBal;

  console.log("\n=== Results ===");
  console.log("Pool reserves: [3000, 1000, 1000] tokens");
  console.log("Input:  10,000,000 of token 0 (3x more abundant)");
  console.log("Output:", received.toLocaleString(), "of token 1 (scarce)");

  // Token 0 price = (R0_numeraire * W0) / (R0 * W0) = 1.0 (it IS the numeraire)
  // Token 1 price = (R0 * W1) / (R1 * W0) = (3000 * 333333) / (1000 * 333333) = 3.0
  // So swapping 10M of token 0 for token 1: 10M * (price_0 / price_1) * (1 - fee)
  // = 10M * (1.0 / 3.0) * 0.997 = 3,323,333
  const expected = 10_000_000n * 1n * 997n / 3n / 1000n;
  console.log("Expected: ~" + expected.toLocaleString());

  if (received < 5_000_000n) {
    console.log("\n✓ Price correctly reflects 3x imbalance (token 0 is worth ~1/3 of token 1)");
  } else if (received < 10_000_000n) {
    console.log("\n~ Partial price effect but not fully reflecting 3x imbalance");
  } else {
    console.log("\n✗ No price impact from imbalance");
  }
}
main().catch(err => { console.error("Error:", err.message || err); process.exit(1); });

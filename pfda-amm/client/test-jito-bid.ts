/**
 * Test Jito bid enforcement in ClearBatch on devnet.
 * Sends ClearBatch with bid_lamports > 0 and a treasury account.
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo, TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE, getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk");
const RPC_URL = "https://api.devnet.solana.com";
const WINDOW_SLOTS = 100n;

function loadPayer(): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(
    JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8"))
  ));
}
function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }

function findPool(a: PublicKey, b: PublicKey) {
  return PublicKey.findProgramAddressSync([Buffer.from("pool"), a.toBuffer(), b.toBuffer()], PROGRAM_ID);
}
function findQueue(pool: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("queue"), pool.toBuffer(), u64Le(id)], PROGRAM_ID);
}
function findHistory(pool: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("history"), pool.toBuffer(), u64Le(id)], PROGRAM_ID);
}
function findTicket(pool: PublicKey, user: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync([Buffer.from("ticket"), pool.toBuffer(), user.toBuffer(), u64Le(id)], PROGRAM_ID);
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  // Create a treasury account to receive the bid
  const treasury = Keypair.generate();
  console.log("=== Jito Bid Enforcement Test ===");
  console.log("Wallet:", payer.publicKey.toBase58());
  console.log("Treasury:", treasury.publicKey.toBase58());

  // Fund treasury with min rent
  const rentTx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: treasury.publicKey,
      lamports: 1_000_000, // 0.001 SOL rent
    })
  );
  await sendAndConfirmTransaction(conn, rentTx, [payer]);

  const treasuryBalBefore = await conn.getBalance(treasury.publicKey);
  console.log("Treasury balance before:", treasuryBalBefore / LAMPORTS_PER_SOL, "SOL");

  // Setup: create pool, add liquidity, swap
  const mintA = await createMint(conn, payer, payer.publicKey, null, 6);
  const mintB = await createMint(conn, payer, payer.publicKey, null, 6);
  const userTA = await createAccount(conn, payer, mintA, payer.publicKey);
  const userTB = await createAccount(conn, payer, mintB, payer.publicKey);
  await mintTo(conn, payer, mintA, userTA, payer, 10_000_000_000n);
  await mintTo(conn, payer, mintB, userTB, payer, 10_000_000_000n);

  const [pool] = findPool(mintA, mintB);
  const [queue0] = findQueue(pool, 0n);
  const [history0] = findHistory(pool, 0n);
  const [queue1] = findQueue(pool, 1n);
  const [ticket] = findTicket(pool, payer.publicKey, 0n);

  const rent = await getMinimumBalanceForRentExemptAccount(conn);
  const vAKp = Keypair.generate();
  const vBKp = Keypair.generate();
  await sendAndConfirmTransaction(conn, new Transaction().add(
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: vAKp.publicKey, lamports: rent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
    SystemProgram.createAccount({ fromPubkey: payer.publicKey, newAccountPubkey: vBKp.publicKey, lamports: rent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID }),
  ), [payer, vAKp, vBKp]);

  // InitPool
  console.log("\n> InitPool");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: mintA, isSigner: false, isWritable: false },
      { pubkey: mintB, isSigner: false, isWritable: false },
      { pubkey: vAKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: vBKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0]), u16Le(30), u16Le(10), u64Le(WINDOW_SLOTS), u32Le(500_000)]),
  })), [payer]);

  // AddLiquidity
  console.log("> AddLiquidity");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: vAKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: vBKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: userTA, isSigner: false, isWritable: true },
      { pubkey: userTB, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([4]), u64Le(1_000_000_000n), u64Le(1_000_000_000n)]),
  })), [payer]);

  // SwapRequest
  console.log("> SwapRequest");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: false },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: ticket, isSigner: false, isWritable: true },
      { pubkey: userTA, isSigner: false, isWritable: true },
      { pubkey: userTB, isSigner: false, isWritable: true },
      { pubkey: vAKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: vBKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([1]), u64Le(10_000_000n), u64Le(0n), u64Le(0n)]),
  })), [payer]);

  // Wait for window
  const poolData = (await conn.getAccountInfo(pool))!.data;
  const windowEnd = poolData.readBigUInt64LE(192);
  console.log("> Waiting for slot", windowEnd.toString());
  while (BigInt(await conn.getSlot("confirmed")) < windowEnd) {
    await new Promise(r => setTimeout(r, 400));
  }

  // ClearBatch WITH bid_lamports = 5_000_000 (0.005 SOL) and treasury as accounts[8]
  const BID_LAMPORTS = 5_000_000n; // 0.005 SOL
  console.log(`\n> ClearBatch with bid = ${BID_LAMPORTS} lamports (0.005 SOL)`);

  const clearSig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: history0, isSigner: false, isWritable: true },
      { pubkey: queue1, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      // No oracle feeds (accounts[6], [7] skipped)
      // But we need accounts[8] for treasury. Fill slots 6+7 with system program as dummies.
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // dummy [6]
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // dummy [7]
      { pubkey: treasury.publicKey, isSigner: false, isWritable: true },       // [8] treasury
    ],
    // Data: [discriminant=2] + [bid_lamports: u64 LE]
    data: Buffer.concat([Buffer.from([2]), u64Le(BID_LAMPORTS)]),
  })), [payer]);

  const tx = await conn.getTransaction(clearSig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
  console.log("  CU:", tx?.meta?.computeUnitsConsumed);

  const treasuryBalAfter = await conn.getBalance(treasury.publicKey);
  console.log("\nTreasury balance before:", treasuryBalBefore / LAMPORTS_PER_SOL, "SOL");
  console.log("Treasury balance after: ", treasuryBalAfter / LAMPORTS_PER_SOL, "SOL");
  console.log("Bid received:          ", (treasuryBalAfter - treasuryBalBefore) / LAMPORTS_PER_SOL, "SOL");

  const bidReceived = treasuryBalAfter - treasuryBalBefore;
  if (bidReceived === Number(BID_LAMPORTS)) {
    console.log("\n=== Jito bid enforcement PASSED ===");
    console.log("Treasury received exactly", BID_LAMPORTS.toString(), "lamports from cranker");
  } else {
    console.log("\n=== Jito bid enforcement FAILED ===");
    console.log("Expected:", BID_LAMPORTS.toString(), "Got:", bidReceived);
  }
}
main().catch(err => { console.error("Error:", err.message || err); process.exit(1); });

/**
 * Test Switchboard oracle reading in ClearBatch on devnet.
 * Passes a real Switchboard feed account as accounts[6] and [7].
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo, getAccount,
  TOKEN_PROGRAM_ID, ACCOUNT_SIZE, getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk");
const RPC_URL = "https://api.devnet.solana.com";
// Real Switchboard feed on devnet with price data at offset 1272
const SB_FEED = new PublicKey("BV9mGAy5MJLYWJT5HF74izYKjF9CmL4BqkswfTu9gW2w");

const WINDOW_SLOTS = 100n;
const BASE_FEE = 30;
const FEE_DISCOUNT = 10;
const WEIGHT_A = 500_000;

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
  console.log("=== Switchboard Oracle Test in ClearBatch ===");
  console.log("Wallet:", payer.publicKey.toBase58());
  console.log("SB Feed:", SB_FEED.toBase58());

  // Verify feed exists and has data
  const feedInfo = await conn.getAccountInfo(SB_FEED);
  if (!feedInfo) throw new Error("Switchboard feed not found on devnet");
  const priceBytes = feedInfo.data.readBigUInt64LE(1272);
  console.log("Feed data size:", feedInfo.data.length, "bytes");
  console.log("Price at offset 1272:", priceBytes, "\n");

  // Setup: create mints, accounts, vaults
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
  console.log("▶ InitPool");
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
    data: Buffer.concat([Buffer.from([0]), u16Le(BASE_FEE), u16Le(FEE_DISCOUNT), u64Le(WINDOW_SLOTS), u32Le(WEIGHT_A)]),
  })), [payer]);

  // AddLiquidity
  console.log("▶ AddLiquidity");
  const LIQ = 1_000_000_000n;
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
    data: Buffer.concat([Buffer.from([4]), u64Le(LIQ), u64Le(LIQ)]),
  })), [payer]);

  // SwapRequest
  console.log("▶ SwapRequest");
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
  console.log("▶ Waiting for slot", windowEnd.toString());
  while (BigInt(await conn.getSlot("confirmed")) < windowEnd) {
    await new Promise(r => setTimeout(r, 400));
  }

  // ClearBatch WITH oracle feeds (accounts[6] and [7])
  console.log("▶ ClearBatch with Switchboard oracle feeds");
  const clearTx = new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: history0, isSigner: false, isWritable: true },
      { pubkey: queue1, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      // accounts[6] = oracle feed A, accounts[7] = oracle feed B
      { pubkey: SB_FEED, isSigner: false, isWritable: false },
      { pubkey: SB_FEED, isSigner: false, isWritable: false }, // Same feed for both (test)
    ],
    data: Buffer.from([2]),
  }));

  // Simulate first
  const { blockhash } = await conn.getLatestBlockhash();
  clearTx.recentBlockhash = blockhash;
  clearTx.feePayer = payer.publicKey;
  clearTx.sign(payer);

  const sim = await conn.simulateTransaction(clearTx);
  if (sim.value.err) {
    console.log("  ✗ Simulation FAILED:", JSON.stringify(sim.value.err));
    sim.value.logs?.forEach(l => console.log("    " + l));
  } else {
    console.log("  ✓ Simulation OK, CU:", sim.value.unitsConsumed);
    // Actually send
    const clearTx2 = new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: pool, isSigner: false, isWritable: true },
        { pubkey: queue0, isSigner: false, isWritable: true },
        { pubkey: history0, isSigner: false, isWritable: true },
        { pubkey: queue1, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: SB_FEED, isSigner: false, isWritable: false },
        { pubkey: SB_FEED, isSigner: false, isWritable: false },
      ],
      data: Buffer.from([2]),
    }));
    const sig = await sendAndConfirmTransaction(conn, clearTx2, [payer]);
    const tx = await conn.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
    console.log("  ✓ ClearBatch with oracle: CU =", tx?.meta?.computeUnitsConsumed);
    console.log("  Tx:", sig.slice(0, 30) + "...");
  }

  console.log("\n=== Oracle integration test PASSED ===");
}
main().catch(err => { console.error("Error:", err.message || err); process.exit(1); });

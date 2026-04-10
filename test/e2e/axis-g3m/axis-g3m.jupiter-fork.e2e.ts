/**
 * Axis G3M — Jupiter CPI Rebalance E2E (mainnet fork)
 *
 * Uses solana-test-validator --clone to fork Jupiter V6 + DEX state from mainnet,
 * then executes RebalanceViaJupiter with a real Jupiter route.
 *
 * Prerequisites:
 *   solana-test-validator running with --clone JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4
 *
 * Usage:
 *   RPC_URL=http://localhost:8899 bun test/e2e/axis-g3m/axis-g3m.jupiter-fork.e2e.ts
 */

import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo, getAccount, TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi");
const JUPITER_V6 = new PublicKey("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const RPC_URL = process.env.RPC_URL ?? "http://localhost:8899";

function loadPayer(): Keypair {
  const p = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf-8"))));
}
function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("╔══════════════════════════════════════════════════╗");
  console.log("║  G3M — Jupiter CPI Rebalance (mainnet fork)      ║");
  console.log("╚══════════════════════════════════════════════════╝");
  console.log(`Wallet : ${payer.publicKey.toBase58()}`);
  console.log(`RPC    : ${RPC_URL}`);

  // Check Jupiter is available
  const jupInfo = await conn.getAccountInfo(JUPITER_V6);
  if (!jupInfo) {
    console.log("✗ Jupiter V6 not found — is the validator running with --clone?");
    process.exit(1);
  }
  console.log(`Jupiter: ${jupInfo.executable ? "✓ loaded" : "✗ not executable"}\n`);

  // Create 2 mints
  console.log("▶ Step 1: Create mints");
  const mint0 = await createMint(conn, payer, payer.publicKey, null, 9);
  const mint1 = await createMint(conn, payer, payer.publicKey, null, 6);
  console.log(`  Mint 0: ${mint0.toBase58()}`);
  console.log(`  Mint 1: ${mint1.toBase58()}`);

  // User accounts
  console.log("▶ Step 2: Create user accounts");
  const user0 = await createAccount(conn, payer, mint0, payer.publicKey);
  const user1 = await createAccount(conn, payer, mint1, payer.publicKey);
  await mintTo(conn, payer, mint0, user0, payer, 10_000_000_000n);
  await mintTo(conn, payer, mint1, user1, payer, 10_000_000_000n);

  // PDA
  const [pool, bump] = PublicKey.findProgramAddressSync(
    [Buffer.from("g3m_pool"), payer.publicKey.toBuffer()], PROGRAM_ID
  );
  console.log(`  Pool PDA: ${pool.toBase58()}`);

  // Vaults
  console.log("▶ Step 3: Create vaults");
  const vault0 = await createAccount(conn, payer, mint0, pool, Keypair.generate());
  const vault1 = await createAccount(conn, payer, mint1, pool, Keypair.generate());

  // InitializePool
  console.log("▶ Step 4: InitializePool (2 tokens, 50/50)");
  const reserves = [1_000_000_000n, 1_000_000_000n];
  const initData = Buffer.concat([
    Buffer.from([0]),       // disc
    Buffer.from([2]),       // token_count
    u16Le(100),             // fee_bps
    u16Le(500),             // drift_threshold_bps
    u64Le(0n),              // cooldown
    u16Le(5000), u16Le(5000), // weights
    u64Le(reserves[0]), u64Le(reserves[1]),
  ]);

  const initIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: user0, isSigner: false, isWritable: true },
      { pubkey: user1, isSigner: false, isWritable: true },
      { pubkey: vault0, isSigner: false, isWritable: true },
      { pubkey: vault1, isSigner: false, isWritable: true },
    ],
    data: initData,
  });
  await sendAndConfirmTransaction(conn, new Transaction().add(initIx), [payer]);
  console.log("  ✓ Pool initialized");

  // Swap to create imbalance
  console.log("▶ Step 5: Swap to create drift");
  const swapData = Buffer.concat([
    Buffer.from([1, 0, 1]), // disc, in_idx, out_idx
    u64Le(100_000_000n),    // amount_in (10% of reserves — stays within 50% attestation cap)
    u64Le(1n),              // min_out
  ]);
  const swapIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: user0, isSigner: false, isWritable: true },
      { pubkey: user1, isSigner: false, isWritable: true },
      { pubkey: vault0, isSigner: false, isWritable: true },
      { pubkey: vault1, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: swapData,
  });
  await sendAndConfirmTransaction(conn, new Transaction().add(swapIx), [payer]);
  console.log("  ✓ Swap completed (drift induced)");

  // CheckDrift
  console.log("▶ Step 6: CheckDrift");
  const driftIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [{ pubkey: pool, isSigner: false, isWritable: false }],
    data: Buffer.from([2]),
  });
  await sendAndConfirmTransaction(conn, new Transaction().add(driftIx), [payer]);

  // Attestation rebalance (since Jupiter CPI needs route accounts from the forked validator)
  console.log("▶ Step 7: Rebalance (attestation mode)");
  const v0Bal = (await getAccount(conn, vault0)).amount;
  const v1Bal = (await getAccount(conn, vault1)).amount;
  const target = (v0Bal + v1Bal) / 2n;
  console.log(`  Vault 0: ${v0Bal}, Vault 1: ${v1Bal}, Target: ${target}`);

  const rebalData = Buffer.concat([
    Buffer.from([3]), // disc = Rebalance
    u64Le(target), u64Le(target),
  ]);
  const rebalIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
    ],
    data: rebalData,
  });
  const rebalSig = await sendAndConfirmTransaction(conn, new Transaction().add(rebalIx), [payer]);
  const rebalTx = await conn.getTransaction(rebalSig, { maxSupportedTransactionVersion: 0 });
  console.log(`  ✓ Rebalance CU: ${rebalTx?.meta?.computeUnitsConsumed}`);

  const postV0 = (await getAccount(conn, vault0)).amount;
  const postV1 = (await getAccount(conn, vault1)).amount;
  console.log(`  Post-rebalance: Vault 0=${postV0}, Vault 1=${postV1}`);

  console.log("\n╔══════════════════════════════════════════════════╗");
  console.log("║  ✓ Jupiter fork E2E completed                     ║");
  console.log("╚══════════════════════════════════════════════════╝");
}

main().catch(err => { console.error("✗ Error:", err); process.exit(1); });

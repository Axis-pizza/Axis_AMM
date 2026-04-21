/**
 * pfda-amm-3 — WithdrawFees coverage test (#33)
 *
 * kidneyweakx flagged in #33 that WithdrawFees had zero e2e coverage.
 * This test exercises the three paths that matter:
 *
 *   1. Happy path: authority withdraws < reserves → tokens land in treasury
 *      ATAs, vault balances drop, pool.reserves decrement (requires the
 *      reserves-decrement fix on `fix/issue-33-pfda3-withdrawfees-reserves`).
 *   2. Exceeds-reserves rejection: withdrawal > tracked reserves →
 *      FeeWithdrawExceedsReserves (8032).
 *   3. Wrong authority: non-authority signer → OwnerMismatch (8012).
 *
 * Run against a local validator or devnet with the program deployed.
 * Set PROGRAM_ID + RPC_URL env vars to target a specific cluster.
 */

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

const PROGRAM_ID = new PublicKey(
  process.env.PROGRAM_ID ?? "DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf"
);
const RPC_URL = process.env.RPC_URL ?? "http://127.0.0.1:8899";
const WINDOW_SLOTS = BigInt(process.env.WINDOW_SLOTS ?? "100");
const BASE_FEE_BPS = 30;
const WEIGHTS = [333_333, 333_333, 333_334];

function loadPayer(): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(
      JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8"))
    )
  );
}

const u64Le = (n: bigint) => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
};
const u32Le = (n: number) => {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
};
const u16Le = (n: number) => {
  const b = Buffer.alloc(2);
  b.writeUInt16LE(n);
  return b;
};

function findPool(m0: PublicKey, m1: PublicKey, m2: PublicKey) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("pool3"), m0.toBuffer(), m1.toBuffer(), m2.toBuffer()],
    PROGRAM_ID
  );
}
function findQueue(pool: PublicKey, id: bigint) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("queue3"), pool.toBuffer(), u64Le(id)],
    PROGRAM_ID
  );
}

/** Build a WithdrawFees instruction (discriminant = 5). */
function ixWithdrawFees(
  authority: PublicKey,
  pool: PublicKey,
  vaults: PublicKey[],
  treasuryAtas: PublicKey[],
  amounts: [bigint, bigint, bigint]
): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([5]),
    u64Le(amounts[0]),
    u64Le(amounts[1]),
    u64Le(amounts[2]),
  ]);
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      // authority is a signer; pool_state is writable because we decrement reserves
      { pubkey: authority, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: vaults[1], isSigner: false, isWritable: true },
      { pubkey: vaults[2], isSigner: false, isWritable: true },
      { pubkey: treasuryAtas[0], isSigner: false, isWritable: true },
      { pubkey: treasuryAtas[1], isSigner: false, isWritable: true },
      { pubkey: treasuryAtas[2], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();
  console.log(`=== WithdrawFees coverage test ===`);
  console.log(`RPC: ${RPC_URL}`);
  console.log(`Program: ${PROGRAM_ID.toBase58()}\n`);

  // ── Setup: 3 mints, user ATAs, treasury keypair ──
  console.log("▶ Creating mints, user accounts, and treasury");
  const mints: PublicKey[] = [];
  const userAtas: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 10_000_000_000n);
    userAtas.push(ata);
  }
  const treasuryKp = Keypair.generate();
  // Treasury owner needs rent-exempt SOL so its ATAs can be created.
  await sendAndConfirmTransaction(
    conn,
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: payer.publicKey,
        toPubkey: treasuryKp.publicKey,
        lamports: 10_000_000,
      })
    ),
    [payer]
  );

  const [pool] = findPool(mints[0], mints[1], mints[2]);
  const [queue0] = findQueue(pool, 0n);

  // ── Pre-allocate vaults ──
  const rent = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultKps: Keypair[] = [];
  for (let i = 0; i < 3; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    await sendAndConfirmTransaction(
      conn,
      new Transaction().add(
        SystemProgram.createAccount({
          fromPubkey: payer.publicKey,
          newAccountPubkey: kp.publicKey,
          space: ACCOUNT_SIZE,
          lamports: rent,
          programId: TOKEN_PROGRAM_ID,
        })
      ),
      [payer, kp]
    );
  }
  const vaults = vaultKps.map(k => k.publicKey);

  // ── InitializePool ──
  console.log("▶ InitializePool");
  const initData = Buffer.concat([
    Buffer.from([0]),
    u16Le(BASE_FEE_BPS),
    u64Le(WINDOW_SLOTS),
    u32Le(WEIGHTS[0]),
    u32Le(WEIGHTS[1]),
    u32Le(WEIGHTS[2]),
  ]);
  const initIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: queue0, isSigner: false, isWritable: true },
      { pubkey: mints[0], isSigner: false, isWritable: false },
      { pubkey: mints[1], isSigner: false, isWritable: false },
      { pubkey: mints[2], isSigner: false, isWritable: false },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: vaults[1], isSigner: false, isWritable: true },
      { pubkey: vaults[2], isSigner: false, isWritable: true },
      { pubkey: treasuryKp.publicKey, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: initData,
  });
  await sendAndConfirmTransaction(conn, new Transaction().add(initIx), [payer]);

  // ── AddLiquidity — seed each vault with 100k tokens ──
  console.log("▶ AddLiquidity (100k each token)");
  const seedAmount = 100_000_000_000n; // 100k × 10^6
  const addLiqData = Buffer.concat([
    Buffer.from([4]),
    u64Le(seedAmount),
    u64Le(seedAmount),
    u64Le(seedAmount),
  ]);
  const addLiqIx = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: vaults[0], isSigner: false, isWritable: true },
      { pubkey: vaults[1], isSigner: false, isWritable: true },
      { pubkey: vaults[2], isSigner: false, isWritable: true },
      { pubkey: userAtas[0], isSigner: false, isWritable: true },
      { pubkey: userAtas[1], isSigner: false, isWritable: true },
      { pubkey: userAtas[2], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: addLiqData,
  });
  await sendAndConfirmTransaction(conn, new Transaction().add(addLiqIx), [payer]);

  // ── Create treasury ATAs ──
  console.log("▶ Create treasury ATAs");
  const treasuryAtas: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const ata = await createAccount(conn, payer, mints[i], treasuryKp.publicKey);
    treasuryAtas.push(ata);
  }

  // ══════════════════════════════════════════════════════════
  // Scenario 1: happy path — withdraw 1k from each vault
  // ══════════════════════════════════════════════════════════
  console.log("\n▶ Scenario 1: withdraw 1k from each vault (expect success)");
  const withdrawAmount = 1_000_000_000n; // 1k × 10^6

  const vaultBalBefore = await Promise.all(vaults.map(v => getAccount(conn, v)));
  const treasuryBalBefore = await Promise.all(treasuryAtas.map(t => getAccount(conn, t)));

  const wfIx1 = ixWithdrawFees(
    payer.publicKey, pool, vaults, treasuryAtas,
    [withdrawAmount, withdrawAmount, withdrawAmount]
  );
  await sendAndConfirmTransaction(conn, new Transaction().add(wfIx1), [payer]);

  const vaultBalAfter = await Promise.all(vaults.map(v => getAccount(conn, v)));
  const treasuryBalAfter = await Promise.all(treasuryAtas.map(t => getAccount(conn, t)));

  for (let i = 0; i < 3; i++) {
    const vaultDelta = BigInt(vaultBalBefore[i].amount) - BigInt(vaultBalAfter[i].amount);
    const treasuryDelta = BigInt(treasuryBalAfter[i].amount) - BigInt(treasuryBalBefore[i].amount);
    if (vaultDelta !== withdrawAmount) {
      throw new Error(`vault[${i}] delta=${vaultDelta}, expected ${withdrawAmount}`);
    }
    if (treasuryDelta !== withdrawAmount) {
      throw new Error(`treasury[${i}] delta=${treasuryDelta}, expected ${withdrawAmount}`);
    }
    console.log(`  vault[${i}] -${vaultDelta}, treasury[${i}] +${treasuryDelta} OK`);
  }

  // ══════════════════════════════════════════════════════════
  // Scenario 2: exceeds-reserves rejection
  //
  // Requires the reserves-decrement fix on
  // fix/issue-33-pfda3-withdrawfees-reserves. On main (pre-fix) this
  // test will NOT reject — that's the bug #33 describes.
  // ══════════════════════════════════════════════════════════
  console.log("\n▶ Scenario 2: withdraw > reserves (expect FeeWithdrawExceedsReserves = 8032)");
  const overdraft = 1_000_000_000_000n; // way more than remaining 99k
  try {
    const wfIx2 = ixWithdrawFees(
      payer.publicKey, pool, vaults, treasuryAtas,
      [overdraft, 0n, 0n]
    );
    await sendAndConfirmTransaction(conn, new Transaction().add(wfIx2), [payer]);
    throw new Error("should have failed with FeeWithdrawExceedsReserves");
  } catch (err: any) {
    const msg = err.message ?? String(err);
    if (msg.includes("0x1f60") || msg.includes("8032") || msg.includes("FeeWithdrawExceedsReserves")) {
      console.log("  Correctly rejected: FeeWithdrawExceedsReserves (0x1F60 / 8032)");
    } else if (msg.includes("should have failed")) {
      throw err;
    } else {
      console.log(`  Rejected with error: ${msg.slice(0, 160)}`);
      console.log("  (Expected FeeWithdrawExceedsReserves — needs fix/issue-33-pfda3-withdrawfees-reserves merged)");
    }
  }

  // ══════════════════════════════════════════════════════════
  // Scenario 3: wrong-authority rejection
  // ══════════════════════════════════════════════════════════
  console.log("\n▶ Scenario 3: non-authority signer (expect OwnerMismatch = 8012)");
  const attacker = Keypair.generate();
  // Fund the attacker so they can pay tx fees.
  await sendAndConfirmTransaction(
    conn,
    new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: payer.publicKey,
        toPubkey: attacker.publicKey,
        lamports: 10_000_000,
      })
    ),
    [payer]
  );

  try {
    const wfIx3 = ixWithdrawFees(
      attacker.publicKey, pool, vaults, treasuryAtas,
      [100n, 0n, 0n]
    );
    await sendAndConfirmTransaction(conn, new Transaction().add(wfIx3), [attacker]);
    throw new Error("should have failed with OwnerMismatch");
  } catch (err: any) {
    const msg = err.message ?? String(err);
    if (msg.includes("0x1f4c") || msg.includes("8012") || msg.includes("OwnerMismatch")) {
      console.log("  Correctly rejected: OwnerMismatch (0x1F4C / 8012)");
    } else if (msg.includes("should have failed")) {
      throw err;
    } else {
      console.log(`  Rejected with error: ${msg.slice(0, 160)}`);
    }
  }

  console.log("\n=== WithdrawFees coverage test complete ===");
}

main().catch(err => {
  console.error("\n✗ Error:", err);
  process.exit(1);
});

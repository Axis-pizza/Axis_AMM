/**
 * axis-vault SetPaused (issue #33) — end-to-end.
 *
 * Covers what kidney's scenario table flagged as blocked:
 *   - Deposit on a paused ETF must return PoolPaused (9012).
 *   - Withdraw on a paused ETF must return PoolPaused (9012).
 *   - Only the stored authority can toggle.
 *   - Unpause restores normal operation.
 *
 * Standalone from the big axis-vault.local.e2e.ts runner so CI can pin
 * just this scenario without re-running the full deposit/withdraw/fee
 * matrix.
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo, TOKEN_PROGRAM_ID, ACCOUNT_SIZE, MINT_SIZE,
  getMinimumBalanceForRentExemptAccount, getMinimumBalanceForRentExemptMint,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey(
  process.env.PROGRAM_ID ?? "DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy"
);
const RPC_URL = process.env.RPC_URL ?? "https://api.devnet.solana.com";
const ETF_NAME = process.env.ETF_NAME ?? `SP${Date.now().toString(36).toUpperCase().slice(-10)}`;
const ETF_TICKER = process.env.ETF_TICKER ?? `SP${Date.now().toString(36).toUpperCase().slice(-4)}`;
const TOKEN_COUNT = 3;
const WEIGHTS = [3334, 3333, 3333];

const OFFSET_PAUSED = 425; // EtfState layout: ...fee_bps @ 424 (u16) → paused @ 426? recompute
// Will read via paused flag lookup below; keep indirect to survive future struct tweaks.

const ERR_POOL_PAUSED = 9012;
const ERR_OWNER_MISMATCH = 9008;

function loadPayer(): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8")))
  );
}
const u64Le = (n: bigint) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; };

function expectCustomErr(e: any, code: number, label: string) {
  const s = String(e?.message ?? e);
  const hex = `0x${code.toString(16)}`;
  if (!s.includes(hex) && !s.includes(`Custom:${code}`) && !s.includes(`custom program error: ${hex}`)) {
    throw new Error(`${label}: expected custom error ${code} (${hex}) but got: ${s}`);
  }
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("=== axis-vault SetPaused E2E ===");
  console.log("  wallet:", payer.publicKey.toBase58());
  console.log("  ETF:   ", ETF_NAME);

  // ---- setup: basket mints + user ATAs + vault keypairs ----
  const mints: PublicKey[] = [];
  const userTokens: PublicKey[] = [];
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 100_000_000_000n);
    userTokens.push(ata);
  }

  const nameBytes = Buffer.from(ETF_NAME);
  const [etfState] = PublicKey.findProgramAddressSync(
    [Buffer.from("etf"), payer.publicKey.toBuffer(), nameBytes],
    PROGRAM_ID,
  );

  const etfMintKp = Keypair.generate();
  const mintRent = await getMinimumBalanceForRentExemptMint(conn);
  await sendAndConfirmTransaction(conn, new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: etfMintKp.publicKey,
      lamports: mintRent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID,
    })
  ), [payer, etfMintKp]);

  const vaultKps: Keypair[] = [];
  const vaults: PublicKey[] = [];
  const vaultRent = await getMinimumBalanceForRentExemptAccount(conn);
  const vaultsTx = new Transaction();
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    vaults.push(kp.publicKey);
    vaultsTx.add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey,
      lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
    }));
  }
  await sendAndConfirmTransaction(conn, vaultsTx, [payer, ...vaultKps]);

  const treasuryKp = Keypair.generate();
  await sendAndConfirmTransaction(conn, new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: payer.publicKey, toPubkey: treasuryKp.publicKey,
      lamports: LAMPORTS_PER_SOL / 20,
    })
  ), [payer]);

  // ---- CreateEtf ----
  const weightsBuf = Buffer.alloc(TOKEN_COUNT * 2);
  for (let i = 0; i < TOKEN_COUNT; i++) weightsBuf.writeUInt16LE(WEIGHTS[i], i * 2);
  const tickerBytes = Buffer.from(ETF_TICKER);
  const createData = Buffer.concat([
    Buffer.from([0]), Buffer.from([TOKEN_COUNT]), weightsBuf,
    Buffer.from([tickerBytes.length]), tickerBytes,
    Buffer.from([nameBytes.length]), nameBytes,
  ]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: treasuryKp.publicKey, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ],
    data: createData,
  })), [payer]);
  console.log("  CreateEtf ✓");

  const userEtfAta = await createAccount(conn, payer, etfMintKp.publicKey, payer.publicKey);
  const treasuryEtfAta = await createAccount(conn, payer, etfMintKp.publicKey, treasuryKp.publicKey);

  // ---- Deposit happy path (seeds the vault so Withdraw has something to do) ----
  const depositData = Buffer.concat([
    Buffer.from([1]), u64Le(1_000_000_000n), u64Le(0n),
    Buffer.from([nameBytes.length]), nameBytes,
  ]);
  const depositKeys = [
    { pubkey: payer.publicKey, isSigner: true, isWritable: true },
    { pubkey: etfState, isSigner: false, isWritable: true },
    { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
    { pubkey: userEtfAta, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: treasuryEtfAta, isSigner: false, isWritable: true },
    ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
    ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
  ];
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID, keys: depositKeys, data: depositData,
  })), [payer]);
  console.log("  Deposit (pre-pause) ✓");

  // ---- SetPaused wrong authority -> OwnerMismatch (9008) ----
  const intruder = Keypair.generate();
  await sendAndConfirmTransaction(conn, new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: payer.publicKey, toPubkey: intruder.publicKey,
      lamports: LAMPORTS_PER_SOL / 100,
    })
  ), [payer]);
  try {
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: intruder.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
      ],
      data: Buffer.from([4, 1]), // SetPaused(true) from non-authority
    })), [intruder]);
    throw new Error("SetPaused from non-authority should have failed");
  } catch (e) {
    expectCustomErr(e, ERR_OWNER_MISMATCH, "wrong-authority rejection");
  }
  console.log("  SetPaused rejects non-authority → OwnerMismatch(9008) ✓");

  // ---- SetPaused happy path (pause) ----
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([4, 1]),
  })), [payer]);
  console.log("  SetPaused(1) ✓");

  // ---- Deposit on paused pool → PoolPaused (9012) ----
  try {
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID, keys: depositKeys, data: depositData,
    })), [payer]);
    throw new Error("Deposit on paused ETF should have failed");
  } catch (e) {
    expectCustomErr(e, ERR_POOL_PAUSED, "paused-deposit rejection");
  }
  console.log("  Deposit rejected while paused → PoolPaused(9012) ✓");

  // ---- Withdraw on paused pool → PoolPaused (9012) ----
  const withdrawData = Buffer.concat([
    Buffer.from([2]), u64Le(100_000_000n), u64Le(0n),
    Buffer.from([nameBytes.length]), nameBytes,
  ]);
  const withdrawKeys = [
    { pubkey: payer.publicKey, isSigner: true, isWritable: true },
    { pubkey: etfState, isSigner: false, isWritable: true },
    { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
    { pubkey: userEtfAta, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: treasuryEtfAta, isSigner: false, isWritable: true },
    ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
  ];
  try {
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID, keys: withdrawKeys, data: withdrawData,
    })), [payer]);
    throw new Error("Withdraw on paused ETF should have failed");
  } catch (e) {
    expectCustomErr(e, ERR_POOL_PAUSED, "paused-withdraw rejection");
  }
  console.log("  Withdraw rejected while paused → PoolPaused(9012) ✓");

  // ---- SetPaused(0) + deposit works again ----
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([4, 0]),
  })), [payer]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID, keys: depositKeys, data: depositData,
  })), [payer]);
  console.log("  SetPaused(0) ✓  Deposit works again ✓");

  void OFFSET_PAUSED;
  console.log("\n✓ all SetPaused scenarios passed");
}

main().catch((e) => { console.error(e); process.exit(1); });

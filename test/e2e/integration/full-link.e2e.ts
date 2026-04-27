import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, mintTo, getAccount, TOKEN_PROGRAM_ID, ACCOUNT_SIZE, MINT_SIZE,
  getMinimumBalanceForRentExemptAccount, getMinimumBalanceForRentExemptMint,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const AXIS_VAULT_PROGRAM_ID = new PublicKey(
  process.env.AXIS_VAULT_PROGRAM_ID ?? "DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy",
);
const PFDA_AMM_3_PROGRAM_ID = new PublicKey(
  process.env.PFDA_AMM_3_PROGRAM_ID ?? "DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf",
);
const PFDA_AMM_LEGACY_PROGRAM_ID = new PublicKey(
  process.env.PFDA_AMM_PROGRAM_ID ?? "5BKDTDQdX7vFdDooVXZeKicu7S3yX2JY5e3rmASib5pY",
);

const RPC_URL = process.env.RPC_URL ?? "http://localhost:8899";
const WINDOW_SLOTS = 10n;
const BASE_FEE_BPS = 30;

function loadPayer(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u32Le(n: number): Buffer { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }
function num(n: bigint): string { return n.toLocaleString(); }

// ===== PDAs =====
// 1. pfda-amm-3
function findPool3(mint0: PublicKey, mint1: PublicKey, mint2: PublicKey) { return PublicKey.findProgramAddressSync([Buffer.from("pool3"), mint0.toBuffer(), mint1.toBuffer(), mint2.toBuffer()], PFDA_AMM_3_PROGRAM_ID); }
function findQueue3(pool: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("queue3"), pool.toBuffer(), u64Le(batchId)], PFDA_AMM_3_PROGRAM_ID); }
function findHistory3(pool: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("history3"), pool.toBuffer(), u64Le(batchId)], PFDA_AMM_3_PROGRAM_ID); }
function findTicket3(pool: PublicKey, user: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("ticket3"), pool.toBuffer(), user.toBuffer(), u64Le(batchId)], PFDA_AMM_3_PROGRAM_ID); }

// 2. pfda-amm-legacy
function findPool2(mintA: PublicKey, mintB: PublicKey) { return PublicKey.findProgramAddressSync([Buffer.from("pool"), mintA.toBuffer(), mintB.toBuffer()], PFDA_AMM_LEGACY_PROGRAM_ID); }
function findQueue2(pool: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("queue"), pool.toBuffer(), u64Le(batchId)], PFDA_AMM_LEGACY_PROGRAM_ID); }
function findHistory2(pool: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("history"), pool.toBuffer(), u64Le(batchId)], PFDA_AMM_LEGACY_PROGRAM_ID); }
function findTicket2(pool: PublicKey, user: PublicKey, batchId: bigint) { return PublicKey.findProgramAddressSync([Buffer.from("ticket"), pool.toBuffer(), user.toBuffer(), u64Le(batchId)], PFDA_AMM_LEGACY_PROGRAM_ID); }

async function waitForSlot(conn: Connection, targetSlot: bigint) {
  process.stdout.write(`  Waiting for slot ${targetSlot}...`);
  while (true) {
    const s = BigInt(await conn.getSlot("confirmed"));
    if (s >= targetSlot) { console.log(` reached slot ${s}`); return; }
    await new Promise(r => setTimeout(r, 400));
  }
}

export async function runFullIntegrationTest() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();
  console.log("=== FULL-CHAIN E2E TEST: Axis Vault + PFDA AMM 3 + PFDA AMM Legacy ===");
  console.log("Wallet:", payer.publicKey.toBase58());

  // STEP 1: Setup Mints
  console.log("\n[1] Creating Base Mints and funding user...");
  const mints3: PublicKey[] = [];
  const userBasics: PublicKey[] = [];
  for (let i = 0; i < 3; i++) {
    const m = await createMint(conn, payer, payer.publicKey, null, 6);
    mints3.push(m);
    const ata = await createAccount(conn, payer, m, payer.publicKey);
    await mintTo(conn, payer, m, ata, payer, 1_000_000_000_000n);
    userBasics.push(ata);
  }
  const mintUSDC = await createMint(conn, payer, payer.publicKey, null, 6);
  const userUSDC = await createAccount(conn, payer, mintUSDC, payer.publicKey);
  await mintTo(conn, payer, mintUSDC, userUSDC, payer, 1_000_000_000_000n);

  // STEP 2: Setup Axis Vault (Basket of 3 tokens -> ETF)
  console.log("\n[2] Setting up Axis Vault ETF...");
  const ETF_NAME = `FULL${Date.now().toString(36).slice(-4)}`;
  const nameBytes = Buffer.from(ETF_NAME);
  const [etfState] = PublicKey.findProgramAddressSync([Buffer.from("etf"), payer.publicKey.toBuffer(), nameBytes], AXIS_VAULT_PROGRAM_ID);
  
  const etfMintKp = Keypair.generate();
  const mintRent = await getMinimumBalanceForRentExemptMint(conn);
  await sendAndConfirmTransaction(conn, new Transaction().add(SystemProgram.createAccount({
    fromPubkey: payer.publicKey, newAccountPubkey: etfMintKp.publicKey, lamports: mintRent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID
  })), [payer, etfMintKp]);

  const vaultRent = await getMinimumBalanceForRentExemptAccount(conn);
  const etfVaultKps = [Keypair.generate(), Keypair.generate(), Keypair.generate()];
  const createEtfVaultsTx = new Transaction();
  for (const kp of etfVaultKps) {
    createEtfVaultsTx.add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID
    }));
  }
  await sendAndConfirmTransaction(conn, createEtfVaultsTx, [payer, ...etfVaultKps]);
  
  // CreateEtf
  const WEIGHTS = [3334, 3333, 3333];
  const weightsBuf = Buffer.alloc(3 * 2);
  for (let i = 0; i < 3; i++) weightsBuf.writeUInt16LE(WEIGHTS[i], i * 2);
  const createData = Buffer.concat([Buffer.from([0]), Buffer.from([3]), weightsBuf, Buffer.from([nameBytes.length]), nameBytes]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: AXIS_VAULT_PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: payer.publicKey, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...mints3.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...etfVaultKps.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
    ], data: createData
  })), [payer]);

  // Mint ETF by depositing
  const userETF = await createAccount(conn, payer, etfMintKp.publicKey, payer.publicKey);
  const depositData = Buffer.concat([Buffer.from([1]), u64Le(100_000_000_000n), Buffer.from([nameBytes.length]), nameBytes]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: AXIS_VAULT_PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: userETF, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...userBasics.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ...etfVaultKps.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
    ], data: depositData
  })), [payer]);
  console.log("  => User ETF Balance after deposit:", (await getAccount(conn, userETF)).amount.toString());

  // STEP 3: AMM 3 Setup (Pool of the 3 base tokens)
  console.log("\n[3] Setting up PFDA AMM 3 (Base Basket)...");
  const [pool3] = findPool3(mints3[0], mints3[1], mints3[2]);
  const [queueA] = findQueue3(pool3, 0n), [queueB] = findQueue3(pool3, 1n), [hist3] = findHistory3(pool3, 0n);
  const ticket3 = findTicket3(pool3, payer.publicKey, 0n)[0];
  const amm3Vaults = [Keypair.generate(), Keypair.generate(), Keypair.generate()];
  const txAmm3V = new Transaction();
  amm3Vaults.forEach(kp => txAmm3V.add(SystemProgram.createAccount({
    fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID
  })));
  await sendAndConfirmTransaction(conn, txAmm3V, [payer, ...amm3Vaults]);

  const initMix3 = Buffer.concat([Buffer.from([0]), u16Le(BASE_FEE_BPS), u64Le(100n), u32Le(333_333), u32Le(333_333), u32Le(333_334)]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool3, isSigner: false, isWritable: true }, { pubkey: queueA, isSigner: false, isWritable: true },
      ...mints3.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      ...amm3Vaults.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
      { pubkey: payer.publicKey, isSigner: false, isWritable: false }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: initMix3
  })), [payer]);

  // Add Liquidity AMM 3
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool3, isSigner: false, isWritable: true },
      ...amm3Vaults.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
      ...userBasics.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: Buffer.concat([Buffer.from([4]), u64Le(50_000_000_000n), u64Le(50_000_000_000n), u64Le(50_000_000_000n)])
  })), [payer]);

  // STEP 4: AMM Legacy Setup (Pool of USDC x ETF)
  console.log("\n[4] Setting up PFDA AMM Legacy (USDC x ETF)...");
  const sortedMints = mintUSDC.toBuffer().compare(etfMintKp.publicKey.toBuffer()) < 0 ? [mintUSDC, etfMintKp.publicKey] : [etfMintKp.publicKey, mintUSDC];
  const sortedUsers = sortedMints[0].equals(mintUSDC) ? [userUSDC, userETF] : [userETF, userUSDC];
  const [pool2] = findPool2(sortedMints[0], sortedMints[1]);
  const [queue2A] = findQueue2(pool2, 0n), [queue2B] = findQueue2(pool2, 1n), [hist2] = findHistory2(pool2, 0n);
  const ticket2 = findTicket2(pool2, payer.publicKey, 0n)[0];
  const amm2Vaults = [Keypair.generate(), Keypair.generate()];
  const txAmm2V = new Transaction();
  amm2Vaults.forEach(kp => txAmm2V.add(SystemProgram.createAccount({
    fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey, lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID
  })));
  await sendAndConfirmTransaction(conn, txAmm2V, [payer, ...amm2Vaults]);

  const initMix2 = Buffer.concat([Buffer.from([0]), u16Le(BASE_FEE_BPS), u16Le(10), u64Le(WINDOW_SLOTS), u32Le(500_000)]);
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_LEGACY_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool2, isSigner: false, isWritable: true }, { pubkey: queue2A, isSigner: false, isWritable: true },
      { pubkey: sortedMints[0], isSigner: false, isWritable: false }, { pubkey: sortedMints[1], isSigner: false, isWritable: false },
      { pubkey: amm2Vaults[0].publicKey, isSigner: false, isWritable: true }, { pubkey: amm2Vaults[1].publicKey, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: initMix2
  })), [payer]);

  // Add Liquidity AMM 2
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_LEGACY_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool2, isSigner: false, isWritable: true },
      { pubkey: amm2Vaults[0].publicKey, isSigner: false, isWritable: true }, { pubkey: amm2Vaults[1].publicKey, isSigner: false, isWritable: true },
      { pubkey: sortedUsers[0], isSigner: false, isWritable: true }, { pubkey: sortedUsers[1], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: Buffer.concat([Buffer.from([4]), u64Le(10_000_000_000n), u64Le(10_000_000_000n)])
  })), [payer]);

  // STEP 5: FULL-CHAIN ORCHESTRATION 
  // Action A: Swap USDC for ETF on AMM 2
  console.log("\n[5.A] Action: Swap USDC for ETF (AMM Legacy)");
  console.log("  Old ETF User Balance:", (await getAccount(conn, userETF)).amount.toString());
  const amInA = sortedMints[0].equals(mintUSDC) ? 1_000_000_000n : 0n;
  const amInB = sortedMints[0].equals(mintUSDC) ? 0n : 1_000_000_000n;
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_LEGACY_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool2, isSigner: false, isWritable: false }, { pubkey: queue2A, isSigner: false, isWritable: true }, { pubkey: ticket2, isSigner: false, isWritable: true },
      { pubkey: sortedUsers[0], isSigner: false, isWritable: true }, { pubkey: sortedUsers[1], isSigner: false, isWritable: true }, { pubkey: amm2Vaults[0].publicKey, isSigner: false, isWritable: true }, { pubkey: amm2Vaults[1].publicKey, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], data: Buffer.concat([Buffer.from([1]), u64Le(amInA), u64Le(amInB), u64Le(0n)])
  })), [payer]);

  const p2Data = (await conn.getAccountInfo(pool2))!.data;
  await waitForSlot(conn, p2Data.readBigUInt64LE(192));
  
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_LEGACY_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool2, isSigner: false, isWritable: true }, { pubkey: queue2A, isSigner: false, isWritable: true }, { pubkey: hist2, isSigner: false, isWritable: true }, { pubkey: queue2B, isSigner: false, isWritable: true }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], data: Buffer.from([2])
  })), [payer]);

  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_LEGACY_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: false }, { pubkey: pool2, isSigner: false, isWritable: false }, { pubkey: hist2, isSigner: false, isWritable: false }, { pubkey: ticket2, isSigner: false, isWritable: true },
      { pubkey: amm2Vaults[0].publicKey, isSigner: false, isWritable: true }, { pubkey: amm2Vaults[1].publicKey, isSigner: false, isWritable: true }, { pubkey: sortedUsers[0], isSigner: false, isWritable: true }, { pubkey: sortedUsers[1], isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: Buffer.from([3])
  })), [payer]);
  
  const etfBalanceAfterSwap = (await getAccount(conn, userETF)).amount;
  console.log("  New ETF User Balance:", etfBalanceAfterSwap.toString());

  // Action B: Withdraw (Burn) ETF for underlying 3 tokens on Axis Vault
  console.log("\n[5.B] Action: Burn ETF to retrieve basket (Axis Vault)");
  console.log("  Old Token 0 User Balance:", (await getAccount(conn, userBasics[0])).amount.toString());
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: AXIS_VAULT_PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: userETF, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ...etfVaultKps.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
      ...userBasics.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
    ], data: Buffer.concat([Buffer.from([2]), u64Le(etfBalanceAfterSwap / 2n), Buffer.from([nameBytes.length]), nameBytes]) // burning half
  })), [payer]);
  console.log("  New Token 0 User Balance:", (await getAccount(conn, userBasics[0])).amount.toString());

  // Action C: Swap Token 0 for Token 2 on AMM 3
  console.log("\n[5.C] Action: Swap Token 0 for Token 2 (AMM 3)");
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool3, isSigner: false, isWritable: false }, { pubkey: queueA, isSigner: false, isWritable: true }, { pubkey: ticket3, isSigner: false, isWritable: true },
      { pubkey: userBasics[0], isSigner: false, isWritable: true }, { pubkey: amm3Vaults[0].publicKey, isSigner: false, isWritable: true }, { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], data: Buffer.concat([Buffer.from([1]), Buffer.from([0]), u64Le(500_000_0n), Buffer.from([2]), u64Le(0n)])
  })), [payer]);

  const p3Data = (await conn.getAccountInfo(pool3))!.data;
  await waitForSlot(conn, p3Data.readBigUInt64LE(256));

  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, { pubkey: pool3, isSigner: false, isWritable: true }, { pubkey: queueA, isSigner: false, isWritable: true }, { pubkey: hist3, isSigner: false, isWritable: true }, { pubkey: queueB, isSigner: false, isWritable: true }, { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ], data: Buffer.from([2])
  })), [payer]);

  const bT2 = (await getAccount(conn, userBasics[2])).amount;
  await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PFDA_AMM_3_PROGRAM_ID, keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: false }, { pubkey: pool3, isSigner: false, isWritable: true }, { pubkey: hist3, isSigner: false, isWritable: false }, { pubkey: ticket3, isSigner: false, isWritable: true },
      ...amm3Vaults.map(v => ({ pubkey: v.publicKey, isSigner: false, isWritable: true })),
      ...userBasics.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ], data: Buffer.from([3])
  })), [payer]);
  const aT2 = (await getAccount(conn, userBasics[2])).amount;
  console.log(`  Token 2 received: ${(aT2 - bT2).toString()}`);
  
  console.log("\n=== FULL-CHAIN E2E TEST PASSED ===");
}

if (require.main === module) {
  runFullIntegrationTest().catch(err => {
    console.error("Error running test:", err);
    process.exit(1);
  });
} 

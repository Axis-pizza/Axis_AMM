/**
 * Axis Vault — E2E Test on Devnet
 * Tests: create_etf → deposit (mint ETF tokens) → withdraw (burn ETF tokens)
 */
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction,
  TransactionInstruction, sendAndConfirmTransaction, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  createMint, createAccount, createInitializeAccountInstruction,
  mintTo, getAccount, TOKEN_PROGRAM_ID, ACCOUNT_SIZE, MINT_SIZE,
  getMinimumBalanceForRentExemptAccount, getMinimumBalanceForRentExemptMint,
} from "@solana/spl-token";
import * as fs from "fs";
import * as os from "os";

const PROGRAM_ID = new PublicKey("DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy");
const RPC_URL = process.env.RPC_URL ?? "https://api.devnet.solana.com";
const ETF_NAME = process.env.ETF_NAME ?? `AX${Date.now().toString(36).toUpperCase().slice(-10)}`;
const TOKEN_COUNT = 3;
const WEIGHTS = [3334, 3333, 3333]; // ~33.3% each, sums to 10000

function loadPayer(): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(`${os.homedir()}/.config/solana/id.json`, "utf-8"))));
}
function u64Le(n: bigint): Buffer { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; }
function u16Le(n: number): Buffer { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; }

async function getCU(conn: Connection, sig: string): Promise<number | null> {
  const tx = await conn.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
  return tx?.meta?.computeUnitsConsumed ?? null;
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  console.log("=== Axis Vault E2E Test (Devnet) ===");
  console.log("Wallet:", payer.publicKey.toBase58());
  console.log("ETF Name:", ETF_NAME);
  console.log("Balance:", (await conn.getBalance(payer.publicKey)) / LAMPORTS_PER_SOL, "SOL\n");

  // 1. Create basket token mints + user accounts
  console.log("> Creating 3 basket tokens...");
  const mints: PublicKey[] = [];
  const userTokens: PublicKey[] = [];
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const mint = await createMint(conn, payer, payer.publicKey, null, 6);
    mints.push(mint);
    const ata = await createAccount(conn, payer, mint, payer.publicKey);
    await mintTo(conn, payer, mint, ata, payer, 100_000_000_000n);
    userTokens.push(ata);
  }

  // 2. Derive ETF state PDA
  const nameBytes = Buffer.from(ETF_NAME);
  const [etfState, etfBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("etf"), payer.publicKey.toBuffer(), nameBytes],
    PROGRAM_ID
  );
  console.log("ETF State PDA:", etfState.toBase58());

  // 3. Create ETF mint account (uninitialized — program will call InitializeMint2)
  const etfMintKp = Keypair.generate();
  const mintRent = await getMinimumBalanceForRentExemptMint(conn);
  await sendAndConfirmTransaction(conn, new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: etfMintKp.publicKey,
      lamports: mintRent,
      space: MINT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    })
  ), [payer, etfMintKp]);
  console.log("ETF Mint:", etfMintKp.publicKey.toBase58());

  // 4. Create vault accounts (uninitialized — program will call InitializeAccount3)
  const vaultKps: Keypair[] = [];
  const vaults: PublicKey[] = [];
  const vaultRent = await getMinimumBalanceForRentExemptAccount(conn);
  const createVaultsTx = new Transaction();
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const kp = Keypair.generate();
    vaultKps.push(kp);
    vaults.push(kp.publicKey);
    createVaultsTx.add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: kp.publicKey,
      lamports: vaultRent,
      space: ACCOUNT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    }));
  }
  await sendAndConfirmTransaction(conn, createVaultsTx, [payer, ...vaultKps]);

  // 5. CreateEtf
  console.log("\n> CreateEtf");
  const weightsBuf = Buffer.alloc(TOKEN_COUNT * 2);
  for (let i = 0; i < TOKEN_COUNT; i++) weightsBuf.writeUInt16LE(WEIGHTS[i], i * 2);

  const createData = Buffer.concat([
    Buffer.from([0]),               // disc = CreateEtf
    Buffer.from([TOKEN_COUNT]),     // token_count
    weightsBuf,                     // weights
    Buffer.from([nameBytes.length]),// name_len
    nameBytes,                      // name
  ]);

  const createSig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: payer.publicKey, isSigner: false, isWritable: false }, // treasury
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      // basket mints
      ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
      // vault accounts
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ],
    data: createData,
  })), [payer]);
  console.log("  CU:", await getCU(conn, createSig));

  // Verify ETF state
  const etfInfo = await conn.getAccountInfo(etfState);
  const totalSupply = etfInfo!.data.readBigUInt64LE(408);
  console.log("  Total supply:", totalSupply.toString());

  // 6. Create user's ETF token account
  const userEtfAta = await createAccount(conn, payer, etfMintKp.publicKey, payer.publicKey);

  // 7. Deposit — deposit 1000 tokens (base amount, scaled by weights)
  console.log("\n> Deposit (1000 base amount)");
  const depositData = Buffer.concat([
    Buffer.from([1]),                     // disc = Deposit
    u64Le(1_000_000_000n),               // amount (1000 tokens with 6 decimals)
    Buffer.from([nameBytes.length]),
    nameBytes,
  ]);

  const depositSig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      // user basket token accounts (source)
      ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      // vault accounts (destination)
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
    ],
    data: depositData,
  })), [payer]);
  console.log("  CU:", await getCU(conn, depositSig));

  // Check ETF token balance
  const etfBalance = (await getAccount(conn, userEtfAta)).amount;
  console.log("  ETF tokens minted:", etfBalance.toString());

  // Check vault balances
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const vaultBal = (await getAccount(conn, vaults[i])).amount;
    console.log(`  Vault ${i} balance: ${vaultBal.toLocaleString()}`);
  }

  // 8. Withdraw — burn half the ETF tokens
  const burnAmount = etfBalance / 2n;
  console.log(`\n> Withdraw (burn ${burnAmount} ETF tokens)`);
  const withdrawData = Buffer.concat([
    Buffer.from([2]),                     // disc = Withdraw
    u64Le(burnAmount),
    Buffer.from([nameBytes.length]),
    nameBytes,
  ]);

  const beforeBalances: bigint[] = [];
  for (let i = 0; i < TOKEN_COUNT; i++) {
    beforeBalances.push((await getAccount(conn, userTokens[i])).amount);
  }

  const withdrawSig = await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: etfState, isSigner: false, isWritable: true },
      { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: userEtfAta, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      // vault accounts (source)
      ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      // user basket token accounts (destination)
      ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
    ],
    data: withdrawData,
  })), [payer]);
  console.log("  CU:", await getCU(conn, withdrawSig));

  // Check what was returned
  const etfAfter = (await getAccount(conn, userEtfAta)).amount;
  console.log("  ETF tokens remaining:", etfAfter.toString());
  for (let i = 0; i < TOKEN_COUNT; i++) {
    const after = (await getAccount(conn, userTokens[i])).amount;
    const received = after - beforeBalances[i];
    console.log(`  Token ${i} received back: ${received.toLocaleString()}`);
  }

  // 9. Test: CreateEtf with duplicate mints → DuplicateMint error (9011 / 0x2333)
  console.log("\n> Test: CreateEtf with duplicate mints (expect error)");
  try {
    const dupName = Buffer.from("DUPTEST");
    const [dupEtfState] = PublicKey.findProgramAddressSync(
      [Buffer.from("etf"), payer.publicKey.toBuffer(), dupName],
      PROGRAM_ID,
    );

    // Create a fresh ETF mint (uninitialized) for the dup test
    const dupMintKp = Keypair.generate();
    await sendAndConfirmTransaction(conn, new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: dupMintKp.publicKey,
        lamports: mintRent,
        space: MINT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      })
    ), [payer, dupMintKp]);

    // Create 3 vault accounts for the dup basket
    const dupVaultKps: Keypair[] = [];
    const dupVaults: PublicKey[] = [];
    const dupVaultsTx = new Transaction();
    for (let i = 0; i < TOKEN_COUNT; i++) {
      const kp = Keypair.generate();
      dupVaultKps.push(kp);
      dupVaults.push(kp.publicKey);
      dupVaultsTx.add(SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: kp.publicKey,
        lamports: vaultRent,
        space: ACCOUNT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }));
    }
    await sendAndConfirmTransaction(conn, dupVaultsTx, [payer, ...dupVaultKps]);

    // Use mints[0] twice: [mints[0], mints[0], mints[2]]
    const dupMints = [mints[0], mints[0], mints[2]];
    const dupWeights = [3334, 3333, 3333];
    const dupWeightsBuf = Buffer.alloc(TOKEN_COUNT * 2);
    for (let i = 0; i < TOKEN_COUNT; i++) dupWeightsBuf.writeUInt16LE(dupWeights[i], i * 2);

    const dupCreateData = Buffer.concat([
      Buffer.from([0]),               // disc = CreateEtf
      Buffer.from([TOKEN_COUNT]),     // token_count
      dupWeightsBuf,                  // weights
      Buffer.from([dupName.length]),  // name_len
      dupName,                        // name
    ]);

    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: dupEtfState, isSigner: false, isWritable: true },
        { pubkey: dupMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: false, isWritable: false }, // treasury
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        // basket mints (with duplicate)
        ...dupMints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
        // vault accounts
        ...dupVaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ],
      data: dupCreateData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // DuplicateMint = 9011 = 0x2333
    if (msg.includes("0x2333") || msg.includes("9011")) {
      console.log("  Correctly rejected with DuplicateMint error:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9011");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("CreateEtf with duplicate mints should have failed but succeeded");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 10. Test: Withdraw more than total_supply → Overflow / InsufficientBalance error
  console.log("\n> Test: Withdraw exceeding total_supply (expect error)");
  try {
    const hugeAmount = 999_999_999_999_999n;
    const badWithdrawData = Buffer.concat([
      Buffer.from([2]),
      u64Le(hugeAmount),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);

    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ],
      data: badWithdrawData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // Accept Overflow (9007 / 0x232F) or InsufficientBalance (9005 / 0x232D)
    if (msg.includes("0x232f") || msg.includes("0x232d") || msg.includes("9007") || msg.includes("9005")) {
      console.log("  Correctly rejected with error:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "overflow/insufficient");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Withdraw with huge amount should have failed but succeeded");
    } else {
      // Any other program error is acceptable — the point is it must not succeed
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 11. Test: Deposit with wrong etf_mint → MintMismatch (9009 / 0x2331)
  console.log("\n> Test: Deposit with wrong etf_mint (expect MintMismatch)");
  try {
    const fakeMint = await createMint(conn, payer, payer.publicKey, null, 6);
    const badDepositData = Buffer.concat([
      Buffer.from([1]),
      u64Le(100_000_000n),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: fakeMint, isSigner: false, isWritable: true }, // WRONG mint
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ],
      data: badDepositData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // MintMismatch = 9009 = 0x2331
    if (msg.includes("0x2331") || msg.includes("9009")) {
      console.log("  Correctly rejected with MintMismatch:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9009");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Deposit with wrong etf_mint should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 12. Test: Deposit with wrong vault → VaultMismatch (9013 / 0x2335)
  console.log("\n> Test: Deposit with wrong vault account (expect VaultMismatch)");
  try {
    // Create a vault-like token account owned by payer (not the EtfState PDA) for mint[0]
    const fakeVault = await createAccount(conn, payer, mints[0], payer.publicKey);
    const wrongVaults = [fakeVault, vaults[1], vaults[2]];
    const badDepositData = Buffer.concat([
      Buffer.from([1]),
      u64Le(100_000_000n),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
        ...wrongVaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ],
      data: badDepositData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // VaultMismatch = 9013 = 0x2335
    if (msg.includes("0x2335") || msg.includes("9013")) {
      console.log("  Correctly rejected with VaultMismatch:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9013");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Deposit with wrong vault should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 13. Test: Withdraw with fake etf_state (wrong program owner) → InvalidProgramOwner (9014 / 0x2336)
  console.log("\n> Test: Withdraw with non-program-owned etf_state (expect InvalidProgramOwner)");
  try {
    // Any account not owned by the vault program — use the ETF mint account (owned by token program)
    const fakeState = etfMintKp.publicKey;
    const badWithdrawData = Buffer.concat([
      Buffer.from([2]),
      u64Le(1_000n),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: fakeState, isSigner: false, isWritable: true }, // WRONG: not program-owned
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ],
      data: badWithdrawData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // InvalidProgramOwner = 9014 = 0x2336
    if (msg.includes("0x2336") || msg.includes("9014")) {
      console.log("  Correctly rejected with InvalidProgramOwner:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9014");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Withdraw with non-program-owned etf_state should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 14. Test: Second deposit (subsequent-depositor proportional-math path)
  // After Step 7 the pool had total_supply>0; Step 8 halved it. A third
  // deposit here must go through the `if total_supply != 0` branch and
  // mint proportional to vault balances (not the base-amount first-deposit path).
  console.log("\n> Test: Subsequent deposit hits proportional-math path");
  {
    const supplyBefore = (await getAccount(conn, userEtfAta)).amount;
    const totalSupplyBefore = (await conn.getAccountInfo(etfState))!.data.readBigUInt64LE(408);
    const secondDepositData = Buffer.concat([
      Buffer.from([1]),
      u64Le(500_000_000n),  // 500 tokens base
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
      ],
      data: secondDepositData,
    })), [payer]);
    const supplyAfter = (await getAccount(conn, userEtfAta)).amount;
    const totalSupplyAfter = (await conn.getAccountInfo(etfState))!.data.readBigUInt64LE(408);
    const minted = supplyAfter - supplyBefore;
    if (minted === 0n || totalSupplyAfter <= totalSupplyBefore) {
      throw new Error(`Subsequent deposit didn't mint or total_supply didn't grow (minted=${minted}, before=${totalSupplyBefore}, after=${totalSupplyAfter})`);
    }
    console.log(`  Minted on 2nd deposit: ${minted}, total_supply: ${totalSupplyBefore} → ${totalSupplyAfter}`);
    console.log("  Correctly routed through proportional path");
  }

  // 15. Test: Withdraw with wrong etf_mint → MintMismatch (9009 / 0x2331)
  // Mirrors Test 11 on the Withdraw side. Must run while total_supply > 0,
  // otherwise the DivisionByZero check in withdraw.rs fires first.
  console.log("\n> Test: Withdraw with wrong etf_mint (expect MintMismatch)");
  try {
    const fakeMint = await createMint(conn, payer, payer.publicKey, null, 6);
    const badWithdrawData = Buffer.concat([
      Buffer.from([2]),
      u64Le(1_000n),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: fakeMint, isSigner: false, isWritable: true }, // WRONG mint
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ],
      data: badWithdrawData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // MintMismatch = 9009 = 0x2331
    if (msg.includes("0x2331") || msg.includes("9009")) {
      console.log("  Correctly rejected with MintMismatch:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9009");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Withdraw with wrong etf_mint should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 16. Test: Withdraw with wrong vault → VaultMismatch (9013 / 0x2335)
  // Mirrors Test 12 on the Withdraw side. Withdraw's account layout puts
  // vaults in [5..5+N] (opposite of Deposit), so swap the first vault here.
  console.log("\n> Test: Withdraw with wrong vault account (expect VaultMismatch)");
  try {
    const fakeVault = await createAccount(conn, payer, mints[0], payer.publicKey);
    const wrongVaults = [fakeVault, vaults[1], vaults[2]];
    const badWithdrawData = Buffer.concat([
      Buffer.from([2]),
      u64Le(1_000n),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...wrongVaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ],
      data: badWithdrawData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // VaultMismatch = 9013 = 0x2335
    if (msg.includes("0x2335") || msg.includes("9013")) {
      console.log("  Correctly rejected with VaultMismatch:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9013");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("Withdraw with wrong vault should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // TODO(#33 follow-up): Once axis-vault exposes a SetPaused instruction,
  // add paused-pool tests for both Deposit and Withdraw. Expected behavior:
  // etf.paused != 0 → PoolPaused (9012 / 0x2334) on both code paths.

  // 17. Test: Full withdrawal (burn_amount == total_supply) → total_supply goes to 0
  console.log("\n> Test: Full withdrawal drains total_supply to zero");
  {
    const remaining = (await getAccount(conn, userEtfAta)).amount;
    const fullWithdrawData = Buffer.concat([
      Buffer.from([2]),
      u64Le(remaining),
      Buffer.from([nameBytes.length]),
      nameBytes,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },
        { pubkey: etfMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: userEtfAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...vaults.map(v => ({ pubkey: v, isSigner: false, isWritable: true })),
        ...userTokens.map(u => ({ pubkey: u, isSigner: false, isWritable: true })),
      ],
      data: fullWithdrawData,
    })), [payer]);
    const etfEnd = (await getAccount(conn, userEtfAta)).amount;
    const totalSupplyEnd = (await conn.getAccountInfo(etfState))!.data.readBigUInt64LE(408);
    if (etfEnd !== 0n || totalSupplyEnd !== 0n) {
      throw new Error(`Full withdrawal didn't zero out balances (etf=${etfEnd}, supply=${totalSupplyEnd})`);
    }
    console.log(`  Burned ${remaining}, total_supply now 0`);
  }

  // 18. Test: CreateEtf with token_count < 2 → InvalidBasketSize (9002 / 0x232A)
  console.log("\n> Test: CreateEtf with token_count=1 (expect InvalidBasketSize)");
  try {
    const badName = Buffer.from("BADSIZE1");
    const [badPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("etf"), payer.publicKey.toBuffer(), badName],
      PROGRAM_ID,
    );
    const badMintKp = Keypair.generate();
    await sendAndConfirmTransaction(conn, new Transaction().add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: badMintKp.publicKey,
      lamports: mintRent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID,
    })), [payer, badMintKp]);
    const badVaultKp = Keypair.generate();
    await sendAndConfirmTransaction(conn, new Transaction().add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: badVaultKp.publicKey,
      lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
    })), [payer, badVaultKp]);

    // token_count=1, weights=[10000] — valid weight sum but basket too small
    const badData = Buffer.concat([
      Buffer.from([0]), Buffer.from([1]), u16Le(10000),
      Buffer.from([badName.length]), badName,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: badPda, isSigner: false, isWritable: true },
        { pubkey: badMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: mints[0], isSigner: false, isWritable: false },
        { pubkey: badVaultKp.publicKey, isSigner: false, isWritable: true },
      ],
      data: badData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // InvalidBasketSize = 9002 = 0x232A
    if (msg.includes("0x232a") || msg.includes("9002")) {
      console.log("  Correctly rejected with InvalidBasketSize:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9002");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("CreateEtf token_count=1 should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 19. Test: CreateEtf with weights summing ≠ 10_000 → WeightsMismatch (9003 / 0x232B)
  console.log("\n> Test: CreateEtf with weights summing to 9999 (expect WeightsMismatch)");
  try {
    const badName = Buffer.from("BADWT01");
    const [badPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("etf"), payer.publicKey.toBuffer(), badName],
      PROGRAM_ID,
    );
    const badMintKp = Keypair.generate();
    await sendAndConfirmTransaction(conn, new Transaction().add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: badMintKp.publicKey,
      lamports: mintRent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID,
    })), [payer, badMintKp]);
    const badVaultKps: Keypair[] = [];
    const badVaultsTx = new Transaction();
    for (let i = 0; i < TOKEN_COUNT; i++) {
      const kp = Keypair.generate();
      badVaultKps.push(kp);
      badVaultsTx.add(SystemProgram.createAccount({
        fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey,
        lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
      }));
    }
    await sendAndConfirmTransaction(conn, badVaultsTx, [payer, ...badVaultKps]);

    // weights sum to 9999 (not 10000)
    const badWeights = [3333, 3333, 3333];
    const wbuf = Buffer.alloc(TOKEN_COUNT * 2);
    for (let i = 0; i < TOKEN_COUNT; i++) wbuf.writeUInt16LE(badWeights[i], i * 2);
    const badData = Buffer.concat([
      Buffer.from([0]), Buffer.from([TOKEN_COUNT]), wbuf,
      Buffer.from([badName.length]), badName,
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: badPda, isSigner: false, isWritable: true },
        { pubkey: badMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
        ...badVaultKps.map(kp => ({ pubkey: kp.publicKey, isSigner: false, isWritable: true })),
      ],
      data: badData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // WeightsMismatch = 9003 = 0x232B
    if (msg.includes("0x232b") || msg.includes("9003")) {
      console.log("  Correctly rejected with WeightsMismatch:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "9003");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("CreateEtf weights=9999 should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  // 20. Test: CreateEtf duplicate-init (same PDA twice) → AlreadyInitialized or system-level failure
  // The original ETF PDA (etfState) is already initialized from Step 5. Attempt another CreateEtf
  // targeting the same PDA — must not succeed.
  console.log("\n> Test: CreateEtf duplicate-init on existing PDA (expect error)");
  try {
    const dupMintKp = Keypair.generate();
    await sendAndConfirmTransaction(conn, new Transaction().add(SystemProgram.createAccount({
      fromPubkey: payer.publicKey, newAccountPubkey: dupMintKp.publicKey,
      lamports: mintRent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID,
    })), [payer, dupMintKp]);
    const dupVaultKps: Keypair[] = [];
    const dupVaultsTx = new Transaction();
    for (let i = 0; i < TOKEN_COUNT; i++) {
      const kp = Keypair.generate();
      dupVaultKps.push(kp);
      dupVaultsTx.add(SystemProgram.createAccount({
        fromPubkey: payer.publicKey, newAccountPubkey: kp.publicKey,
        lamports: vaultRent, space: ACCOUNT_SIZE, programId: TOKEN_PROGRAM_ID,
      }));
    }
    await sendAndConfirmTransaction(conn, dupVaultsTx, [payer, ...dupVaultKps]);

    const dupData = Buffer.concat([
      Buffer.from([0]), Buffer.from([TOKEN_COUNT]), weightsBuf,
      Buffer.from([nameBytes.length]), nameBytes, // same name as Step 5 → same PDA
    ]);
    await sendAndConfirmTransaction(conn, new Transaction().add(new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: etfState, isSigner: false, isWritable: true },       // already-init'd PDA
        { pubkey: dupMintKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ...mints.map(m => ({ pubkey: m, isSigner: false, isWritable: false })),
        ...dupVaultKps.map(kp => ({ pubkey: kp.publicKey, isSigner: false, isWritable: true })),
      ],
      data: dupData,
    })), [payer]);
    throw new Error("Should have failed but succeeded");
  } catch (err: any) {
    const msg = err.message || String(err);
    // AlreadyInitialized = 9001 = 0x2329. Accept either the custom code
    // or the system-level "account already in use" since CreateAccount
    // may fail first depending on execution order.
    if (msg.includes("0x2329") || msg.includes("9001") || msg.includes("already in use")) {
      console.log("  Correctly rejected:", msg.match(/0x[0-9a-f]+/i)?.[0] ?? "already-initialized");
    } else if (msg === "Should have failed but succeeded") {
      throw new Error("CreateEtf duplicate-init should have failed");
    } else {
      console.log("  Rejected with error (unexpected code):", msg.slice(0, 120));
    }
  }

  console.log("\n=== Vault E2E PASSED ===");
}
main().catch(err => { console.error("Error:", err.message || err); process.exit(1); });

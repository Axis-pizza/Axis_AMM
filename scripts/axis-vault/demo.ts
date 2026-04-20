/**
 * Axis Vault SOL-in / SOL-out demo — builds deposit + withdraw plans
 * against a real Jupiter quote API, prints the resulting tx structure,
 * and exits. Does NOT sign or send.
 *
 * Runbook:
 *   # Against real mainnet Jupiter (internet required; does not sign):
 *   JUPITER_HOST=https://quote-api.jup.ag bun scripts/axis-vault/demo.ts
 *
 *   # Against a mainnet-fork local validator with Jupiter cloned:
 *   RPC_URL=http://localhost:8899 bun scripts/axis-vault/demo.ts
 *
 * Purpose: sanity-check that
 *   - Jupiter API responses deserialize into web3.js IXes cleanly,
 *   - ALT account loading resolves,
 *   - the final axis-vault Deposit / Withdraw IX matches the expected
 *     byte layout.
 *
 * Uses mainnet mints (wSOL, USDC, BONK, JTO) purely for the quote calls;
 * the `etfState` / `etfMint` / `vaults` / `treasuryEtfAta` placeholders
 * are fresh Keypairs since we never actually send the tx — we only
 * inspect its structure.
 */
import {
  Connection,
  Keypair,
  PublicKey,
  VersionedTransaction,
} from "@solana/web3.js";
import { buildDepositSolPlan } from "./deposit-sol";
import { buildWithdrawSolPlan } from "./withdraw-sol";

const RPC_URL = process.env.RPC_URL ?? "https://api.mainnet-beta.solana.com";
const JUPITER_HOST = process.env.JUPITER_HOST ?? "https://quote-api.jup.ag";

// Mainnet pubkeys — used for quote calls only, tx is never sent.
const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const BONK = new PublicKey("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263");
const JTO = new PublicKey("jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL");

const AXIS_VAULT_PROGRAM_ID = new PublicKey(
  "DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy",
);

function summarize(vtx: VersionedTransaction, label: string, extra: Record<string, unknown>) {
  const msg = vtx.message;
  const serializedLen = vtx.serialize().length;
  console.log(`--- ${label} ---`);
  console.log(`  serialized length   : ${serializedLen} / 1232 bytes`);
  console.log(`  static accounts     : ${msg.staticAccountKeys.length}`);
  console.log(`  compiled ixes       : ${msg.compiledInstructions.length}`);
  console.log(`  ALT lookups         : ${msg.addressTableLookups.length}`);
  console.log(`  signers             : ${msg.header.numRequiredSignatures}`);
  for (const [k, v] of Object.entries(extra)) {
    console.log(`  ${k.padEnd(20)}: ${String(v)}`);
  }
  console.log();
}

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const user = Keypair.generate().publicKey;

  // Fake ETF state — we never actually send the tx.
  const etfState = Keypair.generate().publicKey;
  const etfMint = Keypair.generate().publicKey;
  const treasuryEtfAta = Keypair.generate().publicKey;
  const vaults = [Keypair.generate().publicKey, Keypair.generate().publicKey, Keypair.generate().publicKey];

  console.log(`=== Axis Vault SOL-UX demo (RPC=${RPC_URL}) ===`);
  console.log(`Jupiter host : ${JUPITER_HOST}`);
  console.log(`User (fake)  : ${user.toBase58()}\n`);

  // --- SOL-in deposit plan ---
  const depositPlan = await buildDepositSolPlan({
    conn,
    user,
    programId: AXIS_VAULT_PROGRAM_ID,
    etfName: "DEMO",
    etfState,
    etfMint,
    treasuryEtfAta,
    basketMints: [USDC, BONK, JTO],
    weights: [5000, 3000, 2000], // 50 / 30 / 20
    vaults,
    solIn: 1_000_000_000n,        // 1 SOL
    minEtfOut: 0n,
    slippageBps: 100,             // 1 %
    jupiterHost: JUPITER_HOST,
  });
  summarize(depositPlan.versionedTx, "DepositSol plan", {
    depositAmount: depositPlan.depositAmount,
    perLegExpected: depositPlan.expectedBasketAmounts.join(", "),
    legQuotes: depositPlan.quotes.map(q => `${q.inAmount}→${q.outAmount}`).join(" | "),
    ixCount: depositPlan.ixCount,
    altCount: depositPlan.altAccounts.length,
  });

  // --- SOL-out withdraw plan ---
  // We can't actually estimate vault amounts against a fake etfState
  // (the helper reads on-chain), so skip the withdraw half of the demo
  // unless pointed at a real / forked validator with a live ETF.
  if (process.env.ETF_STATE && process.env.ETF_MINT) {
    try {
      const realEtfState = new PublicKey(process.env.ETF_STATE);
      const realEtfMint = new PublicKey(process.env.ETF_MINT);
      const realTreasuryEtfAta = new PublicKey(process.env.TREASURY_ETF_ATA!);
      const realVaults = (process.env.VAULTS ?? "").split(",").map(s => new PublicKey(s.trim()));
      const withdrawPlan = await buildWithdrawSolPlan({
        conn,
        user,
        programId: AXIS_VAULT_PROGRAM_ID,
        etfName: process.env.ETF_NAME ?? "DEMO",
        etfState: realEtfState,
        etfMint: realEtfMint,
        treasuryEtfAta: realTreasuryEtfAta,
        basketMints: [USDC, BONK, JTO],
        weights: [5000, 3000, 2000],
        vaults: realVaults,
        burnAmount: 1_000_000n,
        slippageBps: 100,
        jupiterHost: JUPITER_HOST,
      });
      summarize(withdrawPlan.versionedTx, "WithdrawSol plan", {
        expectedSolOut: withdrawPlan.expectedSolOut,
        minSolOutGuaranteed: withdrawPlan.minSolOutGuaranteed,
        ixCount: withdrawPlan.ixCount,
        altCount: withdrawPlan.altAccounts.length,
      });
    } catch (e: any) {
      console.log(`(WithdrawSol plan skipped: ${e.message})`);
    }
  } else {
    console.log("WithdrawSol plan skipped — set ETF_STATE / ETF_MINT / TREASURY_ETF_ATA / VAULTS env vars to a live ETF to exercise.");
  }
}

main().catch(err => {
  console.error("Error:", err.message || err);
  process.exit(1);
});

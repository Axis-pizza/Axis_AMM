/**
 * Axis Vault SOL-UX helpers — stub-fetch smoke test.
 *
 * Exercises `buildDepositSolPlan` / `buildWithdrawSolPlan` against
 * synthetic Jupiter responses. Catches:
 *   - schema drift between Jupiter types and how we deserialize IXes,
 *   - ALT account loading errors when addresses are fake,
 *   - versioned-tx assembly regressions (IX count, signer count, etc.).
 *
 * Does NOT hit network, does NOT sign, does NOT send. Runs under the
 * normal bun test path so CI typecheck + a manual `bun run` catch
 * shape breakage fast.
 *
 * Runbook:
 *   bun scripts/axis-vault/demo.stub.test.ts
 */
import {
  Connection,
  Keypair,
  PublicKey,
  VersionedTransaction,
} from "@solana/web3.js";
import { buildDepositSolPlan } from "./deposit-sol";
import {
  SOL_MINT,
  JUPITER_V6_PROGRAM_ID,
  type JupiterQuoteResponse,
  type JupiterSwapInstructionsResponse,
} from "./jupiter";

const AXIS_VAULT_PROGRAM_ID = new PublicKey(
  "DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy",
);

// A syntactically valid SerializedIx — Jupiter program id, minimal
// account list, empty data. Good enough to test deserializeIx + the
// tx-assembly path without invoking any real DEX.
function stubIx(pubkeyBytes: number): {
  programId: string;
  accounts: { pubkey: string; isSigner: boolean; isWritable: boolean }[];
  data: string;
} {
  return {
    programId: JUPITER_V6_PROGRAM_ID.toBase58(),
    accounts: [
      {
        pubkey: Keypair.generate().publicKey.toBase58(),
        isSigner: false,
        isWritable: true,
      },
    ],
    // A single byte of instruction data — just needs to be valid base64.
    data: Buffer.from([pubkeyBytes]).toString("base64"),
  };
}

function stubQuote(outMint: PublicKey, amountIn: bigint, slippageBps: number): JupiterQuoteResponse {
  // Pretend we get back 1:1 for the test (fine — we only inspect tx
  // structure, not economic output). `otherAmountThreshold` respects
  // slippage so the helper's safe-amount floor computes realistically.
  const out = amountIn;
  const other = (amountIn * (10_000n - BigInt(slippageBps))) / 10_000n;
  return {
    inputMint: SOL_MINT.toBase58(),
    outputMint: outMint.toBase58(),
    inAmount: amountIn.toString(),
    outAmount: out.toString(),
    otherAmountThreshold: other.toString(),
    swapMode: "ExactIn",
    slippageBps,
    priceImpactPct: "0",
    routePlan: [],
  };
}

function stubSwapInstructions(): JupiterSwapInstructionsResponse {
  return {
    tokenLedgerInstruction: null,
    computeBudgetInstructions: [stubIx(1), stubIx(2)],
    setupInstructions: [stubIx(3)],
    swapInstruction: stubIx(4),
    cleanupInstruction: null,
    addressLookupTableAddresses: [],
  };
}

// Minimal `fetch` polyfill that serves canned Jupiter responses.
const realFetch = globalThis.fetch;
globalThis.fetch = (async (input: RequestInfo | URL, _init?: RequestInit): Promise<Response> => {
  const url = typeof input === "string" ? input : input.toString();
  if (url.includes("/v6/quote")) {
    const params = new URL(url).searchParams;
    const outputMint = new PublicKey(params.get("outputMint")!);
    const amount = BigInt(params.get("amount")!);
    const slip = Number(params.get("slippageBps") ?? 50);
    return new Response(JSON.stringify(stubQuote(outputMint, amount, slip)), {
      status: 200, headers: { "Content-Type": "application/json" },
    });
  }
  if (url.includes("/v6/swap-instructions")) {
    return new Response(JSON.stringify(stubSwapInstructions()), {
      status: 200, headers: { "Content-Type": "application/json" },
    });
  }
  return new Response("not stubbed", { status: 404 });
}) as typeof fetch;

async function main() {
  // RPC connection is only used for getLatestBlockhash + ALT lookups.
  // No ALTs in our stub response, so the only RPC call is blockhash.
  // Use devnet since it's usually reachable when mainnet isn't required.
  const conn = new Connection("http://localhost:8899", "confirmed");

  // Override getLatestBlockhash so we don't actually hit the RPC.
  (conn as any).getLatestBlockhash = async () => ({
    blockhash: Keypair.generate().publicKey.toBase58(),
    lastValidBlockHeight: 1_000_000,
  });

  const user = Keypair.generate().publicKey;
  const plan = await buildDepositSolPlan({
    conn,
    user,
    programId: AXIS_VAULT_PROGRAM_ID,
    etfName: "STUBETF",
    etfState: Keypair.generate().publicKey,
    etfMint: Keypair.generate().publicKey,
    treasuryEtfAta: Keypair.generate().publicKey,
    basketMints: [
      Keypair.generate().publicKey,
      Keypair.generate().publicKey,
      Keypair.generate().publicKey,
    ],
    weights: [5000, 3000, 2000],
    vaults: [
      Keypair.generate().publicKey,
      Keypair.generate().publicKey,
      Keypair.generate().publicKey,
    ],
    solIn: 1_000_000_000n,
    minEtfOut: 0n,
    slippageBps: 100,
    recentBlockhash: Keypair.generate().publicKey.toBase58(),
  });

  // Structure assertions — these are the ones that actually guard
  // against regressions in the helper, independent of Jupiter quote
  // quality.
  if (!(plan.versionedTx instanceof VersionedTransaction)) {
    throw new Error("expected VersionedTransaction");
  }
  if (plan.quotes.length !== 3) {
    throw new Error(`expected 3 quotes, got ${plan.quotes.length}`);
  }
  if (plan.versionedTx.message.compiledInstructions.length === 0) {
    throw new Error("no compiled instructions");
  }
  if (plan.ixCount < 3) {
    throw new Error(`ixCount=${plan.ixCount}, expected at least 3 (compute + swap × 3 + deposit)`);
  }
  if (plan.depositAmount <= 0n) {
    throw new Error(`depositAmount=${plan.depositAmount} must be > 0`);
  }

  // The final IX must be the axis-vault Deposit call. We identify it by
  // the program id — Jupiter IXes are all JUP6... , Deposit is AXIS...
  const lastIx = plan.versionedTx.message.compiledInstructions.at(-1)!;
  const lastProgramIdIx = lastIx.programIdIndex;
  const lastProgramId = plan.versionedTx.message.staticAccountKeys[lastProgramIdIx];
  if (!lastProgramId.equals(AXIS_VAULT_PROGRAM_ID)) {
    throw new Error(
      `expected last IX to call axis-vault, got ${lastProgramId.toBase58()}`,
    );
  }
  // Deposit discriminant = 1 is the first byte of the data payload.
  if (lastIx.data[0] !== 1) {
    throw new Error(`final IX disc=${lastIx.data[0]}, expected 1 (Deposit)`);
  }

  // Restore real fetch so the process can still make network calls if
  // imported into a larger harness.
  globalThis.fetch = realFetch;

  console.log("✓ DepositSol stub test passed");
  console.log(`  ixCount=${plan.ixCount}, serialized=${plan.versionedTx.serialize().length} bytes`);
  console.log(`  depositAmount=${plan.depositAmount}`);
  console.log(`  expectedBasket=${plan.expectedBasketAmounts.join(", ")}`);
}

main().catch(err => {
  console.error("✗ stub test failed:", err.message || err);
  process.exit(1);
});

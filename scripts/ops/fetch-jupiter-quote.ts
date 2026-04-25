/**
 * Fetch a Jupiter v6 quote + swap-instructions and dump the response into
 * test/fixtures/jupiter/<pair>.json so mainnet-fork tests can replay it
 * deterministically. Closes #61 item 5(c).
 *
 * Why a recorded fixture instead of live-fetching from CI:
 *   - Jupiter route topology shifts whenever a new AMM pool is added
 *     or liquidity moves. Live tests are flaky; a frozen response is
 *     reproducible.
 *   - The swap-instructions endpoint requires the user pubkey for ATA
 *     derivation. Pinning a known wallet keeps the route accounts
 *     stable.
 *
 * Usage:
 *   bun scripts/ops/fetch-jupiter-quote.ts \
 *     --in So11111111111111111111111111111111111111112 \
 *     --out EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
 *     --amount 100000000 \
 *     --user FoR9joeqd...someBase58... \
 *     --label sol-usdc-100m
 *
 * --in / --out  : SPL token mints (default: SOL → USDC)
 * --amount      : input amount in lamports / smallest unit (default 1e8)
 * --user        : user pubkey for ATA derivation. If omitted, derives a
 *                 deterministic placeholder so route accounts stay stable
 *                 across machines (the placeholder is NOT a real wallet —
 *                 callers replace `userPublicKey` at test time).
 * --label       : output filename stem (default: built from in/out/amount)
 * --slippage    : bps, default 50
 */

import * as fs from "fs";
import * as path from "path";

// Jupiter migrated `quote-api.jup.ag/v6/*` to `lite-api.jup.ag/swap/v1/*`
// (free tier) in 2025-Q1. The on-chain V6 program ID is unchanged; only
// the off-chain route endpoint moved. The lite-api host has no auth
// requirement and is fine for CI rate limits.
const QUOTE_URL = "https://lite-api.jup.ag/swap/v1/quote";
const SWAP_INSTRUCTIONS_URL = "https://lite-api.jup.ag/swap/v1/swap-instructions";

const SOL_MINT = "So11111111111111111111111111111111111111112";
const USDC_MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

// Stable placeholder pubkey so the swap-instructions response is the same
// regardless of who runs this script. NOT a real wallet — the test should
// rebuild the AccountMeta list with its own user pubkey at run time.
const PLACEHOLDER_USER = "11111111111111111111111111111112";

function arg(name: string, fallback?: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  if (idx === -1 || idx + 1 >= process.argv.length) return fallback;
  return process.argv[idx + 1];
}

async function fetchJson(url: string, init?: RequestInit): Promise<any> {
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const r = await fetch(url, init);
      if (!r.ok) {
        throw new Error(`HTTP ${r.status}: ${await r.text()}`);
      }
      return await r.json();
    } catch (e) {
      if (attempt === 3) throw e;
      console.warn(`  attempt ${attempt} failed: ${e}; retrying...`);
      await new Promise(r => setTimeout(r, attempt * 1000));
    }
  }
}

async function main() {
  const inputMint = arg("in", SOL_MINT)!;
  const outputMint = arg("out", USDC_MINT)!;
  const amount = arg("amount", "100000000")!;
  const user = arg("user", PLACEHOLDER_USER)!;
  const slippageBps = arg("slippage", "50")!;
  const label = arg(
    "label",
    `${inputMint.slice(0, 4)}-${outputMint.slice(0, 4)}-${amount}`,
  )!;

  console.log("Fetching Jupiter v6 quote");
  console.log(`  in     : ${inputMint}`);
  console.log(`  out    : ${outputMint}`);
  console.log(`  amount : ${amount}`);
  console.log(`  user   : ${user}`);
  console.log(`  slip   : ${slippageBps} bps`);

  const quoteUrl = new URL(QUOTE_URL);
  quoteUrl.searchParams.set("inputMint", inputMint);
  quoteUrl.searchParams.set("outputMint", outputMint);
  quoteUrl.searchParams.set("amount", amount);
  quoteUrl.searchParams.set("slippageBps", slippageBps);
  quoteUrl.searchParams.set("onlyDirectRoutes", "false");

  const quote = await fetchJson(quoteUrl.toString());
  console.log(`  ✓ quote: ${quote.outAmount} out, ${quote.routePlan?.length ?? 0} hops`);

  const swap = await fetchJson(SWAP_INSTRUCTIONS_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      quoteResponse: quote,
      userPublicKey: user,
      wrapAndUnwrapSol: false,
    }),
  });

  // The instructions response carries:
  //   tokenLedgerInstruction, computeBudgetInstructions, setupInstructions,
  //   swapInstruction, cleanupInstruction, otherInstructions, addressLookupTableAddresses
  //
  // For axis-g3m's RebalanceViaJupiter the relevant pieces are:
  //   - swapInstruction.programId (must equal JUP6Lkb...)
  //   - swapInstruction.data (hex-encoded route bytes — passes through to CPI)
  //   - swapInstruction.accounts (account list — caller's program signs;
  //     for a vault-PDA-signed swap the user account replaces with the
  //     pool PDA and is_signer becomes a PDA-signer at CPI time)
  //   - addressLookupTableAddresses (ALT pubkeys to clone onto the
  //     forked validator)
  console.log(`  ✓ swap ix: ${swap.swapInstruction?.accounts?.length ?? 0} accounts`);
  console.log(`  ✓ ALTs   : ${(swap.addressLookupTableAddresses ?? []).length}`);

  // Write under <repo-root>/test/fixtures/jupiter regardless of which
  // directory the script is invoked from. __dirname points at
  // scripts/ops/, so two `..` to repo root. Using __dirname instead of
  // bun's import.meta.dir keeps the TS check (module: commonjs) green.
  const repoRoot = path.resolve(__dirname, "../..");
  const fixtureDir = path.join(repoRoot, "test/fixtures/jupiter");
  fs.mkdirSync(fixtureDir, { recursive: true });
  const outPath = path.join(fixtureDir, `${label}.json`);

  const fixture = {
    fetchedAt: new Date().toISOString(),
    inputMint,
    outputMint,
    amount,
    slippageBps: Number(slippageBps),
    userPlaceholder: user,
    note:
      "This response was recorded against mainnet-beta. To replay against " +
      "a forked validator, clone every account in `swapInstruction.accounts` " +
      "and every pubkey in `addressLookupTableAddresses` from mainnet-beta " +
      "before running the test. The user account at the swap instruction's " +
      "user position must be rewritten to the test wallet's pubkey at run " +
      "time (the placeholder above is for stable hashing only).",
    quote,
    swap,
  };

  fs.writeFileSync(outPath, JSON.stringify(fixture, null, 2));
  console.log(`\n  wrote ${outPath}`);
  console.log(`  size : ${(fs.statSync(outPath).size / 1024).toFixed(1)} KB`);
}

main().catch((e) => {
  console.error("✗", e);
  process.exit(1);
});

/**
 * Switchboard On-Demand Feed Setup for Axis Protocol
 *
 * Creates SOL/USD, BONK/USD, and WIF/USD price feeds on Solana devnet.
 *
 * Usage: npx ts-node setup-switchboard-feeds.ts
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as os from "os";

const RPC_URL = "https://api.devnet.solana.com";

function loadPayer(): Keypair {
  const path = `${os.homedir()}/.config/solana/id.json`;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

const FEED_JOBS: Record<string, any[]> = {
  "SOL/USD": [
    { tasks: [
      { httpTask: { url: "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd" } },
      { jsonParseTask: { path: "$.solana.usd" } },
    ]},
  ],
  "BONK/USD": [
    { tasks: [
      { httpTask: { url: "https://api.coingecko.com/api/v3/simple/price?ids=bonk&vs_currencies=usd" } },
      { jsonParseTask: { path: "$.bonk.usd" } },
    ]},
  ],
  "WIF/USD": [
    { tasks: [
      { httpTask: { url: "https://api.coingecko.com/api/v3/simple/price?ids=dogwifcoin&vs_currencies=usd" } },
      { jsonParseTask: { path: "$.dogwifcoin.usd" } },
    ]},
  ],
};

async function main() {
  const conn = new Connection(RPC_URL, "confirmed");
  const payer = loadPayer();

  const sb = require("@switchboard-xyz/on-demand");

  console.log("╔══════════════════════════════════════════════════════╗");
  console.log("║  Switchboard On-Demand Feed Setup (Devnet)           ║");
  console.log("╚══════════════════════════════════════════════════════╝");
  console.log(`Wallet: ${payer.publicKey.toBase58()}\n`);

  const feedAddresses: Record<string, string> = {};

  for (const [pair, jobs] of Object.entries(FEED_JOBS)) {
    console.log(`▶ ${pair}:`);

    try {
      // Step 1: Compute feed hash from job definitions
      const feedHash = sb.PullFeed.feedHashFromParams({ jobs });
      console.log(`  Feed hash: ${feedHash}`);

      // Step 2: Store feed config via Crossbar
      const stored = await sb.storeFeed({ queue: "F8ce7MsckeZAbAGmxjJNetxYXQa9mKr9nnrC3qKubyYy", jobs });
      console.log(`  Stored:    ${JSON.stringify(stored).slice(0, 100)}`);

      feedAddresses[pair] = feedHash;
    } catch (err: any) {
      console.log(`  Error: ${err.message}`);

      // Fallback: just compute the feed hash locally
      try {
        const feedHash = sb.PullFeed.feedHashFromParams({ jobs });
        console.log(`  Feed hash (local): ${feedHash}`);
        feedAddresses[pair] = `hash:${feedHash}`;
      } catch (e2: any) {
        console.log(`  Hash error: ${e2.message}`);
      }
    }
  }

  console.log("\n━━━ Feed Hashes ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  for (const [pair, hash] of Object.entries(feedAddresses)) {
    console.log(`  ${pair}: ${hash}`);
  }

  // Save
  const outputPath = `${__dirname}/switchboard-feeds.json`;
  fs.writeFileSync(outputPath, JSON.stringify(feedAddresses, null, 2));
  console.log(`\nSaved to: ${outputPath}`);
}

main().catch(err => { console.error("Error:", err); process.exit(1); });

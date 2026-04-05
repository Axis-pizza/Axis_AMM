/**
 * Axis Protocol — A/B Test Metrics Collector
 *
 * Polls devnet at a fixed interval and records metrics for both ETF A and ETF B.
 * Writes JSONL files to disk for post-test analysis.
 *
 * Usage:
 *   npx ts-node collect-ab-metrics.ts [--config ab-metrics-config.json]
 *
 * Outputs:
 *   metrics/etf-a-metrics.jsonl
 *   metrics/etf-b-metrics.jsonl
 *
 * Stop with Ctrl+C. Gracefully flushes on SIGINT.
 */

import { Connection, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

// ─── Config ──────────────────────────────────────────────────────────────

interface Config {
  rpc_url: string;
  poll_interval_ms: number;
  output_dir: string;
  etf_a: {
    program_id: string;
    label: string;
    pool_address: string;
    treasury_address: string;
    oracle_feed: string;
  };
  etf_b: {
    program_id: string;
    label: string;
    pool_address: string;
  };
}

function loadConfig(): Config {
  const configArg = process.argv.find((_, i, a) => a[i - 1] === "--config") || "ab-metrics-config.json";
  const configPath = path.resolve(__dirname, configArg);
  return JSON.parse(fs.readFileSync(configPath, "utf-8"));
}

// ─── PoolState3 reader (ETF A) ──────────────────────────────────────────

interface EtfASnapshot {
  ts: string;
  slot: number;
  batch_id: number;
  window_end: number;
  reserves: number[];
  base_fee_bps: number;
  treasury_balance_lamports: number | null;
  oracle_price_raw: string | null;
}

async function readEtfA(conn: Connection, config: Config): Promise<EtfASnapshot | null> {
  if (!config.etf_a.pool_address) return null;

  try {
    const poolPk = new PublicKey(config.etf_a.pool_address);
    const info = await conn.getAccountInfo(poolPk);
    if (!info) return null;

    const d = info.data;
    // PoolState3 layout: disc(8) + token_mints(96) + vaults(96) + reserves(24) + weights(12) + ...
    const reserves = [
      Number(d.readBigUInt64LE(8 + 96 + 96)),
      Number(d.readBigUInt64LE(8 + 96 + 96 + 8)),
      Number(d.readBigUInt64LE(8 + 96 + 96 + 16)),
    ];
    const batchId = Number(d.readBigUInt64LE(8 + 96 + 96 + 24 + 12 + 8)); // after reserves + weights + window_slots
    const windowEnd = Number(d.readBigUInt64LE(8 + 96 + 96 + 24 + 12 + 8 + 8));
    const baseFee = d.readUInt16LE(8 + 96 + 96 + 24 + 12 + 8 + 8 + 8 + 32 + 32);

    let treasuryBal: number | null = null;
    if (config.etf_a.treasury_address) {
      try {
        treasuryBal = await conn.getBalance(new PublicKey(config.etf_a.treasury_address));
      } catch {}
    }

    let oraclePrice: string | null = null;
    if (config.etf_a.oracle_feed) {
      try {
        const feedInfo = await conn.getAccountInfo(new PublicKey(config.etf_a.oracle_feed));
        if (feedInfo && feedInfo.data.length >= 1288) {
          const priceBytes = feedInfo.data.slice(1272, 1288);
          const lo = priceBytes.readBigUInt64LE(0);
          const hi = priceBytes.readBigInt64LE(8);
          oraclePrice = ((hi << 64n) | lo).toString();
        }
      } catch {}
    }

    const slot = await conn.getSlot("confirmed");

    return {
      ts: new Date().toISOString(),
      slot,
      batch_id: batchId,
      window_end: windowEnd,
      reserves,
      base_fee_bps: baseFee,
      treasury_balance_lamports: treasuryBal,
      oracle_price_raw: oraclePrice,
    };
  } catch (err: any) {
    console.error(`  [ETF A] Error: ${err.message}`);
    return null;
  }
}

// ─── G3mPoolState reader (ETF B) ────────────────────────────────────────

interface EtfBSnapshot {
  ts: string;
  slot: number;
  reserves: number[];
  drift_max_bps: number;
  drift_max_token: number;
  needs_rebalance: boolean;
  invariant_k_lo: number;
  invariant_k_hi: number;
  fee_rate_bps: number;
  last_rebalance_slot: number;
}

async function readEtfB(conn: Connection, config: Config): Promise<EtfBSnapshot | null> {
  if (!config.etf_b.pool_address) return null;

  try {
    const poolPk = new PublicKey(config.etf_b.pool_address);
    const info = await conn.getAccountInfo(poolPk);
    if (!info) return null;

    const d = info.data;
    const tokenCount = d[40];

    // Read reserves (offset 376 per e2e-devnet.ts readPoolState)
    const reserves: number[] = [];
    for (let i = 0; i < tokenCount; i++) {
      reserves.push(Number(d.readBigUInt64LE(376 + i * 8)));
    }

    // Read target weights (offset 362, u16 each — but with padding, actually at offset after vaults)
    // Use offsets from e2e-devnet.ts: weights start after token_vaults
    const weights: number[] = [];
    // Approximate: target_weights_bps at offset 362 (after 8+32+1+160+160 = 361, +1 padding = 362)
    for (let i = 0; i < tokenCount; i++) {
      weights.push(d.readUInt16LE(362 + i * 2));
    }

    // Compute drift per token
    let totalWeighted = 0;
    for (let i = 0; i < tokenCount; i++) {
      totalWeighted += reserves[i] * weights[i];
    }

    let maxDrift = 0;
    let maxDriftToken = 0;
    const threshold = d.readUInt16LE(434); // drift_threshold_bps

    for (let i = 0; i < tokenCount; i++) {
      if (totalWeighted === 0 || weights[i] === 0) continue;
      const actual = (reserves[i] * weights[i] * 10000) / totalWeighted;
      const target = weights[i]; // target weight is already in bps (2000 = 20%)
      // drift_bps = |actual - target| / target * 10000
      // But actual here is scaled to 10000 already from actual_weight_bps formula
      // Simpler: actual_bps = actual (already 0-10000 range)
      // drift = |actual_bps - target_bps| / target_bps * 10000
      const diff = Math.abs(actual - target);
      const drift = Math.floor((diff * 10000) / target);
      if (drift > maxDrift) {
        maxDrift = drift;
        maxDriftToken = i;
      }
    }

    const needsRebalance = maxDrift > threshold;
    const kLo = Number(d.readBigUInt64LE(416));
    const kHi = Number(d.readBigUInt64LE(424));
    const feeRate = d.readUInt16LE(432);
    const lastRebalSlot = Number(d.readBigUInt64LE(440));
    const slot = await conn.getSlot("confirmed");

    return {
      ts: new Date().toISOString(),
      slot,
      reserves,
      drift_max_bps: maxDrift,
      drift_max_token: maxDriftToken,
      needs_rebalance: needsRebalance,
      invariant_k_lo: kLo,
      invariant_k_hi: kHi,
      fee_rate_bps: feeRate,
      last_rebalance_slot: lastRebalSlot,
    };
  } catch (err: any) {
    console.error(`  [ETF B] Error: ${err.message}`);
    return null;
  }
}

// ─── Main loop ───────────────────────────────────────────────────────────

async function main() {
  const config = loadConfig();
  const conn = new Connection(config.rpc_url, "confirmed");

  const outDir = path.resolve(__dirname, config.output_dir);
  if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });

  const etfAPath = path.join(outDir, "etf-a-metrics.jsonl");
  const etfBPath = path.join(outDir, "etf-b-metrics.jsonl");

  const etfAStream = fs.createWriteStream(etfAPath, { flags: "a" });
  const etfBStream = fs.createWriteStream(etfBPath, { flags: "a" });

  console.log("╔══════════════════════════════════════════════════╗");
  console.log("║  Axis A/B Test — Metrics Collector                ║");
  console.log("╚══════════════════════════════════════════════════╝");
  console.log(`RPC          : ${config.rpc_url}`);
  console.log(`Poll interval: ${config.poll_interval_ms}ms`);
  console.log(`Output       : ${outDir}/`);
  console.log(`ETF A pool   : ${config.etf_a.pool_address || "(not configured)"}`);
  console.log(`ETF B pool   : ${config.etf_b.pool_address || "(not configured)"}`);
  console.log(`\nPress Ctrl+C to stop.\n`);

  let running = true;
  let pollCount = 0;

  process.on("SIGINT", () => {
    console.log(`\nStopping after ${pollCount} polls.`);
    running = false;
    etfAStream.end();
    etfBStream.end();
    process.exit(0);
  });

  while (running) {
    pollCount++;
    const now = new Date().toISOString();
    process.stdout.write(`[${now}] Poll #${pollCount}...`);

    const [snapA, snapB] = await Promise.all([
      readEtfA(conn, config),
      readEtfB(conn, config),
    ]);

    if (snapA) {
      etfAStream.write(JSON.stringify(snapA) + "\n");
      process.stdout.write(` A:batch=${snapA.batch_id}`);
      if (snapA.treasury_balance_lamports !== null) {
        process.stdout.write(`,treasury=${snapA.treasury_balance_lamports}`);
      }
    } else {
      process.stdout.write(" A:skip");
    }

    if (snapB) {
      etfBStream.write(JSON.stringify(snapB) + "\n");
      process.stdout.write(` B:drift=${snapB.drift_max_bps}bps`);
      if (snapB.needs_rebalance) process.stdout.write("(!)");
      process.stdout.write(`,k_lo=${snapB.invariant_k_lo}`);
    } else {
      process.stdout.write(" B:skip");
    }

    console.log();

    await new Promise(r => setTimeout(r, config.poll_interval_ms));
  }
}

main().catch(err => {
  console.error("Fatal:", err);
  process.exit(1);
});

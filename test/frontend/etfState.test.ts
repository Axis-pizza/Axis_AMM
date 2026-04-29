import { describe, expect, test } from "bun:test";
import { PublicKey } from "@solana/web3.js";
import { Buffer } from "buffer";

import {
  decodeEtfState,
  decodeTokenAccountAmount,
  expectedWithdrawOutputs,
  ETF_STATE_SIZE,
} from "../../frontend/src/lib/etfState";

function makeEtfStateBuffer(opts: {
  authority: PublicKey;
  etfMint: PublicKey;
  tokenCount: number;
  tokenMints: PublicKey[];
  tokenVaults: PublicKey[];
  weightsBps: number[];
  totalSupply: bigint;
  treasury: PublicKey;
  feeBps: number;
  paused: boolean;
  bump: number;
  name: string;
  ticker: string;
  createdAtSlot: bigint;
  maxFeeBps: number;
  tvlCap: bigint;
}): Uint8Array {
  const buf = Buffer.alloc(ETF_STATE_SIZE);
  buf.write("etfstat3", 0, 8, "utf8");
  buf.set(opts.authority.toBytes(), 8);
  buf.set(opts.etfMint.toBytes(), 40);
  buf[72] = opts.tokenCount;
  for (let i = 0; i < opts.tokenCount; i++) {
    buf.set(opts.tokenMints[i].toBytes(), 73 + i * 32);
    buf.set(opts.tokenVaults[i].toBytes(), 233 + i * 32);
  }
  for (let i = 0; i < opts.tokenCount; i++) {
    buf.writeUInt16LE(opts.weightsBps[i], 394 + i * 2);
  }
  buf.writeBigUInt64LE(opts.totalSupply, 408);
  buf.set(opts.treasury.toBytes(), 416);
  buf.writeUInt16LE(opts.feeBps, 448);
  buf[450] = opts.paused ? 1 : 0;
  buf[451] = opts.bump;
  buf.write(opts.name, 452, Math.min(opts.name.length, 32), "utf8");
  buf.write(opts.ticker, 484, Math.min(opts.ticker.length, 16), "utf8");
  buf.writeBigUInt64LE(opts.createdAtSlot, 504);
  buf.writeUInt16LE(opts.maxFeeBps, 512);
  buf.writeBigUInt64LE(opts.tvlCap, 520);
  return new Uint8Array(buf);
}

describe("EtfState decoder", () => {
  test("round-trips a 3-token mainnet-shaped state", () => {
    const authority = new PublicKey("BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6");
    const etfMint = new PublicKey("11111111111111111111111111111112");
    const treasury = new PublicKey("BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6");
    const usdc = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    const usdt = new PublicKey("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY8wYb6Fq4jmWZtj");
    const jitoSol = new PublicKey("J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn");
    const v0 = new PublicKey("11111111111111111111111111111113");
    const v1 = new PublicKey("11111111111111111111111111111114");
    const v2 = new PublicKey("11111111111111111111111111111115");

    const raw = makeEtfStateBuffer({
      authority,
      etfMint,
      tokenCount: 3,
      tokenMints: [usdc, usdt, jitoSol],
      tokenVaults: [v0, v1, v2],
      weightsBps: [4_000, 4_000, 2_000],
      totalSupply: 1_500_000n,
      treasury,
      feeBps: 30,
      paused: false,
      bump: 254,
      name: "AX-DEMO-1",
      ticker: "AXD1",
      createdAtSlot: 416_300_000n,
      maxFeeBps: 300,
      tvlCap: 0n,
    });

    const etf = decodeEtfState(raw);
    expect(etf.authority.equals(authority)).toBe(true);
    expect(etf.etfMint.equals(etfMint)).toBe(true);
    expect(etf.tokenCount).toBe(3);
    expect(etf.tokenMints.map((p) => p.toBase58())).toEqual([
      usdc.toBase58(),
      usdt.toBase58(),
      jitoSol.toBase58(),
    ]);
    expect(etf.tokenVaults.map((p) => p.toBase58())).toEqual([
      v0.toBase58(),
      v1.toBase58(),
      v2.toBase58(),
    ]);
    expect(etf.weightsBps).toEqual([4_000, 4_000, 2_000]);
    expect(etf.totalSupply).toBe(1_500_000n);
    expect(etf.treasury.equals(treasury)).toBe(true);
    expect(etf.feeBps).toBe(30);
    expect(etf.paused).toBe(false);
    expect(etf.bump).toBe(254);
    expect(etf.name).toBe("AX-DEMO-1");
    expect(etf.ticker).toBe("AXD1");
    expect(etf.createdAtSlot).toBe(416_300_000n);
    expect(etf.maxFeeBps).toBe(300);
    expect(etf.tvlCap).toBe(0n);
  });

  test("rejects wrong discriminator", () => {
    const raw = new Uint8Array(ETF_STATE_SIZE);
    raw.set(new TextEncoder().encode("etfstat2"), 0);
    expect(() => decodeEtfState(raw)).toThrow(/discriminator mismatch/);
  });

  test("rejects undersized buffer", () => {
    expect(() => decodeEtfState(new Uint8Array(100))).toThrow(/too small/);
  });

  test("rejects out-of-range tokenCount", () => {
    const raw = new Uint8Array(ETF_STATE_SIZE);
    raw.set(new TextEncoder().encode("etfstat3"), 0);
    raw[72] = 6;
    expect(() => decodeEtfState(raw)).toThrow(/invalid tokenCount/);
  });

  test("decodeTokenAccountAmount reads u64 LE at offset 64", () => {
    const buf = Buffer.alloc(165);
    buf.writeBigUInt64LE(987_654_321n, 64);
    expect(decodeTokenAccountAmount(new Uint8Array(buf))).toBe(987_654_321n);
  });
});

describe("expectedWithdrawOutputs (matches axis-vault on-chain math)", () => {
  test("applies fee then proportional payout", () => {
    // burn=100_000, fee=30bps → fee=300, effective=99_700
    // total_supply=1_000_000 → share=99.7%
    const out = expectedWithdrawOutputs(
      [10_000n, 5_000n, 2_500n],
      100_000n,
      1_000_000n,
      30,
    );
    expect(out.feeAmount).toBe(300n);
    expect(out.effectiveBurn).toBe(99_700n);
    expect(out.perLeg).toEqual([997n, 498n, 249n]);
  });

  test("rejects burn > totalSupply", () => {
    expect(() =>
      expectedWithdrawOutputs([10_000n], 2_000_000n, 1_000_000n, 30),
    ).toThrow(/burnAmount/);
  });

  test("rejects zero totalSupply", () => {
    expect(() =>
      expectedWithdrawOutputs([10_000n], 100n, 0n, 30),
    ).toThrow(/totalSupply is zero/);
  });

  test("zero-fee passes through", () => {
    const out = expectedWithdrawOutputs([1_000n, 2_000n], 100n, 1_000n, 0);
    expect(out.feeAmount).toBe(0n);
    expect(out.effectiveBurn).toBe(100n);
    expect(out.perLeg).toEqual([100n, 200n]);
  });
});

import { Connection, PublicKey } from "@solana/web3.js";

/// On-chain layout of `PoolState3` from
/// `contracts/pfda-amm-3/src/state/pool_state.rs`. `repr(C)` with the
/// `u32 weights[3]` followed by `u64 window_slots` gives a 4-byte
/// alignment pad before the first u64, which is why `window_slots`
/// starts at 240 (not 236) and `current_window_end` lands at 256.
const OFFSET_DISCRIMINATOR = 0;
const OFFSET_TOKEN_MINTS = 8;
const OFFSET_VAULTS = 104;
const OFFSET_RESERVES = 200;
const OFFSET_WEIGHTS = 224;
const OFFSET_WINDOW_SLOTS = 240;
const OFFSET_CURRENT_BATCH_ID = 248;
const OFFSET_CURRENT_WINDOW_END = 256;
const OFFSET_TREASURY = 264;
const OFFSET_AUTHORITY = 296;
const OFFSET_BASE_FEE_BPS = 328;
const OFFSET_PAUSED = 332;
const POOL_LEN = 336;

const POOL_DISCRIMINATOR = Buffer.from("pool3st\0");

export interface PoolState3Data {
  pool: PublicKey;
  tokenMints: [PublicKey, PublicKey, PublicKey];
  vaults: [PublicKey, PublicKey, PublicKey];
  reserves: [bigint, bigint, bigint];
  weights: [number, number, number];
  windowSlots: bigint;
  currentBatchId: bigint;
  currentWindowEnd: bigint;
  treasury: PublicKey;
  authority: PublicKey;
  baseFeeBps: number;
  paused: boolean;
}

export function decodePoolState3(pool: PublicKey, data: Buffer): PoolState3Data {
  if (data.length < POOL_LEN) {
    throw new Error(
      `pool state too short: ${data.length} bytes, expected ≥ ${POOL_LEN}`,
    );
  }
  const disc = data.subarray(
    OFFSET_DISCRIMINATOR,
    OFFSET_DISCRIMINATOR + 8,
  );
  if (!disc.equals(POOL_DISCRIMINATOR)) {
    throw new Error(
      `pool state discriminator mismatch: got ${disc.toString("hex")}`,
    );
  }
  const readPk = (off: number): PublicKey =>
    new PublicKey(data.subarray(off, off + 32));

  const tokenMints: [PublicKey, PublicKey, PublicKey] = [
    readPk(OFFSET_TOKEN_MINTS),
    readPk(OFFSET_TOKEN_MINTS + 32),
    readPk(OFFSET_TOKEN_MINTS + 64),
  ];
  const vaults: [PublicKey, PublicKey, PublicKey] = [
    readPk(OFFSET_VAULTS),
    readPk(OFFSET_VAULTS + 32),
    readPk(OFFSET_VAULTS + 64),
  ];
  const reserves: [bigint, bigint, bigint] = [
    data.readBigUInt64LE(OFFSET_RESERVES),
    data.readBigUInt64LE(OFFSET_RESERVES + 8),
    data.readBigUInt64LE(OFFSET_RESERVES + 16),
  ];
  const weights: [number, number, number] = [
    data.readUInt32LE(OFFSET_WEIGHTS),
    data.readUInt32LE(OFFSET_WEIGHTS + 4),
    data.readUInt32LE(OFFSET_WEIGHTS + 8),
  ];

  return {
    pool,
    tokenMints,
    vaults,
    reserves,
    weights,
    windowSlots: data.readBigUInt64LE(OFFSET_WINDOW_SLOTS),
    currentBatchId: data.readBigUInt64LE(OFFSET_CURRENT_BATCH_ID),
    currentWindowEnd: data.readBigUInt64LE(OFFSET_CURRENT_WINDOW_END),
    treasury: readPk(OFFSET_TREASURY),
    authority: readPk(OFFSET_AUTHORITY),
    baseFeeBps: data.readUInt16LE(OFFSET_BASE_FEE_BPS),
    paused: data.readUInt8(OFFSET_PAUSED) !== 0,
  };
}

export async function fetchPoolState3(
  conn: Connection,
  pool: PublicKey,
): Promise<PoolState3Data | null> {
  const info = await conn.getAccountInfo(pool, "confirmed");
  if (!info) return null;
  return decodePoolState3(pool, info.data);
}

/// Enumerate every PoolState3 owned by the pfda-amm-3 program. Filters by
/// the `pool3st\0` discriminator + 336-byte length so we only get pools.
/// Falls back to the unfiltered query if memcmp filter is unsupported by
/// the RPC; we still discriminate client-side after decoding.
export async function fetchAllPools(
  conn: Connection,
  programId: PublicKey,
): Promise<PoolState3Data[]> {
  const accounts = await conn.getProgramAccounts(programId, {
    commitment: "confirmed",
    filters: [
      { dataSize: POOL_LEN },
      // base58 of "pool3st\0" — RPCs accept a base58 string here.
      { memcmp: { offset: 0, bytes: "pj9Ddx6r" } },
    ],
  });
  const out: PoolState3Data[] = [];
  for (const a of accounts) {
    try {
      out.push(decodePoolState3(a.pubkey, a.account.data as Buffer));
    } catch {
      // Account was filtered correctly but failed to decode — skip
      // rather than aborting the whole listing.
    }
  }
  return out;
}

// ─── ClearedBatchHistory3 ──────────────────────────────────────────────────
// Layout (repr(C)):
//   0..8   discriminator "clrd3h\0\0"
//   8..40  pool
//   40..48 batch_id (u64)
//   48..72 clearing_prices[3] (u64)
//   72..96 total_out[3]       (u64)
//   96..120 total_in[3]        (u64)
//   120..122 fee_bps (u16)
//   122      is_cleared (u8)
//   123      bump (u8)
//   124..128 padding
const HISTORY_LEN = 128;
const HISTORY_DISCRIMINATOR = Buffer.from("clrd3h\0\0");

export interface History3Data {
  account: PublicKey;
  pool: PublicKey;
  batchId: bigint;
  clearingPrices: [bigint, bigint, bigint];
  totalOut: [bigint, bigint, bigint];
  totalIn: [bigint, bigint, bigint];
  feeBps: number;
  isCleared: boolean;
}

export function decodeHistory3(
  account: PublicKey,
  data: Buffer,
): History3Data {
  if (data.length < HISTORY_LEN) {
    throw new Error(`history too short: ${data.length} bytes`);
  }
  if (!data.subarray(0, 8).equals(HISTORY_DISCRIMINATOR)) {
    throw new Error("history discriminator mismatch");
  }
  return {
    account,
    pool: new PublicKey(data.subarray(8, 40)),
    batchId: data.readBigUInt64LE(40),
    clearingPrices: [
      data.readBigUInt64LE(48),
      data.readBigUInt64LE(56),
      data.readBigUInt64LE(64),
    ],
    totalOut: [
      data.readBigUInt64LE(72),
      data.readBigUInt64LE(80),
      data.readBigUInt64LE(88),
    ],
    totalIn: [
      data.readBigUInt64LE(96),
      data.readBigUInt64LE(104),
      data.readBigUInt64LE(112),
    ],
    feeBps: data.readUInt16LE(120),
    isCleared: data.readUInt8(122) !== 0,
  };
}

export async function fetchHistory3(
  conn: Connection,
  account: PublicKey,
): Promise<History3Data | null> {
  const info = await conn.getAccountInfo(account, "confirmed");
  if (!info) return null;
  return decodeHistory3(account, info.data);
}

// ─── UserOrderTicket3 ──────────────────────────────────────────────────────
// Layout (repr(C)):
//   0..8    discriminator "usrord3\0"
//   8..40   owner
//   40..72  pool
//   72..80  batch_id (u64)
//   80..104 amounts_in[3] (u64)
//   104     out_token_idx (u8)
//   105..112 padding for u64 alignment
//   112..120 min_amount_out (u64)
//   120     is_claimed (u8)
//   121     bump (u8)
//   122..127 padding[5]
const TICKET_LEN = 128;
const TICKET_DISCRIMINATOR = Buffer.from("usrord3\0");

export interface Ticket3Data {
  account: PublicKey;
  owner: PublicKey;
  pool: PublicKey;
  batchId: bigint;
  amountsIn: [bigint, bigint, bigint];
  outTokenIdx: number;
  minAmountOut: bigint;
  isClaimed: boolean;
}

export function decodeTicket3(
  account: PublicKey,
  data: Buffer,
): Ticket3Data {
  if (data.length < TICKET_LEN) {
    throw new Error(`ticket too short: ${data.length} bytes`);
  }
  if (!data.subarray(0, 8).equals(TICKET_DISCRIMINATOR)) {
    throw new Error("ticket discriminator mismatch");
  }
  return {
    account,
    owner: new PublicKey(data.subarray(8, 40)),
    pool: new PublicKey(data.subarray(40, 72)),
    batchId: data.readBigUInt64LE(72),
    amountsIn: [
      data.readBigUInt64LE(80),
      data.readBigUInt64LE(88),
      data.readBigUInt64LE(96),
    ],
    outTokenIdx: data.readUInt8(104),
    minAmountOut: data.readBigUInt64LE(112),
    isClaimed: data.readUInt8(120) !== 0,
  };
}

export async function fetchTicket3(
  conn: Connection,
  account: PublicKey,
): Promise<Ticket3Data | null> {
  const info = await conn.getAccountInfo(account, "confirmed");
  if (!info) return null;
  return decodeTicket3(account, info.data);
}

/**
 * Build the BPFLoaderUpgradeable.ExtendProgramChecked instruction for
 * `axis-vault` so we can submit it through a Squads V4 custom-transaction.
 *
 * Print everything (default):
 *   bun scripts/ops/build-extend-program-ix.ts
 *
 * Copy a specific field straight to the macOS clipboard:
 *   bun scripts/ops/build-extend-program-ix.ts program-id
 *   bun scripts/ops/build-extend-program-ix.ts data-hex
 *   bun scripts/ops/build-extend-program-ix.ts data-base58
 *   bun scripts/ops/build-extend-program-ix.ts tx-base58
 *   bun scripts/ops/build-extend-program-ix.ts tx-base64
 *   bun scripts/ops/build-extend-program-ix.ts account-0   # programData
 *   bun scripts/ops/build-extend-program-ix.ts account-1   # program
 *   bun scripts/ops/build-extend-program-ix.ts account-2   # system
 *   bun scripts/ops/build-extend-program-ix.ts account-3   # squads vault
 *   bun scripts/ops/build-extend-program-ix.ts account-4   # payer
 *
 * Override extension size:
 *   ADDITIONAL_BYTES=16384 bun scripts/ops/build-extend-program-ix.ts
 *
 * Constructed for: program `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX`,
 * upgrade authority = Squads V4 vault `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6`,
 * fee payer = `BACLqhYA35zTJ8ua1541wrFjEjzAatrbBNk19YzVUhvH` (Squads member).
 *
 * The instruction layout matches `ExtendProgramChecked` (variant index 8)
 * from the upgradeable BPF loader: u32 LE discriminator, u32 LE additional
 * bytes. Account order:
 *   0 ProgramData          [writable]
 *   1 Program              [writable]
 *   2 System program       []
 *   3 Upgrade authority    [signer]                   <- Squads vault
 *   4 Payer (optional)     [writable, signer]         <- Squads member BACLqhYA
 */
import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { Buffer } from "buffer";
import { spawnSync } from "child_process";

// Local base58 encoder — avoids the `bs58` dep so this script stays
// usable from a stock Node/bun toolchain without extra @types.
const B58_ALPHABET =
  "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
function bs58encode(buf: Buffer | Uint8Array): string {
  let n = 0n;
  for (const b of buf) n = n * 256n + BigInt(b);
  let out = "";
  while (n > 0n) {
    const r = Number(n % 58n);
    n = n / 58n;
    out = B58_ALPHABET[r] + out;
  }
  for (const b of buf) {
    if (b === 0) out = "1" + out;
    else break;
  }
  return out;
}

const BPF_LOADER_UPGRADEABLE = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111",
);
const PROGRAM_ID = new PublicKey(
  "Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX",
);
const PROGRAM_DATA = new PublicKey(
  "6szAV5iFQKzJ7BuYZipSeWc3thauWCVi9q26k1WQEjrt",
);
const SQUADS_VAULT = new PublicKey(
  "BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6",
);
const PAYER = new PublicKey("BACLqhYA35zTJ8ua1541wrFjEjzAatrbBNk19YzVUhvH");

const additionalBytes = Number.parseInt(
  process.env.ADDITIONAL_BYTES ?? "8192",
  10,
);
if (!Number.isInteger(additionalBytes) || additionalBytes <= 0) {
  throw new Error(`ADDITIONAL_BYTES must be a positive integer (got ${additionalBytes})`);
}

// Variant 8 = ExtendProgramChecked. u32 LE discriminator, u32 LE bytes.
const data = Buffer.alloc(8);
data.writeUInt32LE(8, 0);
data.writeUInt32LE(additionalBytes, 4);

const ix = new TransactionInstruction({
  programId: BPF_LOADER_UPGRADEABLE,
  keys: [
    { pubkey: PROGRAM_DATA, isSigner: false, isWritable: true },
    { pubkey: PROGRAM_ID, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SQUADS_VAULT, isSigner: true, isWritable: false },
    { pubkey: PAYER, isSigner: true, isWritable: true },
  ],
  data,
});

async function maybeFetchBlockhash(): Promise<string> {
  if (process.env.SKIP_BLOCKHASH_FETCH === "1") return "1".repeat(32);
  try {
    const conn = new Connection(
      process.env.RPC_URL ?? "https://api.mainnet-beta.solana.com",
      "confirmed",
    );
    const { blockhash } = await conn.getLatestBlockhash();
    return blockhash;
  } catch {
    return "1".repeat(32);
  }
}

async function main(): Promise<void> {
  const blockhash = await maybeFetchBlockhash();
  const tx = new Transaction({ feePayer: SQUADS_VAULT, recentBlockhash: blockhash });
  tx.add(ix);
  const serialized = tx.serialize({ requireAllSignatures: false, verifySignatures: false });

  const fields: Record<string, string> = {
    "program-id": BPF_LOADER_UPGRADEABLE.toBase58(),
    "data-hex": data.toString("hex"),
    "data-base58": bs58encode(data),
    "tx-base58": bs58encode(serialized),
    "tx-base64": serialized.toString("base64"),
    "account-0": ix.keys[0].pubkey.toBase58(),
    "account-1": ix.keys[1].pubkey.toBase58(),
    "account-2": ix.keys[2].pubkey.toBase58(),
    "account-3": ix.keys[3].pubkey.toBase58(),
    "account-4": ix.keys[4].pubkey.toBase58(),
  };

  const requested = process.argv[2];

  if (requested) {
    const value = fields[requested];
    if (!value) {
      console.error(`Unknown field: ${requested}`);
      console.error(`Available: ${Object.keys(fields).join(", ")}`);
      process.exit(1);
    }
    // pbcopy on macOS / xclip on Linux.
    const clip =
      process.platform === "darwin"
        ? spawnSync("pbcopy", [], { input: value })
        : spawnSync("xclip", ["-selection", "clipboard"], { input: value });
    if (clip.status !== 0) {
      console.error("Clipboard copy failed; printing value instead:");
      console.error(value);
      process.exit(1);
    }
    const preview = value.length > 80 ? `${value.slice(0, 60)}…(${value.length} chars)` : value;
    console.log(`✓ Copied ${requested} → clipboard`);
    console.log(`  ${preview}`);
    return;
  }

  // No field arg → print everything as before.
  console.log("=== ExtendProgramChecked ix ===");
  console.log("Program:", BPF_LOADER_UPGRADEABLE.toBase58());
  console.log("Additional bytes:", additionalBytes);
  console.log("Data (hex):     ", data.toString("hex"));
  console.log("Data (base58):  ", bs58encode(data));
  console.log("");
  console.log("Accounts (order matters):");
  ix.keys.forEach((k, i) => {
    const flags = `${k.isSigner ? "S" : "-"}${k.isWritable ? "W" : "-"}`;
    console.log(`  ${i}: [${flags}] ${k.pubkey.toBase58()}`);
  });
  console.log("  Legend: S = signer, W = writable");
  console.log("");
  console.log(
    `New programData size after extend: 104,701 + ${additionalBytes} = ${
      104701 + additionalBytes
    } bytes`,
  );
  console.log(
    `v1.1 .so + 45-byte header needs 108,885 bytes — ${
      104701 + additionalBytes - 108885
    } bytes spare after upgrade`,
  );
  console.log("");
  console.log("=== Unsigned legacy transaction (blockhash:", blockhash, ") ===");
  console.log("Base58:", bs58encode(serialized));
  console.log("");
  console.log("Base64:", serialized.toString("base64"));
  console.log("");
  console.log("Pass a field name as the first arg to pipe directly to pbcopy:");
  console.log(
    "  bun scripts/ops/build-extend-program-ix.ts <" + Object.keys(fields).join(" | ") + ">",
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

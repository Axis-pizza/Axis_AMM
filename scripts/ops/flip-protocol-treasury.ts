/**
 * Post-Squads-provisioning helper. Takes a base58 vault key, validates
 * the format, rewrites the PROTOCOL_TREASURY constant in
 * contracts/axis-vault/src/constants.rs in place, and prints the next
 * deploy steps. Closes the code-side of #61 item 3.
 *
 * Usage:
 *   bun scripts/ops/flip-protocol-treasury.ts <vault-base58>
 *
 * Pre-conditions (kidney + muse must complete BEFORE running this):
 *   1. Squads V4 2-of-2 multisig provisioned on devnet (kidney + muse)
 *   2. Devnet end-to-end test passed (CreateEtf → DepositSol → ...)
 *   3. Squads V4 2-of-2 multisig provisioned on MAINNET
 *   4. Mainnet vault address copied from the Squads UI
 *
 * Don't run this script before step 3. Flipping the constant against a
 * not-yet-deployed Squads vault key ships a binary pointing at a key
 * nobody controls — bricks the treasury gate until you redeploy.
 */

import * as fs from "fs";
import * as path from "path";

const repoRoot = path.resolve(import.meta.dir, "../..");
const CONSTANTS_PATH = path.join(
  repoRoot,
  "contracts/axis-vault/src/constants.rs",
);

function decodeBase58(s: string): Uint8Array {
  // Tiny base58 decode — enough for a 32-byte pubkey.
  const ALPHABET =
    "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let n = 0n;
  for (const c of s) {
    const i = ALPHABET.indexOf(c);
    if (i < 0) throw new Error(`invalid base58 char: ${c}`);
    n = n * 58n + BigInt(i);
  }
  // Account for leading 1s → leading zero bytes.
  let leadingZeros = 0;
  for (const c of s) {
    if (c === "1") leadingZeros++;
    else break;
  }
  const bytes: number[] = [];
  while (n > 0n) {
    bytes.push(Number(n & 0xffn));
    n >>= 8n;
  }
  while (leadingZeros-- > 0) bytes.push(0);
  return new Uint8Array(bytes.reverse());
}

function formatAsRustArray(bytes: Uint8Array): string {
  // Match the existing TOKEN_PROGRAM_ID style: 8 bytes per line, 2-hex,
  // 4-space indent.
  const lines: string[] = [];
  for (let i = 0; i < 32; i += 8) {
    const row = Array.from(bytes.slice(i, i + 8))
      .map((b) => `0x${b.toString(16).padStart(2, "0")}`)
      .join(", ");
    lines.push(`    ${row},`);
  }
  return lines.join("\n");
}

function main() {
  const vaultB58 = process.argv[2];
  if (!vaultB58) {
    console.error("usage: bun scripts/ops/flip-protocol-treasury.ts <vault-base58>");
    process.exit(1);
  }

  const bytes = decodeBase58(vaultB58);
  if (bytes.length !== 32) {
    console.error(`✗ expected 32 bytes, got ${bytes.length}`);
    process.exit(1);
  }
  if (bytes.every((b) => b === 0)) {
    console.error("✗ refusing to write the all-zero key (that's the inert sentinel)");
    process.exit(1);
  }

  const src = fs.readFileSync(CONSTANTS_PATH, "utf-8");
  const constName = "PROTOCOL_TREASURY";
  const re = new RegExp(
    `pub const ${constName}: \\[u8; 32\\] = \\[[\\s\\S]*?\\];`,
  );
  if (!re.test(src)) {
    console.error(`✗ could not locate ${constName} in ${CONSTANTS_PATH}`);
    process.exit(1);
  }

  const replacement =
    `pub const ${constName}: [u8; 32] = [\n${formatAsRustArray(bytes)}\n];`;
  const next = src.replace(re, replacement);

  if (next === src) {
    console.error("✗ replacement produced no change — already pointing at this key?");
    process.exit(1);
  }

  fs.writeFileSync(CONSTANTS_PATH, next);
  console.log(`  ✓ wrote ${vaultB58}`);
  console.log(`  ✓ updated ${path.relative(repoRoot, CONSTANTS_PATH)}`);
  console.log();
  console.log("Next steps (must do, in order):");
  console.log("  1. Update contracts/axis-vault/src/lib.rs declare_id! to the");
  console.log("     mainnet program ID (was a placeholder).");
  console.log("  2. cargo build-sbf --manifest-path contracts/axis-vault/Cargo.toml");
  console.log("  3. Verify the .so size hasn't drifted: `ls -la contracts/axis-vault/target/deploy/axis_vault.so`");
  console.log("  4. Muse deploys via Squads (the deploy itself is a Squads tx).");
  console.log("  5. Verify on-chain:");
  console.log(`       solana account <axis_vault_program_id> --output json | jq .`);
  console.log(`       # then a CreateEtf tx that exercises the gate — must succeed`);
  console.log(`       # for treasury == ${vaultB58} and reject for any other key.`);
  console.log("  6. Close #59 + the related items in #61.");
}

main();

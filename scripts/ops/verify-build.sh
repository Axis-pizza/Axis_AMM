#!/usr/bin/env bash
# verify-build.sh — drive the reproducible Docker build for axis-vault
# and compare the resulting .so hash against the deployed mainnet
# program. Goal: prove that a future `solana-verify verify-from-repo`
# will succeed before we ask the Squads multisig to sign anything.
#
# Outputs:
#   - SHA-256 of the local docker-built .so
#   - SHA-256 of the deployed mainnet program (`Agae3Wet…`)
#   - PASS/FAIL match summary
#
# Hash equality means the on-chain program was built from this commit
# under the same toolchain — `verify-from-repo` will succeed.
# Hash inequality means we must redeploy the docker-built .so first;
# see `docs/ops/SOLSCAN_VERIFY_RUNBOOK.md` for the Squads redeploy path.
#
# This script does NOT touch the chain. It only reads, builds, and
# diffs hashes.

set -euo pipefail

PROGRAM_ID="${PROGRAM_ID:-Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX}"
LIBRARY_NAME="${LIBRARY_NAME:-axis_vault}"
MOUNT_PATH="${MOUNT_PATH:-contracts/axis-vault}"
RPC_URL="${RPC_URL:-https://api.mainnet-beta.solana.com}"
# Pinocchio programs don't pull in `solana-program`, so solana-verify
# can't auto-detect a Solana version from Cargo.lock. Pin the docker
# image explicitly. Default tracks the local toolchain
# (solana-cli 3.0.15 → closest verifiable-build tag is 3.0.14).
BASE_IMAGE="${BASE_IMAGE:-solanafoundation/solana-verifiable-build:3.0.14}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing dep: $1" >&2
    exit 1
  fi
}

require solana-verify
require docker
require solana

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not running. Open Docker Desktop and retry." >&2
  exit 2
fi

echo "── 1/3  Reproducible docker build ($MOUNT_PATH, image $BASE_IMAGE) ──"
# Docker requires an absolute mount path; relative paths get interpreted
# as named-volume identifiers and fail with "invalid characters".
ABS_MOUNT="$REPO_ROOT/$MOUNT_PATH"
solana-verify build \
  --library-name "$LIBRARY_NAME" \
  --base-image "$BASE_IMAGE" \
  "$ABS_MOUNT"

# `solana-verify build` writes the .so to <mount>/target/deploy/<lib>.so
LOCAL_SO="$MOUNT_PATH/target/deploy/${LIBRARY_NAME}.so"
if [ ! -f "$LOCAL_SO" ]; then
  echo "expected build output not found: $LOCAL_SO" >&2
  exit 3
fi

echo
echo "── 2/3  Hash the local build ──"
LOCAL_HASH="$(solana-verify get-executable-hash "$LOCAL_SO")"
LOCAL_RAW_HASH="$(shasum -a 256 "$LOCAL_SO" | awk '{print $1}')"
echo "local docker (verify-normalized) : $LOCAL_HASH"
echo "local docker (raw sha256)        : $LOCAL_RAW_HASH"

echo
echo "── 3/3  Hash the on-chain program ──"
ONCHAIN_HASH="$(solana-verify get-program-hash "$PROGRAM_ID" -u "$RPC_URL")"
echo "on-chain (verify-normalized)     : $ONCHAIN_HASH"
TMP_DUMP="$(mktemp -t axis_vault_onchain.XXXXXX.so)"
trap 'rm -f "$TMP_DUMP"' EXIT
solana program dump "$PROGRAM_ID" "$TMP_DUMP" -u "$RPC_URL" >/dev/null
ONCHAIN_RAW_HASH="$(shasum -a 256 "$TMP_DUMP" | awk '{print $1}')"
echo "on-chain (raw sha256)            : $ONCHAIN_RAW_HASH"

echo
if [ "$LOCAL_HASH" = "$ONCHAIN_HASH" ]; then
  echo "PASS: hashes match. Proceed with the verify-from-repo Squads tx export."
  echo "Next:"
  echo "  solana-verify export-pda-tx \\"
  echo "    --library-name $LIBRARY_NAME \\"
  echo "    --mount-path $MOUNT_PATH \\"
  echo "    --program-id $PROGRAM_ID \\"
  echo "    --uploader <SQUADS_VAULT_KEY> \\"
  echo "    https://github.com/Axis-pizza/Axis_AMM"
  exit 0
else
  echo "FAIL: hash mismatch. The deployed binary was not built reproducibly."
  echo "Path forward: re-deploy the docker-built .so through Squads, then re-run."
  echo "See docs/ops/SOLSCAN_VERIFY_RUNBOOK.md \"Hash mismatch\" section."
  exit 4
fi

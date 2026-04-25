#!/usr/bin/env bash
#
# Refresh contracts/axis-g3m/fixtures/jupiter_v6.so by dumping the
# current Jupiter V6 mainnet binary. Run this on a developer machine
# whenever Jupiter announces a program upgrade — committing the result
# keeps CI deterministic without round-tripping through mainnet RPC.
#
# Why a manual refresh: the public RPCs that allow anonymous program
# dumps (mainnet-beta, projectserum, ankr public endpoint) all proved
# too unreliable under CI load. A committed binary trades ~3 MB of
# repo size for deterministic CI.
#
# Usage:
#   scripts/ops/refresh-jupiter-fixture.sh [rpc-url]
#
# Defaults to https://api.mainnet-beta.solana.com if no URL passed.
# Use a paid Helius/Triton/QuickNode endpoint if mainnet-beta is
# being flaky on your end.

set -euo pipefail

RPC_URL="${1:-https://api.mainnet-beta.solana.com}"
JUPITER_V6_ID="JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"
OUT="contracts/axis-g3m/fixtures/jupiter_v6.so"

if ! command -v solana >/dev/null; then
  echo "✗ solana CLI not on PATH" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUT")"

# Snapshot the existing binary's size + sha for the diff log
prev_size=""
prev_sha=""
if [ -f "$OUT" ]; then
  prev_size=$(stat -f%z "$OUT" 2>/dev/null || stat -c%s "$OUT" 2>/dev/null)
  prev_sha=$(shasum -a 256 "$OUT" | cut -d' ' -f1)
fi

echo "Dumping $JUPITER_V6_ID from $RPC_URL"
solana program dump -u "$RPC_URL" "$JUPITER_V6_ID" "$OUT"

new_size=$(stat -f%z "$OUT" 2>/dev/null || stat -c%s "$OUT" 2>/dev/null)
new_sha=$(shasum -a 256 "$OUT" | cut -d' ' -f1)

echo
echo "  ✓ wrote $OUT ($new_size bytes)"
echo "    sha256: $new_sha"
if [ -n "$prev_size" ]; then
  if [ "$prev_sha" = "$new_sha" ]; then
    echo "    no change vs previous binary — Jupiter V6 not upgraded"
  else
    echo "    previous: $prev_size bytes / $prev_sha"
    echo "  Commit with: git add $OUT && git commit -m 'ops: refresh Jupiter V6 fixture'"
  fi
fi

#!/usr/bin/env bash
#
# Dump every account a Jupiter swap-instructions fixture references, so
# the mainnet-fork e2e can boot a solana-test-validator entirely from
# local files (no `--clone --url mainnet` at startup, which is what the
# Main AB Report workflow used to fail on). Closes #61 item 5(b).
#
# Usage:
#   scripts/ops/dump-jupiter-fixture-accounts.sh <fixture.json> [out-dir]
#
# Reads the fixture's swap.swapInstruction.accounts[*].pubkey and
# swap.addressLookupTableAddresses[*], dumps each via `solana account`
# with multi-RPC retry, and writes:
#
#   out-dir/<pubkey>.json     # one file per account
#   out-dir/clone-args.txt    # ready-to-paste `--account ...` args
#
# The validator command in the runbook copies clone-args.txt verbatim
# so the dump and the launch never re-touch the network.

set -euo pipefail

FIXTURE="${1:-}"
OUT_DIR="${2:-test/fixtures/jupiter/accounts}"

if [ -z "$FIXTURE" ] || [ ! -f "$FIXTURE" ]; then
  echo "usage: $0 <fixture.json> [out-dir]" >&2
  exit 1
fi

if ! command -v jq >/dev/null; then
  echo "✗ jq is required (brew install jq / apt install jq)" >&2
  exit 1
fi

if ! command -v solana >/dev/null; then
  echo "✗ solana CLI not on PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
> "$OUT_DIR/clone-args.txt"

URLS=(
  "https://api.mainnet-beta.solana.com"
  "https://solana-api.projectserum.com"
  "https://rpc.ankr.com/solana"
)

dump_one() {
  local pubkey="$1"
  local out="$OUT_DIR/$pubkey.json"

  if [ -f "$out" ]; then
    return 0
  fi

  for url in "${URLS[@]}"; do
    for attempt in 1 2 3; do
      if solana account "$pubkey" -u "$url" --output json --output-file "$out" >/dev/null 2>&1; then
        return 0
      fi
      sleep $((attempt * 2))
    done
  done
  echo "  ✗ $pubkey: all RPCs failed" >&2
  return 1
}

PUBKEYS=$(jq -r '
  ((.swap.swapInstruction.accounts // []) | .[].pubkey),
  ((.swap.addressLookupTableAddresses // []) | .[])
' "$FIXTURE" | sort -u)

count=0
for pk in $PUBKEYS; do
  count=$((count + 1))
  echo "  [$count] $pk"
  if dump_one "$pk"; then
    echo "--account $pk $OUT_DIR/$pk.json" >> "$OUT_DIR/clone-args.txt"
  fi
done

echo
echo "  ✓ dumped $count accounts to $OUT_DIR/"
echo "  ✓ clone-args.txt ready for solana-test-validator"

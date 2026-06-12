#!/usr/bin/env bash
# Build a Solana SBF program reliably despite the cargo-build-sbf /
# rustup-1.27 toolchain mis-parse. Usage:
#   ci/build-sbf.sh <crate-dir> <crate-name> [--features f1,f2]
set -euo pipefail
CRATE_DIR="$1"; CRATE_NAME="$2"; shift 2
FEATURES_ARG=()
if [[ "${1:-}" == "--features" ]]; then FEATURES_ARG=(--features "$2"); shift 2; fi

# Pick the platform-tools sbf toolchain from rustup (split off the TAB+path).
SBF_TC="$(rustup toolchain list | grep -E 'sbpf-solana' | head -1 | awk '{print $1}')"
if [[ -z "$SBF_TC" ]]; then
  echo "ERROR: no sbpf-solana rustup toolchain found; run a solana program build once to install platform-tools" >&2
  exit 1
fi

( cd "$CRATE_DIR" && rustup run "$SBF_TC" cargo build --release --target sbpf-solana-solana ${FEATURES_ARG[@]+"${FEATURES_ARG[@]}"} )
mkdir -p "$CRATE_DIR/target/deploy"
cp -f "$CRATE_DIR/target/sbpf-solana-solana/release/${CRATE_NAME}.so" "$CRATE_DIR/target/deploy/${CRATE_NAME}.so"
echo "built $CRATE_DIR/target/deploy/${CRATE_NAME}.so"

#!/usr/bin/env bash
# Build a Solana SBF program.
#
# Tries the standard `cargo build-sbf` first — this works in CI and installs
# platform-tools on first run. It falls back to a direct rustup-run build ONLY
# when `cargo build-sbf` mis-parses `rustup toolchain list` (a known
# rustup-1.27 / agave-3.0.15 bug seen on some local setups, where the whole
# `name<TAB>path` line is passed to `rustup run`).
#
# Usage: ci/build-sbf.sh <crate-dir> <crate-name> [--features f1,f2]
set -euo pipefail
CRATE_DIR="$1"; CRATE_NAME="$2"; shift 2
FEATURES_ARG=()
if [[ "${1:-}" == "--features" ]]; then FEATURES_ARG=(--features "$2"); shift 2; fi

LOG="$(mktemp)"
if ( cd "$CRATE_DIR" && cargo build-sbf ${FEATURES_ARG[@]+"${FEATURES_ARG[@]}"} ) >"$LOG" 2>&1; then
  cat "$LOG"
elif grep -q "invalid toolchain name" "$LOG"; then
  echo "==> cargo build-sbf hit the rustup toolchain mis-parse; falling back to rustup-run" >&2
  SBF_TC="$(rustup toolchain list | grep -E 'sbpf-solana' | head -1 | awk '{print $1}')"
  if [[ -z "$SBF_TC" ]]; then
    echo "ERROR: no sbpf-solana rustup toolchain found (run 'cargo build-sbf' once to install platform-tools)" >&2
    exit 1
  fi
  ( cd "$CRATE_DIR" && rustup run "$SBF_TC" cargo build --release --target sbpf-solana-solana ${FEATURES_ARG[@]+"${FEATURES_ARG[@]}"} )
  mkdir -p "$CRATE_DIR/target/deploy"
  cp -f "$CRATE_DIR/target/sbpf-solana-solana/release/${CRATE_NAME}.so" "$CRATE_DIR/target/deploy/${CRATE_NAME}.so"
else
  cat "$LOG" >&2
  exit 1
fi
echo "built $CRATE_DIR/target/deploy/${CRATE_NAME}.so"

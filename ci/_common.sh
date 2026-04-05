#!/usr/bin/env bash

set -euo pipefail

RUST_MANIFESTS=(
  "contracts/pfda-amm/Cargo.toml"
  "contracts/pfda-amm-3/Cargo.toml"
  "contracts/axis-g3m/Cargo.toml"
  "contracts/axis-vault/Cargo.toml"
  "solana-tfmm-rs/Cargo.toml"
)

SBF_BUILD_MANIFESTS=(
  "contracts/pfda-amm/Cargo.toml"
  "contracts/pfda-amm-3/Cargo.toml"
  "contracts/axis-g3m/Cargo.toml"
  "contracts/axis-vault/Cargo.toml"
)

SBF_LOCAL_E2E_MANIFESTS=(
  "contracts/pfda-amm/Cargo.toml"
  "contracts/pfda-amm-3/Cargo.toml"
  "contracts/axis-g3m/Cargo.toml"
  "contracts/axis-vault/Cargo.toml"
)

TS_PROJECTS=(
  "scripts/tsconfig.json"
  "test/ab/tsconfig.json"
  "test/benchmark/tsconfig.json"
  "test/e2e/axis-g3m/tsconfig.json"
  "test/e2e/axis-vault/tsconfig.json"
  "test/e2e/pfda-amm-3/tsconfig.json"
  "test/e2e/pfda-amm-legacy/tsconfig.json"
)

SOLANA_BIN_DIR="${HOME}/.local/share/solana/install/active_release/bin"

ensure_solana_path() {
  export PATH="${SOLANA_BIN_DIR}:${PATH}"
}

run_rust_manifests() {
  local cmd=$1
  shift

  for manifest in "$@"; do
    echo "==> ${cmd} (${manifest})"
    cargo "${cmd}" --manifest-path "${manifest}" --all-targets
  done
}

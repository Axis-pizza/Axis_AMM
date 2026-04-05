#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"

ensure_solana_path

for manifest in "${SBF_LOCAL_E2E_MANIFESTS[@]}"; do
  echo "==> cargo build-sbf (${manifest})"
  cargo build-sbf --manifest-path "${manifest}"
done

mkdir -p "${HOME}/.config/solana"
solana-keygen new --force --no-bip39-passphrase --silent -o "${HOME}/.config/solana/id.json"

if [[ -f /tmp/solana-test-validator.pid ]]; then
  stale_pid="$(cat /tmp/solana-test-validator.pid || true)"
  if [[ -n "${stale_pid}" ]]; then
    kill "${stale_pid}" >/dev/null 2>&1 || true
  fi
  rm -f /tmp/solana-test-validator.pid
fi

solana-test-validator \
  --reset \
  --ledger /tmp/solana-ci-ledger \
  --bpf-program 5BKDTDQdX7vFdDooVXZeKicu7S3yX2JY5e3rmASib5pY contracts/pfda-amm/target/deploy/pfda_amm.so \
  --bpf-program DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf contracts/pfda-amm-3/target/deploy/pfda_amm_3.so \
  --bpf-program 65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi contracts/axis-g3m/target/deploy/axis_g3m.so \
  --bpf-program DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy contracts/axis-vault/target/deploy/axis_vault.so \
  > /tmp/solana-test-validator.log 2>&1 &
validator_pid=$!

echo "${validator_pid}" > /tmp/solana-test-validator.pid

export SOLANA_URL=http://localhost:8899
solana config set --url localhost
until solana -u localhost cluster-version >/dev/null 2>&1; do
  sleep 1
done

solana -u localhost airdrop 2 || echo "Airdrop unavailable on this CLI/runtime; continuing with existing local balance."

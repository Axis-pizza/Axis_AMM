#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"

ensure_solana_path

for manifest in "${SBF_LOCAL_E2E_MANIFESTS[@]}"; do
  # axis-vault: enable the local-only feature that zeroes PROTOCOL_TREASURY
  # so the CreateEtf gate stays inert. The verifiable mainnet build never
  # runs this script, so the real Squads vault key still ships to mainnet.
  if [[ "${manifest}" == "contracts/axis-vault/Cargo.toml" ]]; then
    echo "==> cargo build-sbf (${manifest}) [features: e2e-disable-treasury-gate]"
    cargo build-sbf --manifest-path "${manifest}" --features e2e-disable-treasury-gate
  else
    echo "==> cargo build-sbf (${manifest})"
    cargo build-sbf --manifest-path "${manifest}"
  fi
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

# axis-vault v1.1 CreateEtf CPIs into Metaplex Token Metadata
# (`metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`). The bare local
# validator doesn't ship Metaplex, so we dump the program from
# mainnet once and load it via --bpf-program. We dump rather than
# --clone-upgradeable-program because dumps are cacheable in the
# CI runner's /tmp and don't add mainnet RPC dependency to every
# validator restart.
mpl_so_path=/tmp/mpl_token_metadata.so
if [[ ! -s "${mpl_so_path}" ]]; then
  echo "==> dumping Metaplex Token Metadata from mainnet → ${mpl_so_path}"
  solana program dump \
    -u https://api.mainnet-beta.solana.com \
    metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s \
    "${mpl_so_path}"
fi

solana-test-validator \
  --reset \
  --ledger /tmp/solana-ci-ledger \
  --bpf-program 5BKDTDQdX7vFdDooVXZeKicu7S3yX2JY5e3rmASib5pY contracts/pfda-amm/target/deploy/pfda_amm.so \
  --bpf-program DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf contracts/pfda-amm-3/target/deploy/pfda_amm_3.so \
  --bpf-program 65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi contracts/axis-g3m/target/deploy/axis_g3m.so \
  --bpf-program DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy contracts/axis-vault/target/deploy/axis_vault.so \
  --bpf-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s "${mpl_so_path}" \
  > /tmp/solana-test-validator.log 2>&1 &
validator_pid=$!

echo "${validator_pid}" > /tmp/solana-test-validator.pid

export SOLANA_URL=http://localhost:8899
solana config set --url localhost
until solana -u localhost cluster-version >/dev/null 2>&1; do
  sleep 1
done

solana -u localhost airdrop 2 || echo "Airdrop unavailable on this CLI/runtime; continuing with existing local balance."

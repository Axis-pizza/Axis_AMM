#!/usr/bin/env bash

set -euo pipefail

MIN_BALANCE_SOL="${1:-2}"
MAX_AIRDROP_ATTEMPTS="${2:-4}"
AIRDROP_SOL_PER_ATTEMPT="${3:-2}"

to_lamports() {
  node -e '
    const sol = Number(process.argv[1]);
    if (!Number.isFinite(sol) || sol <= 0) process.exit(2);
    process.stdout.write(String(Math.ceil(sol * 1_000_000_000)));
  ' "$1"
}

read_balance_lamports() {
  local raw
  raw="$(solana -u devnet balance --lamports | awk "{print \$1}")"
  if [[ "${raw}" =~ ^[0-9]+$ ]]; then
    echo "${raw}"
    return 0
  fi
  echo "Unable to parse lamports from: ${raw}" >&2
  return 1
}

min_balance_lamports="$(to_lamports "${MIN_BALANCE_SOL}")"
wallet_address="$(solana -u devnet address)"
current_lamports="$(read_balance_lamports)"

if (( current_lamports >= min_balance_lamports )); then
  echo "Devnet wallet ${wallet_address} has sufficient balance (${current_lamports} lamports)."
  exit 0
fi

echo "Devnet wallet ${wallet_address} is underfunded (${current_lamports} lamports)."
echo "Target minimum: ${min_balance_lamports} lamports (${MIN_BALANCE_SOL} SOL)."

for ((attempt=1; attempt<=MAX_AIRDROP_ATTEMPTS && current_lamports<min_balance_lamports; attempt++)); do
  echo "Airdrop attempt ${attempt}/${MAX_AIRDROP_ATTEMPTS}: ${AIRDROP_SOL_PER_ATTEMPT} SOL..."
  solana -u devnet airdrop "${AIRDROP_SOL_PER_ATTEMPT}" || true
  sleep 2
  current_lamports="$(read_balance_lamports)"
done

if (( current_lamports < min_balance_lamports )); then
  echo "Insufficient devnet balance after airdrop attempts. Current: ${current_lamports} lamports." >&2
  exit 1
fi

echo "Devnet wallet funded successfully (${current_lamports} lamports)."

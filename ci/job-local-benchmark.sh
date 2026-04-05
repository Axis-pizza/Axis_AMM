#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"

ensure_solana_path

cleanup() {
  bash ci/e2e-local-cleanup.sh
}
trap cleanup EXIT

bash ci/e2e-local-prepare.sh
bun run bench:ab

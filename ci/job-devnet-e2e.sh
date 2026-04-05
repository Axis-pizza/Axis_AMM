#!/usr/bin/env bash

set -euo pipefail

bun run e2e:axis-g3m:devnet
bun run e2e:pfda-amm-3:oracle-bid:devnet
bun run test:pfda-amm-3:o1-proof
bun run test:pfda-amm-3:imbalanced
bun run test:pfda-amm-3:multiuser
bun run test:pfda-amm-legacy:oracle
bun run test:pfda-amm-legacy:jito-bid

#!/usr/bin/env bash

set -euo pipefail

bunx jest --config jest.config.cjs --runInBand
echo "--- bun frontend tests ---"
bun test test/frontend/*.test.ts

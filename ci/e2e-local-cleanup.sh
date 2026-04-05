#!/usr/bin/env bash

set -euo pipefail

pid_file="/tmp/solana-test-validator.pid"

if [[ -f "${pid_file}" ]]; then
  validator_pid="$(cat "${pid_file}" || true)"
  if [[ -n "${validator_pid}" ]]; then
    kill "${validator_pid}" >/dev/null 2>&1 || true
  fi
  rm -f "${pid_file}"
fi

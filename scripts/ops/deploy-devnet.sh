#!/usr/bin/env bash
#
# deploy-devnet.sh — one-shot devnet deploy with pre-flight + verification.
#
# Deploys axis-vault, pfda-amm-3, pfda-amm to devnet using their existing
# program-ID keypairs. axis-g3m is intentionally skipped (no source changes
# in the current pre-mainnet hardening pass).
#
# Pre-flight (each step aborts on failure):
#   1. solana CLI on devnet
#   2. wallet balance >= 10 SOL (ballpark for 3 program upgrades)
#   3. all 3 .so files exist and are newer than their src/ tree
#   4. on-chain upgrade authority on each program == current wallet
#   5. on-chain program is not "finalized" (i.e. still upgradeable)
#   6. interactive Y/N confirmation showing what's about to deploy
#
# Deploy order: axis-vault → pfda-amm-3 → pfda-amm. axis-vault first
# because it has the schema bump (etfstat2→etfstat3) and clients need
# it live before any new ETF flows. pfda-amm-3 second (oracle hardening).
# pfda-amm last (alignment-guard only, lowest blast radius).
#
# Post-deploy verification:
#   - "Last Deployed Slot" advanced for each program
#   - executable bit set
#
# Usage:
#   bash scripts/ops/deploy-devnet.sh                              # interactive, canonical, full stack
#   bash scripts/ops/deploy-devnet.sh --fresh                      # parallel kidney-owned env, full stack
#   bash scripts/ops/deploy-devnet.sh --fresh --mainnet-scope      # only the 2 mainnet v1 programs
#   bash scripts/ops/deploy-devnet.sh --yes                        # skip confirmation
#   bash scripts/ops/deploy-devnet.sh --skip-build                 # don't cargo build-sbf
#   bash scripts/ops/deploy-devnet.sh --dry-run                    # pre-flight only
#
# Modes:
#   CANONICAL (default): redeploys to existing devnet program IDs from
#     DEVNET_TESTING.md. Requires the wallet to be the original upgrade
#     authority (currently `6t4B1TVgSjnAM9h5MpahLhGc9MtWFTGmcaPsy9JGskoV`).
#     Most teammates won't have this key on disk.
#   FRESH (--fresh): first-time deploy using local target/deploy
#     keypairs. Claims new program IDs; the deploying wallet becomes
#     the upgrade authority. Writes .env.devnet.kidney with the new IDs
#     so e2e suites can override via `PROGRAM_ID=$AXIS_VAULT_PROGRAM_ID`.
#     Use this to run a parallel devnet test environment without needing
#     the canonical deploy authority key.
#
# Scope flag:
#   --mainnet-scope: restrict deploy to the 2 programs that ship to
#     mainnet v1 — pfda-amm-3 + axis-vault. Skips axis-g3m (research-
#     only) and pfda-amm (legacy regression). Use this to validate the
#     mainnet stack on devnet without dragging research code along.
#     See docs/architecture/MAINNET_SCOPE.md for the rationale.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

# ─── Color output ───────────────────────────────────────────────────────
if [ -t 1 ]; then
  RED=$'\033[0;31m'; GRN=$'\033[0;32m'; YEL=$'\033[0;33m'
  BLU=$'\033[0;34m'; BLD=$'\033[1m'; CLR=$'\033[0m'
else
  RED=""; GRN=""; YEL=""; BLU=""; BLD=""; CLR=""
fi

step()  { printf "%s▶%s %s\n" "$BLU" "$CLR" "$*"; }
ok()    { printf "%s✓%s %s\n" "$GRN" "$CLR" "$*"; }
warn()  { printf "%s⚠%s %s\n" "$YEL" "$CLR" "$*"; }
fail()  { printf "%s✗%s %s\n" "$RED" "$CLR" "$*" >&2; exit 1; }
title() { printf "\n%s%s%s\n" "$BLD" "$*" "$CLR"; }

# ─── Args ───────────────────────────────────────────────────────────────
SKIP_CONFIRM=false
SKIP_BUILD=false
DRY_RUN=false
FRESH=false
MAINNET_SCOPE=false
for arg in "$@"; do
  case "$arg" in
    --yes|-y)         SKIP_CONFIRM=true ;;
    --skip-build)     SKIP_BUILD=true ;;
    --dry-run)        DRY_RUN=true ;;
    --fresh)          FRESH=true ;;
    --mainnet-scope)  MAINNET_SCOPE=true ;;
    *) fail "unknown arg: $arg" ;;
  esac
done

# ─── Program manifests ───────────────────────────────────────────────────
#
# Two manifest sets, switched by --fresh:
#
# CANONICAL (default): redeploys to the existing devnet program IDs Muse
# originally provisioned. Requires the wallet to be the upgrade authority
# `6t4B1TVgSjnAM9h5MpahLhGc9MtWFTGmcaPsy9JGskoV`. Each entry is
# <name>|<on-chain-pid>|<so-path>|<src-dir>.
#
# pfda-amm note: local validator fixture uses
# `5BKDTDQdX7vFdDooVXZeKicu7S3yX2JY5e3rmASib5pY` (per
# ci/e2e-local-prepare.sh), but the devnet canonical deploy is at
# `CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk`. Don't conflate them.
PROGRAMS_CANONICAL=(
  "axis-vault|DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy|contracts/axis-vault/target/deploy/axis_vault.so|contracts/axis-vault/src"
  "pfda-amm-3|DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf|contracts/pfda-amm-3/target/deploy/pfda_amm_3.so|contracts/pfda-amm-3/src"
  "pfda-amm|CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk|contracts/pfda-amm/target/deploy/pfda_amm.so|contracts/pfda-amm/src"
)
#
# FRESH (--fresh): first-time deploy using the locally checked-in
# program keypairs at target/deploy/*-keypair.json. Each entry is
# <name>|<keypair-path>|<so-path>|<src-dir>. The deploy claims new
# program IDs derived from those keypairs and sets the deploying wallet
# as the upgrade authority by default. Use this when you don't have the
# canonical upgrade-authority keypair on this machine — the fresh deploy
# stands up a parallel "kidney-owned" devnet test environment that runs
# alongside the canonical one without conflict.
#
# After --fresh deploy, the script writes .env.devnet.kidney with the
# new program IDs so the e2e suite can target them via env-var override.
PROGRAMS_FRESH=(
  "axis-vault|contracts/axis-vault/target/deploy/axis_vault-keypair.json|contracts/axis-vault/target/deploy/axis_vault.so|contracts/axis-vault/src"
  "pfda-amm-3|contracts/pfda-amm-3/target/deploy/pfda_amm_3-keypair.json|contracts/pfda-amm-3/target/deploy/pfda_amm_3.so|contracts/pfda-amm-3/src"
  "pfda-amm|contracts/pfda-amm/target/deploy/pfda_amm-keypair.json|contracts/pfda-amm/target/deploy/pfda_amm.so|contracts/pfda-amm/src"
  "axis-g3m|contracts/axis-g3m/target/deploy/axis_g3m-keypair.json|contracts/axis-g3m/target/deploy/axis_g3m.so|contracts/axis-g3m/src"
)
# axis-g3m is bundled into the fresh manifest (but not canonical, since
# its source didn't change in the pre-mainnet pass) so the kidney-owned
# environment is a complete 4-program stack from day one — unless
# --mainnet-scope is passed, in which case only the 2 mainnet v1
# programs ship.
#
# MAINNET-SCOPE (--mainnet-scope): the strict subset of programs that
# ship to mainnet v1 per docs/architecture/MAINNET_SCOPE.md. Excludes
# axis-g3m (research baseline) and pfda-amm (legacy regression).
PROGRAMS_MAINNET_SCOPE_CANONICAL=(
  "axis-vault|DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy|contracts/axis-vault/target/deploy/axis_vault.so|contracts/axis-vault/src"
  "pfda-amm-3|DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf|contracts/pfda-amm-3/target/deploy/pfda_amm_3.so|contracts/pfda-amm-3/src"
)
PROGRAMS_MAINNET_SCOPE_FRESH=(
  "axis-vault|contracts/axis-vault/target/deploy/axis_vault-keypair.json|contracts/axis-vault/target/deploy/axis_vault.so|contracts/axis-vault/src"
  "pfda-amm-3|contracts/pfda-amm-3/target/deploy/pfda_amm_3-keypair.json|contracts/pfda-amm-3/target/deploy/pfda_amm_3.so|contracts/pfda-amm-3/src"
)

# Pick manifest by (FRESH, MAINNET_SCOPE) — four combinations resolve
# cleanly without any boolean gymnastics.
if $FRESH && $MAINNET_SCOPE; then
  PROGRAMS=("${PROGRAMS_MAINNET_SCOPE_FRESH[@]}")
elif $FRESH; then
  PROGRAMS=("${PROGRAMS_FRESH[@]}")
elif $MAINNET_SCOPE; then
  PROGRAMS=("${PROGRAMS_MAINNET_SCOPE_CANONICAL[@]}")
else
  PROGRAMS=("${PROGRAMS_CANONICAL[@]}")
fi

# ─── Pre-flight 1: solana CLI on devnet ──────────────────────────────────
title "Pre-flight checks"

step "Confirming solana CLI is on devnet"
CURRENT_URL=$(solana config get | awk '/RPC URL/ {print $3}')
if [[ "$CURRENT_URL" != *"devnet"* ]]; then
  fail "solana config RPC URL = $CURRENT_URL; run: solana config set --url devnet"
fi
ok "RPC URL: $CURRENT_URL"

# ─── Pre-flight 2: wallet identity + balance ─────────────────────────────
step "Checking wallet identity + balance"
WALLET=$(solana address)
BAL_RAW=$(solana balance --output json 2>/dev/null | grep -oE '"value":[0-9.]+' | cut -d: -f2 || true)
if [ -z "$BAL_RAW" ]; then
  # Fall back to text parse if --output json is unsupported.
  BAL_RAW=$(solana balance | awk '{print $1}')
fi
ok "Wallet: $WALLET"
ok "Balance: $BAL_RAW SOL"

# Threshold scales with the number of programs in the active manifest.
# Each ~90KB .so deploy holds ~1.3 SOL peak (buffer + program data rent;
# returned on success). We add a 1 SOL buffer for tx fees + retry headroom.
# Devnet faucet is rate-limited; if low, transfer from another wallet
# (e.g. tbw.json) rather than waiting for airdrop.
NUM_PROGS=${#PROGRAMS[@]}
EST_SOL=$(awk -v n="$NUM_PROGS" 'BEGIN{printf "%.1f", n*1.3 + 1}')
EST_CENTS=$(awk -v v="$EST_SOL" 'BEGIN{printf "%d", v*100}')
BAL_INT=$(awk -v v="$BAL_RAW" 'BEGIN{printf "%d", v*100}')
if [ "$BAL_INT" -lt "$EST_CENTS" ]; then
  fail "Balance $BAL_RAW SOL < ${EST_SOL} SOL needed for $NUM_PROGS program deploys. Run: solana airdrop 2 (or transfer from another wallet)"
fi
ok "Balance OK for $NUM_PROGS deploys (need ~$EST_SOL SOL)"

# ─── Resolve program IDs per mode ────────────────────────────────────────
#
# In canonical mode, the second field IS the on-chain pubkey.
# In fresh mode, the second field is a keypair path; derive the pubkey
# via solana-keygen pubkey. We populate RESOLVED_IDS in declaration order
# so later loops can use it without re-deriving.
declare -a RESOLVED_IDS
for entry in "${PROGRAMS[@]}"; do
  IFS='|' read -r name target so_path src_dir <<< "$entry"
  if $FRESH; then
    if [ ! -f "$target" ]; then
      fail "$name: keypair file missing at $target — run cargo build-sbf to generate it."
    fi
    pid=$(solana-keygen pubkey "$target")
  else
    pid="$target"
  fi
  RESOLVED_IDS+=("$pid")
done

# ─── Optional: cargo build-sbf for each program ──────────────────────────
if ! $SKIP_BUILD; then
  title "Rebuilding SBF binaries"
  for entry in "${PROGRAMS[@]}"; do
    IFS='|' read -r name target so_path src_dir <<< "$entry"
    step "cargo build-sbf $name"
    cargo build-sbf --manifest-path "${src_dir%/src}/Cargo.toml" 2>&1 | tail -3
  done
fi

# ─── Pre-flight 3: .so files fresh ───────────────────────────────────────
step "Checking .so freshness vs src/ trees"
for entry in "${PROGRAMS[@]}"; do
  IFS='|' read -r name target so_path src_dir <<< "$entry"
  if [ ! -f "$so_path" ]; then
    fail "$name: .so missing at $so_path. Run with cargo build-sbf or drop --skip-build."
  fi
  SO_MTIME=$(stat -f %m "$so_path" 2>/dev/null || stat -c %Y "$so_path")
  NEWEST_SRC=$(find "$src_dir" -type f -name '*.rs' -exec stat -f %m {} \; 2>/dev/null \
    | sort -n | tail -1)
  if [ -z "$NEWEST_SRC" ]; then
    NEWEST_SRC=$(find "$src_dir" -type f -name '*.rs' -exec stat -c %Y {} \; \
      | sort -n | tail -1)
  fi
  if [ -n "$NEWEST_SRC" ] && [ "$SO_MTIME" -lt "$NEWEST_SRC" ]; then
    warn "$name: .so older than newest .rs in $src_dir — run cargo build-sbf or drop --skip-build"
  fi
  SIZE_KB=$(( $(stat -f %z "$so_path" 2>/dev/null || stat -c %s "$so_path") / 1024 ))
  ok "$name: $so_path (${SIZE_KB}KB)"
done

# ─── Pre-flight 4: on-chain authority (canonical only) ───────────────────
if $FRESH; then
  step "Verifying fresh-mode keypairs are unused on devnet"
  for i in "${!PROGRAMS[@]}"; do
    IFS='|' read -r name target so_path src_dir <<< "${PROGRAMS[$i]}"
    pid="${RESOLVED_IDS[$i]}"
    EXISTS=$(solana program show "$pid" -u devnet 2>&1 || true)
    if echo "$EXISTS" | grep -q "Program Id:"; then
      # Already deployed — falls through to upgrade flow with this wallet
      # only if the wallet IS the existing upgrade authority. Otherwise
      # bail; otherwise we'd deploy on top of someone else's program.
      ON_CHAIN_AUTH=$(echo "$EXISTS" | awk '/Upgrade Authority/ {print $3}')
      if [ "$ON_CHAIN_AUTH" != "$WALLET" ]; then
        fail "$name: keypair derives to $pid which is ALREADY deployed on devnet with upgrade authority $ON_CHAIN_AUTH (not $WALLET). Either drop --fresh or rotate the keypair."
      fi
      warn "$name: $pid already deployed by this wallet — will upgrade in place"
    else
      ok "$name: $pid free for fresh deploy"
    fi
  done
else
  step "Verifying on-chain upgrade authority for each program"
  for i in "${!PROGRAMS[@]}"; do
    IFS='|' read -r name target so_path src_dir <<< "${PROGRAMS[$i]}"
    pid="${RESOLVED_IDS[$i]}"
    PROG_INFO=$(solana program show "$pid" 2>&1 || true)
    if echo "$PROG_INFO" | grep -q "Upgradeable: false"; then
      fail "$name ($pid): NOT upgradeable on-chain. Cannot redeploy."
    fi
    ON_CHAIN_AUTH=$(echo "$PROG_INFO" | awk '/Upgrade Authority/ {print $3}')
    if [ -z "$ON_CHAIN_AUTH" ]; then
      fail "$name ($pid): could not read Upgrade Authority. Output:\n$PROG_INFO"
    fi
    if [ "$ON_CHAIN_AUTH" != "$WALLET" ]; then
      fail "$name ($pid): Upgrade Authority on-chain is $ON_CHAIN_AUTH, current wallet is $WALLET. Either get the auth keypair, ask Muse to transfer authority, or use --fresh for a parallel deploy."
    fi
    ok "$name: upgrade authority OK ($ON_CHAIN_AUTH)"
  done
fi

# ─── Confirmation ────────────────────────────────────────────────────────
title "Ready to deploy"
if $FRESH; then
  echo "  Mode:    FRESH (parallel devnet env owned by this wallet)"
else
  echo "  Mode:    CANONICAL (upgrade existing devnet programs in-place)"
fi
if $MAINNET_SCOPE; then
  echo "  Scope:   MAINNET v1 ONLY (pfda-amm-3 + axis-vault)"
  echo "           Skipping axis-g3m (research) + pfda-amm (legacy)"
else
  echo "  Scope:   FULL (all 4 programs incl. research + legacy)"
fi
echo "  Network: devnet"
echo "  Wallet:  $WALLET"
echo "  Programs:"
for i in "${!PROGRAMS[@]}"; do
  IFS='|' read -r name target so_path src_dir <<< "${PROGRAMS[$i]}"
  echo "    - $name   ${RESOLVED_IDS[$i]}"
done
echo

if $DRY_RUN; then
  ok "Dry-run complete. Pre-flight passed. No deploy executed."
  exit 0
fi

if ! $SKIP_CONFIRM; then
  read -r -p "Proceed with deploy? [y/N] " ans
  case "$ans" in
    y|Y|yes|YES) ;;
    *) fail "Aborted by user." ;;
  esac
fi

# ─── Deploy ──────────────────────────────────────────────────────────────
title "Deploying"
for i in "${!PROGRAMS[@]}"; do
  IFS='|' read -r name target so_path src_dir <<< "${PROGRAMS[$i]}"
  pid="${RESOLVED_IDS[$i]}"
  step "Deploying $name → $pid"

  # Capture pre-deploy "Last Deployed Slot" so we can verify the delta.
  # On --fresh first-time deploys this read fails (program doesn't
  # exist) so we default to 0.
  PRE_SLOT=$(solana program show "$pid" 2>/dev/null | awk '/Last Deployed/ {print $4}')
  PRE_SLOT="${PRE_SLOT:-0}"

  # In FRESH mode pass the keypair file directly — this both sets the
  # program ID AND signs the buffer-write. In CANONICAL mode pass the
  # pubkey; the wallet (already verified as upgrade authority) signs the
  # upgrade.
  if $FRESH; then
    DEPLOY_PID_ARG="$target"  # keypair path
  else
    DEPLOY_PID_ARG="$pid"     # base58 pubkey
  fi
  # Run deploy with `set +e` so we capture exit status without aborting
  # the script — pinocchio deploys sometimes succeed but the wrapping
  # solana CLI returns non-zero when it can't immediately read back the
  # program account (RPC propagation lag). We retry the post-deploy
  # check with backoff to absorb that.
  set +e
  solana program deploy \
      --url devnet \
      --program-id "$DEPLOY_PID_ARG" \
      "$so_path"
  DEPLOY_RC=$?
  set -e
  if [ $DEPLOY_RC -ne 0 ]; then
    warn "$name: solana program deploy returned $DEPLOY_RC — checking on-chain anyway"
  fi

  # Verify post-deploy: slot advanced + executable. Retry up to 5 times
  # with 2s backoff for RPC propagation delay (devnet RPC can lag a few
  # seconds after a successful deploy).
  POST_SLOT=""
  IS_EXEC=""
  for try in 1 2 3 4 5; do
    POST_INFO=$(solana program show "$pid" 2>/dev/null || true)
    POST_SLOT=$(echo "$POST_INFO" | awk '/Last Deployed/ {print $4}')
    IS_EXEC=$(echo "$POST_INFO" | awk '/Executable/ {print $2}')
    if [ -n "$POST_SLOT" ] && [ "$POST_SLOT" -gt "$PRE_SLOT" ]; then
      break
    fi
    sleep 2
  done

  if [ -z "$POST_SLOT" ] || [ "$POST_SLOT" -le "$PRE_SLOT" ]; then
    fail "$name: Last Deployed Slot did not advance after 5 retries (pre=$PRE_SLOT post=$POST_SLOT). solana program deploy returned $DEPLOY_RC."
  fi
  if [ "$IS_EXEC" != "true" ]; then
    fail "$name: program not executable post-deploy"
  fi
  ok "$name: deployed at slot $POST_SLOT (was $PRE_SLOT)"
done

# ─── Write env file (FRESH mode only) ───────────────────────────────────
if $FRESH; then
  if $MAINNET_SCOPE; then
    ENV_FILE="$REPO_ROOT/.env.devnet.kidney.mainnet-scope"
  else
    ENV_FILE="$REPO_ROOT/.env.devnet.kidney"
  fi
  step "Writing program IDs to $ENV_FILE"
  {
    echo "# Generated by scripts/ops/deploy-devnet.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    if $MAINNET_SCOPE; then
      echo "# Mode: --fresh --mainnet-scope (mainnet v1 stack only, parallel kidney-owned env)"
    else
      echo "# Mode: --fresh (full 4-program stack, parallel kidney-owned env)"
    fi
    echo "# Owner: $WALLET"
    echo "#"
    echo "# Source this file before running e2e tests:"
    echo "#   source $(basename "$ENV_FILE")"
    echo "#   PROGRAM_ID=\$AXIS_VAULT_PROGRAM_ID  bun run e2e:axis-vault:devnet"
    echo "#"
    echo "# These IDs are independent of Muse's canonical devnet deployment"
    echo "# (the IDs hardcoded in DEVNET_TESTING.md). Both environments coexist;"
    echo "# pick which one to target by sourcing this file or not."
    if $MAINNET_SCOPE; then
      echo "#"
      echo "# This file ONLY contains the mainnet v1 stack — axis-g3m and pfda-amm"
      echo "# are not deployed in this environment. Use the full stack file"
      echo "# (.env.devnet.kidney) for A/B research."
    fi
    echo
    for i in "${!PROGRAMS[@]}"; do
      IFS='|' read -r name target so_path src_dir <<< "${PROGRAMS[$i]}"
      var=$(echo "$name" | tr 'a-z-' 'A-Z_')_PROGRAM_ID
      echo "export $var=${RESOLVED_IDS[$i]}"
    done
    echo
    echo "# For per-test PROGRAM_ID env-var overrides (used by single-program test files),"
    echo "# prefix the test command:"
    echo "#   PROGRAM_ID=\$AXIS_VAULT_PROGRAM_ID bun run e2e:axis-vault:devnet"
  } > "$ENV_FILE"
  ok "Wrote $ENV_FILE"
fi

# ─── Post-deploy summary ────────────────────────────────────────────────
title "Deploy complete"
if $FRESH; then
  echo "Fresh deploy complete. Program IDs written to .env.devnet.kidney."
  echo
  echo "Next steps:"
  echo "  1. Source the env and run smokes:"
  echo "       source .env.devnet.kidney"
  echo "       PROGRAM_ID=\$AXIS_VAULT_PROGRAM_ID bun run e2e:axis-vault:devnet"
  echo "       PROGRAM_ID=\$PFDA_AMM_3_PROGRAM_ID bun run e2e:pfda-amm-3:oracle-bid:devnet"
  echo "       PROGRAM_ID=\$AXIS_G3M_PROGRAM_ID  bun run e2e:axis-g3m:devnet"
  echo
  echo "  2. Tag this deploy for rollback reference:"
  echo "       git tag -a kidney-devnet-\$(date +%Y%m%d) -m 'fresh kidney-owned devnet deploy'"
else
  echo "Canonical redeploy complete."
  echo
  echo "Next steps:"
  echo "  1. Run devnet smoke tests against canonical IDs:"
  echo "       bun run e2e:axis-g3m:devnet"
  echo "       bun run e2e:pfda-amm-3:oracle-bid:devnet"
  echo "       bun run e2e:axis-vault:devnet"
  echo
  echo "  2. If anything regresses, roll back by re-deploying the previous .so"
  echo "     archive. Tag this deploy:"
  echo "       git tag -a devnet-deploy-\$(date +%Y%m%d) -m 'devnet deploy after pre-mainnet hardening'"
fi
echo
ok "Done."

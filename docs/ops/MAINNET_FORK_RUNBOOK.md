# Mainnet-fork Jupiter route test

How to drive a real Jupiter V6 route through `axis-g3m
RebalanceViaJupiter` (disc=4) on a forked validator. Closes the
testable side of #61 item 5; what's still gated on you is provisioning
the funded ALT and refreshing the fixture if the route topology
shifts.

## What we have in tree

- `scripts/ops/fetch-jupiter-quote.ts` — calls Jupiter's lite-api,
  saves quote + swap-instructions to `test/fixtures/jupiter/<label>.json`
- `test/fixtures/jupiter/sol-usdc-100m.json` — recorded fixture for
  100M lamports SOL → USDC, slippage 50 bps. Refresh whenever the
  route plan changes substantially (e.g. Jupiter adds a new AMM
  source that becomes the dominant leg).
- `scripts/ops/dump-jupiter-fixture-accounts.sh` — given a fixture
  JSON, dumps every referenced account via `solana account` with
  RPC retry/fallback, writes a `clone-args.txt` ready to paste into
  the validator command line.

## Refresh the fixture

```bash
# Default pair (SOL → USDC, 100M lamports, 50 bps slippage)
bun scripts/ops/fetch-jupiter-quote.ts --label sol-usdc-100m

# Custom pair / amount
bun scripts/ops/fetch-jupiter-quote.ts \
  --in  So11111111111111111111111111111111111111112 \
  --out EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
  --amount 1000000000 \
  --slippage 100 \
  --label sol-usdc-1b
```

Lite-api has no auth requirement and a soft rate limit fine for CI.
Free tier confirms #61 item 5(a) — no Jupiter Quote API key needed.

## Dump the route accounts (one-time)

The `--clone --url mainnet-beta` approach in earlier workflows used to
fail intermittently when mainnet RPC blipped. Pre-dumping accounts is
deterministic.

```bash
scripts/ops/dump-jupiter-fixture-accounts.sh \
  test/fixtures/jupiter/sol-usdc-100m.json \
  test/fixtures/jupiter/accounts/sol-usdc-100m
```

Produces:

```
test/fixtures/jupiter/accounts/sol-usdc-100m/
  <pubkey1>.json
  <pubkey2>.json
  ...
  clone-args.txt   ← every account as `--account <pubkey> <path>`
```

Commit the dumped account JSON files alongside the fixture so CI never
touches mainnet RPC at validator-launch time.

## Provision an ALT (manual, one-time)

This is the part that needs your wallet. Jupiter routes use ALTs to
fit the account list inside the versioned-tx envelope; the ALT pubkeys
are in the fixture's `swap.addressLookupTableAddresses[*]`.

For a forked validator the ALT account itself can be cloned (the
dump-accounts script already does that — ALT pubkeys are included
under `addressLookupTableAddresses`). What you need to provision:

- A funded test wallet (SOL on whatever cluster the test runs on)
- The wallet keypair stored at `~/.config/solana/id.json` (matches
  `loadPayer()` in the existing e2e scripts)

No new on-chain ALT creation needed — we ride the existing mainnet
ALTs that Jupiter's response references.

## Run the test

```bash
# 1. Make sure all programs are built
cargo build-sbf --manifest-path contracts/axis-g3m/Cargo.toml
cargo build-sbf --manifest-path contracts/pfda-amm-3/Cargo.toml

# 2. Make sure Jupiter V6 is dumped (fallback for the workflow)
mkdir -p contracts/axis-g3m/fixtures
solana program dump -u https://api.mainnet-beta.solana.com \
  JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 \
  contracts/axis-g3m/fixtures/jupiter_v6.so

# 3. Boot the validator with everything pre-loaded
solana-test-validator --reset --ledger /tmp/fork-ledger \
  --bpf-program 65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi contracts/axis-g3m/target/deploy/axis_g3m.so \
  --bpf-program DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf contracts/pfda-amm-3/target/deploy/pfda_amm_3.so \
  --bpf-program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 contracts/axis-g3m/fixtures/jupiter_v6.so \
  $(cat test/fixtures/jupiter/accounts/sol-usdc-100m/clone-args.txt) \
  > /tmp/fork-validator.log 2>&1 &

# 4. Run the e2e
JUPITER_FIXTURE=test/fixtures/jupiter/sol-usdc-100m.json \
RPC_URL=http://localhost:8899 \
  bun test/e2e/axis-g3m/axis-g3m.jupiter-fork.e2e.ts
```

The existing test currently uses attestation-mode rebalance (disc=3,
no real Jupiter CPI). To exercise disc=4 RebalanceViaJupiter with the
recorded route, the e2e needs to be extended to:

1. Read `JUPITER_FIXTURE` and parse `swap.swapInstruction`
2. Rewrite the user-position account to the test wallet's pubkey
   (the fixture uses a placeholder)
3. Build a `disc=4` ix data: `[3] [u32 LE jupiter_data_len]
   [swapInstruction.data bytes]`
4. Pass the fixture's `swap.swapInstruction.accounts` (plus the
   axis-g3m fixed prefix: authority, pool, jupiter_program, vaults)
   as the AccountMeta list

Tracked as a code follow-up — the runbook + fixture + dump tooling
above is what's gating it. Once the e2e extension lands, wire it into
`.github/workflows/main-report.yml` between the existing E2E and
"Stop forked validator" steps.

## When fixture refresh is needed

- Jupiter adds a new dominant AMM source for the recorded pair (you'll
  see it as a different `routePlan[0].swapInfo.label` in a fresh quote)
- The `addressLookupTableAddresses` list changes
- An ALT version bump retires an account that's referenced

Fixture rot doesn't break the test catastrophically — the dumped
accounts stay valid until a mainnet ALT version bump. Refresh
quarterly or on observed test drift, whichever comes first.

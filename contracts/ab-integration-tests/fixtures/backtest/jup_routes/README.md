# Jupiter Route Fixtures (`jup_routes/`)

This directory holds **recorded Jupiter swap routes** and the associated mainnet
account snapshots needed to replay them deterministically inside LiteSVM.

## What is stored here

Each `route_<n>.json` file represents a single Jupiter swap route captured from
the mainnet Jupiter v6 API.  The JSON schema is:

```json
{
  "label":      "SOL->BONK 0.01 SOL",
  "in_mint":    "<base58 pubkey>",
  "out_mint":   "<base58 pubkey>",
  "in_amount":  10000000,
  "out_amount": 123456789,
  "swap_data":  "<base64-encoded instruction data>",
  "accounts": [
    { "pubkey": "<base58>", "is_signer": false, "is_writable": true },
    ...
  ],
  "address_lookup_tables": ["<base58>", ...]
}
```

Fields mirror `JupiterRoute` in `src/helpers/mainnet_fork.rs` so the replay
path in `stage2_exec_quality` can deserialise them with zero transformation.

## How to populate this directory

Run the offline refresher **with a working mainnet RPC**:

```sh
MAINNET_RPC_URL=https://api.mainnet-beta.solana.com \
  cargo test --test refresh_backtest_fixtures -- --ignored --nocapture
```

The refresher fetches a small grid of SOL→BONK routes (sizes: 0.01, 0.1, 1 SOL)
from `api.jup.ag`, serialises each route, and writes `route_0.json`,
`route_1.json`, `route_2.json` here.

## CI behaviour

When this directory is **empty** (no `*.json` files), the `stage2_exec_quality`
function in `backtest.rs` returns an empty `Vec` and prints a skip line:

```
[backtest] stage-2 skipped: no recorded routes in .../fixtures/backtest/jup_routes
```

The report will contain a placeholder section instead of a comparison table.
No network calls are ever made during a normal `cargo test` run.

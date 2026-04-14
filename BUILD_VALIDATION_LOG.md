# Build And Validation Log

Date: 2026-04-04
Branch: `axis/ab-test-implementation`
Latest pushed commit: `0829dfd`

## Rust tests

- `cargo test --manifest-path pfda-amm/Cargo.toml` -> PASS
- `cargo test --manifest-path axis-g3m/Cargo.toml` -> PASS
- `cargo test --manifest-path axis-vault/Cargo.toml` -> PASS

## SBF builds

- `cargo build-sbf --manifest-path pfda-amm/Cargo.toml` -> PASS
- `cargo build-sbf --manifest-path axis-g3m/Cargo.toml` -> PASS
- `cargo build-sbf --manifest-path axis-vault/Cargo.toml` -> PASS

## TypeScript checks

- `npx --prefix pfda-amm/programs/pfda-amm-3/client tsc --noEmit -p pfda-amm/programs/pfda-amm-3/client/tsconfig.json` -> PASS
- `npx --prefix axis-g3m/client tsc --noEmit -p axis-g3m/client/tsconfig.json` -> PASS
- `npx --prefix axis-vault/client tsc --noEmit -p axis-vault/client/tsconfig.json` -> PASS
- `npx --prefix scripts tsc --noEmit -p scripts/tsconfig.json` -> PASS

## Devnet validation

### ETF A canonical

Command:

```bash
cd pfda-amm/programs/pfda-amm-3/client
npx ts-node oracle-bid-e2e.ts
```

Result: PASS

- InitPool: `13,666 CU`
- SwapRequest: `9,276 CU`
- ClearBatch: `11,511 CU`
- Claim: `2,052 CU`
- Oracle: PASS
- Bid: PASS
- Treasury delta: `+1,000,000 lamports`

### ETF B rehearsal

Command:

```bash
cd axis-g3m/client
npx ts-node e2e-devnet.ts
```

Result: PASS

- InitializePool: `22,193 CU`
- Swap: `18,074 CU`
- CheckDrift: `3,582 CU`
- LargeSwap: `18,070 CU`
- Rebalance: `13,382 CU`
- Post-large-swap drift: `2010 bps`
- `needs_rebalance = true`

### A/B rehearsal script

Command:

```bash
cd scripts
npx ts-node run-ab-rehearsal.ts
```

Result: PASS

- ETF A ClearBatch: `10,011 CU`
- ETF B max drift: `2010 bps`

### Axis vault

Command:

```bash
cd axis-vault/client
npx ts-node e2e.ts
```

Result: PASS

- CreateEtf: `10,566 CU`
- Deposit: `5,684 CU`
- Withdraw: `5,733 CU`
- ETF tokens minted: `1,000,000,000`
- Withdraw returned proportional basket amounts

Note:

- `axis-vault/client/e2e.ts` was updated to support `RPC_URL` override and fresh `ETF_NAME` generation so reruns do not collide with an existing devnet PDA.

## Local validator validation

### GitHub Actions

- Run: `23969316716`
- Status: PASS
- Jobs:
  - `Rust Tests, SBF Builds, and TS Checks` -> PASS
  - `Local Validator E2E` -> PASS

### Manual local CI-shaped sweep

Programs loaded into local validator:

- `pfda-amm-3`
- `axis-g3m`
- `axis-vault`

Results:

- PFDA-3 local:
  - InitPool: `25,611 CU`
  - SwapRequest: `19,845 CU`
  - ClearBatch: `13,084 CU`
  - Claim: `6,621 CU`
- G3M local:
  - InitializePool: `45,038 CU`
  - Swap: `27,212 CU`
  - CheckDrift: `3,582 CU`
  - LargeSwap: `27,208 CU`
  - Rebalance: `13,382 CU`
- Axis vault local:
  - CreateEtf: `23,587 CU`
  - Deposit: `23,760 CU`
  - Withdraw: `24,016 CU`

## Scope note

- ETF A is the canonical implemented A/B path in this branch.
- ETF B currently validates the rehearsal/state-sync path, not the original same-transaction Jupiter CPI auto-rebalance design.

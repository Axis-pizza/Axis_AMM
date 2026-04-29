# Mainnet v1 Scope

This document defines what ships to mainnet for v1, what is explicitly out of scope, and what the audit boundary is. Audit firms should bid against this document; the rest of the repo is research and ops scaffolding.

Last updated: 2026-04-30 (post pfda-amm-3 verifiable deploy)

---

## TL;DR

Mainnet v1 ships **two Solana programs**:

| Program | Mainnet status | Mainnet ID | Lines of code (approx) | Audit priority |
|---|---|---|---|---|
| `pfda-amm-3` | **live, OtterSec-verified** (2026-04-29) | `3SBbfZgzAHyaijxbUbxBLt89aX6Z2d4ptL5PH6pzMazV` | ~2,400 | P0 |
| `axis-vault` | **live, OtterSec-verified** (2026-04-29) | `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX` | ~2,100 | P0 |

Built with [Pinocchio](https://github.com/anza-xyz/pinocchio), `no_std`, no Anchor.

Total in-scope on-chain code: **~4,500 LoC** + supporting modules (oracle reader, Jito bid logic, fixed-point math).

Everything else in this repo (`axis-g3m`, `pfda-amm`, `solana-tfmm-rs`, A/B harness, simulation models) is **out of mainnet scope** and stays on devnet/locally.

---

## In-scope programs

### 1. `pfda-amm-3` — 3-token Periodic Fee-Discount Auction AMM

**Purpose.** A discrete-time batch auction AMM that internalizes Loss-versus-Rebalancing (LVR) on Solana. Users submit swap intents during a window; one searcher wins the right to clear the batch by paying a Jito bid to the protocol treasury; the clear executes a single O(1) settlement against the aggregated state at an oracle-bounded clearing price.

**Instructions (8, all in scope):**

| Disc | Name | Authority required | Purpose |
|---|---|---|---|
| 0 | `InitializePool` | Pool creator | One-time pool setup with 3 mints, weights, fee, window. `base_fee_bps` capped at `MAX_BASE_FEE_BPS = 100` (1.0 %, Uniswap V3 top tier) and immutable post-init |
| 1 | `SwapRequest` | User (signer) | Add a swap intent to the current batch queue |
| 2 | `ClearBatch` | Cranker (signer) | Settle the batch; pays bid to treasury, computes clearing prices |
| 3 | `Claim` | User (signer) | Withdraw the user's pro-rata share of the cleared batch |
| 4 | `AddLiquidity` | Authority | Add reserves to all three vaults proportionally |
| 5 | `WithdrawFees` | Authority | Withdraw accumulated protocol fees |
| 6 | `SetPaused` | Authority | Emergency pause toggle |
| 7 | `CloseBatchHistory` | Cranker | Close historical batch accounts after delay (rent reclaim) |
| 8 | `CloseExpiredTicket` | Cranker | Close unclaimed user tickets after delay (rent reclaim) |

Disc 9 (`SetBatchId`) is **feature-gated** behind `cargo build-sbf --features test-time-warp` and **does not exist in the mainnet build**. The mainnet binary rejects disc 9 with `InvalidInstructionData`. This is enforced by an integration test (`contracts/ab-integration-tests/tests/issue_61_coverage.rs::pfda3_set_batch_id_disc_unknown_in_mainnet_build`).

**External dependencies:**
- **Switchboard On-Demand V3** (`SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv`) — strict-mode oracle bounding for clearing prices. When feeds are supplied, all three must be fresh; mixed staleness aborts the batch (sandwich-resistance, see `clear_batch.rs:186-220`).
- **Jito Block Engine** — searchers submit `ClearBatch` with `bid_lamports`, transferred to `pool.treasury` on success.
- **No CPI into other in-scope programs.** Pure self-contained AMM.

### 2. `axis-vault` — ETF token lifecycle

**Purpose.** Manages a basket of up to 5 SPL tokens as a single ETF token. Users deposit basket tokens and receive ETF tokens minted proportionally; users burn ETF tokens and receive basket tokens back. Protocol takes a configurable fee (capped at 3 %) routed to a Squads V4 multisig treasury.

**Instructions (9, all in scope):**

| Disc | Name | Authority required | Purpose |
|---|---|---|---|
| 0 | `CreateEtf` | Authority | Create an ETF with a basket, weights, ticker, name |
| 1 | `Deposit` | User (signer) | Deposit basket tokens, mint ETF tokens proportionally |
| 2 | `Withdraw` | User (signer) | Burn ETF tokens, receive proportional basket tokens |
| 3 | `SweepTreasury` | Treasury (signer) | Burn accumulated treasury ETF balance, receive proportional basket |
| 4 | `SetPaused` | Authority | Emergency pause toggle |
| 5 | `DepositSol` | User (signer) | Wrap SOL → wSOL → Jupiter route into each basket token → mint ETF |
| 6 | `WithdrawSol` | User (signer) | Burn ETF → Jupiter route each basket token → wSOL → unwrap |
| 7 | `SetFee` | Authority | Adjust `fee_bps` within `[0, max_fee_bps]` |
| 8 | `SetCap` | Authority | Raise (never lower) the TVL cap |

**External dependencies:**
- **Jupiter V6** (`JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4`) — CPI'd from `DepositSol`/`WithdrawSol` for SOL-side ETF creation/redemption. Verified by program-ID equality before invoke (`axis-vault/src/jupiter.rs:79`).
- **wSOL mint** (`So11111111111111111111111111111111111111112`) — verified by program-ID equality.
- **SPL Token program** — verified as owner of every vault and treasury ATA before any read.
- **Squads V4 multisig** — `PROTOCOL_TREASURY` constant, governance-gated CreateEtf check (currently inert, flips active when constant is non-zero per `docs/ops/SQUADS_RUNBOOK.md`).
- **No CPI into `pfda-amm-3`.** Vault and AMM are independent venues that share token mints only.

---

## Out of scope (and why)

### `axis-g3m` — research-only baseline

`axis-g3m` is a 5-token G3M (geometric mean market maker) continuous AMM with drift-based rebalance. It exists to provide the "Vanilla" baseline in the A/B test (`reports/ab/latest.md`). The thesis of the project is that PFDA outperforms G3M at LVR mitigation; G3M is the comparison subject, not a shipped product.

**Mainnet excluding g3m saves:**
- ~1,800 LoC of audit work
- Two extra entry-point dispatchers
- Disc=4 `RebalanceViaJupiter` design rework (see deferred item D1 below)
- The associated keeper bot, oracle infrastructure, and fund flows for a token nobody will use

**The A/B test data is preserved** by keeping `axis-g3m` on devnet and continuing to publish `reports/ab/*` from there. `pfda-amm-3` ships standalone; users don't need to see g3m to use the LVR-friendly pool.

### `pfda-amm` (legacy 2-token) — regression-only

The original 2-token PFDA prototype that predates the 3-token canonical implementation. Kept in-tree because some integration tests use it to detect regressions in shared modules (`oracle.rs` math, `jito.rs` bid logic). **Not deployed to mainnet.**

### `solana-tfmm-rs` — off-chain simulator

The Python/Rust simulation harness that generates the LVR P&L tables in `README.md`. Off-chain code, never executed on a Solana validator.

---

## Deferred to v1.1+ (NOT shipping in v1, NOT audited in v1)

### D1. `axis-g3m` disc=4 `RebalanceViaJupiter` (real Jupiter CPI)

**Status.** Code exists in `contracts/axis-g3m/src/jupiter.rs:78-307`. Has a structural design issue: the CPI metas builder auto-prepends vault accounts at slots 0–N, but Jupiter V6's swap-instruction layout expects Token Program at slot 0 and signer at slot 1 (per `test/fixtures/jupiter/sol-usdc-100m.json`). The two layouts collide, so a real Jupiter CPI through this path will fail.

**Path to v1.1.** Refactor `process_rebalance_via_jupiter` to mirror `axis-vault/src/jupiter.rs::invoke_jupiter_leg` (caller controls full account order, no auto-prepend). Re-record the route fixture with `userPublicKey = pool PDA` so source/dest ATAs are the pool's vaults. Wire the e2e in `test/e2e/axis-g3m/axis-g3m.jupiter-fork-disc4.e2e.ts`.

**v1 mitigation.** axis-g3m uses attestation-mode rebalance (disc=3) for all flows. This is a trust-the-authority model: the keeper computes target reserves off-chain (typically from a Jupiter quote) and submits them via authority signature. The program validates: drift exceeded, cooldown elapsed, post-rebalance global k within 0.5 %, per-token weight within drift threshold, single-token reserve change ≤ 50 %, Jupiter program present as a witness account at index 2. This is what `reports/ab/*` already use.

**Why deferring is safe.** axis-g3m doesn't ship to mainnet in v1, so the broken disc=4 path is unreachable.

### ~~D2. `PROTOCOL_TREASURY` Squads V4 multisig flip~~ — DONE 2026-04-29

**Status (2026-04-29).** Shipped. `contracts/axis-vault/src/constants.rs:73-78` now holds the Squads V4 vault key `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6`; `protocol_treasury_is_active()` returns true; `CreateEtf` enforces `treasury == PROTOCOL_TREASURY` per `instructions/create_etf.rs:103-110`. Verified by `bun test/frontend/programs.test.ts` (canonical-treasury assertions) and by the on-chain `axis-vault` upgrade landed at slot 416463163 (deploy sig `2TBxw…vsMHBY` for the original deploy; redeployed under the same Squads vault as part of the verifiable-build flow — see "Deployment artifacts" below).

### ~~D3. Upgrade-authority handoff to Squads multisig~~ — DONE 2026-04-30

**Status.** Shipped for both programs. Mainnet upgrade authority is the Squads V4 vault `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6` for `axis-vault` (since 2026-04-29) and `pfda-amm-3` (since 2026-04-30, immediately after the initial deploy via `set-upgrade-authority --skip-new-upgrade-authority-signer-check`). Verified via `solana program show … -u mainnet-beta` for both.

### D4. 24–72h upgrade timelock

**Status.** Squads multisig provides multi-sig but no time delay. DeFi norm is 24–72h propose-execute window so users can withdraw before a contentious upgrade lands.

**Path.** v1.1: introduce an on-chain timelock program owning the upgrade authority, with Squads as the proposer and a 48h delay. Emergency-pause path bypasses delay for `SetPaused` only.

**v1 mitigation.** Documented as a known gap; mitigated by 2-of-2 multisig and explicit hardware-wallet requirement on signers.

### D5. Codama IDL + on-chain event emission

**Status (2026-04-30).** Partial. A hand-rolled `axis.manual-idl.v1` for `axis-vault` ships at `idl/axis_vault.json` covering all 9 instructions, the 536-byte `EtfState` layout (with explicit alignment padding and offsets), the 9000-block error variants, the deploy artifacts (program ID, programData PDA, slot, signature, sbf SHA-256, upgrade authority, protocol treasury), and the relevant constants. External integrators have a versioned schema to read against. Codama codegen + on-chain event emission (`msg!` / `log_data` per ix) are still deferred.

**Path.** Author Codama specs (the manual IDL is the seed for these), generate TS clients into `clients/ts/src/generated/`. Define event schema in `docs/architecture/events.md` (`EVT:` prefix + base64 payload), implement via `pinocchio::msg!` in each ix.

**v1 mitigation.** Hand-rolled clients used in `test/e2e/*` and `frontend/src/lib/ix.ts` are tested against the deployed programs in CI; the manual IDL keeps schema drift visible. Indexers can reconstruct state from account diffs (slow but functional).

### D6. State versioning byte

**Status.** State structs have `_padding` bytes at the end to absorb additive field changes without a discriminator bump (`EtfState`, `PoolState3`, `G3mPoolState`). Discriminator was bumped once already (`etfstate→etfstat2→etfstat3` for `axis-vault`), forcing existing devnet pools to re-init.

**Path.** v1.1: reserve the first byte of every account as `version: u8`. Add a no-op `MigrateState(version_to)` instruction now to lock the ABI for the future migration ix. Document the policy in `docs/architecture/ADR-001-state-versioning.md`.

**v1 mitigation.** Mainnet pools at v1 launch start at v1; the discriminator-bump approach has been exercised once and works (re-init only).

### D7. TVL cap defaults to 0 (uncapped)

**Status.** `EtfState.tvl_cap` is initialized to 0 (uncapped) by `CreateEtf`. The closed-beta ramp curve is set by the authority via `SetCap`.

**Path.** At mainnet launch the authority sets a low initial cap (e.g. ~$100k equivalent per ETF), then raises monotonically as soak-test data accumulates. `SetCap` rejects decreases (`InvalidCapDecrease`) so a compromised authority cannot strand pools above a lower cap.

---

## Accepted v1 risks

These are known, documented, and accepted for v1 launch. Each has a mitigation; none are blockers.

| # | Risk | Mitigation |
|---|---|---|
| R1 | No third-party security audit at launch | Audit booked before TVL exceeds $100k. Closed-beta ramp via TVL cap (`SetCap`) limits exposure during pre-audit window. |
| ~~R2~~ | ~~Single-EOA upgrade authority at first deploy~~ | **Resolved 2026-04-29.** `axis-vault` upgrade authority is the Squads V4 vault `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6`. `pfda-amm-3` will start under the same Squads vault on its first mainnet deploy. |
| R3 | No on-chain timelock on upgrades | Squads 2-of-2 + hardware wallets; v1.1 adds timelock (D4) |
| R4 | ETF mint has `freeze_authority = None` | Intentional trust-minimization decision. ETF holders cannot have their tokens frozen; the protocol cannot recover funds for compromised users. Disclosed in user-facing docs. |
| R5 | No Pyth oracle redundancy on `pfda-amm-3` | Only Switchboard. Strict-mode rejects on any stale feed. The pool freezes during a Switchboard outage, but no funds are lost. v1.1 adds Pyth fallback. |
| R6 | No automated bug-bounty program at launch | `SECURITY.md` will publish a `security@` email and severity-based reward table. Immunefi listing planned post-audit. |
| R7 | No event emission for indexers | Indexers reconstruct state from account diffs. v1.1 adds `EVT:` prefix log_data (D5). |
| R8 | Hand-rolled TS instruction encoders | Tested against the deployed programs in CI; sloppy clients fail integration tests before merge. The manual IDL at `idl/axis_vault.json` documents the layout; Codama codegen still v1.1 (D5). |
| R9 | Frontend client-side Jupiter composition (Deposit-SOL/Withdraw-SOL flows) instead of on-chain `DepositSol`/`WithdrawSol` CPI | The on-chain disc=5/6 paths are audit-in-scope but currently unused by the deployed UI; the UI bundles Jupiter `swap-instructions` ixs and an axis `Deposit`/`Withdraw` ix into one (or two, on overflow) versioned tx. Sidesteps the CPI account-layout concern in D1 while preserving slippage guards (`min_mint_out`, `min_tokens_out`, `MIN_FIRST_DEPOSIT_BASE`). |

---

## Audit boundary

### What we want audited

The 4,500 LoC across `contracts/pfda-amm-3/src/**` and `contracts/axis-vault/src/**`.

Specific high-priority modules:

| Module | LoC | Reason |
|---|---|---|
| `pfda-amm-3/src/instructions/clear_batch.rs` | ~600 | Most complex instruction; oracle-bounded clearing math + reentrancy guard + bid validation |
| `pfda-amm-3/src/oracle.rs` | ~120 | Switchboard discriminator/owner check, Q32.32 conversion, staleness handling |
| `pfda-amm-3/src/jito.rs` | ~130 | Bid validation, alpha-split math, tip-account verification |
| `axis-vault/src/instructions/deposit.rs`, `withdraw.rs` | ~250 each | Proportional mint math, NAV deviation guard, vault owner checks |
| `axis-vault/src/instructions/deposit_sol.rs`, `withdraw_sol.rs` | ~390 each | Jupiter CPI surface, multi-leg orchestration, slippage |
| `axis-vault/src/instructions/sweep_treasury.rs` | ~225 | Treasury redemption path, supply accounting interaction |
| `axis-vault/src/instructions/set_fee.rs`, `set_cap.rs`, `set_paused.rs` | ~90 each | Authority-gated state mutations, PDA re-derivation |
| `axis-vault/src/state/etf.rs` | ~75 | State layout (536 bytes, etfstat3 discriminator) |
| `*/src/state/mod.rs` (all 3 in-scope programs) | ~35 each | Unsafe transmute with size + alignment guard |

### What we do NOT want audited (out of scope)

- `contracts/axis-g3m/**` — research-only, not shipped
- `contracts/pfda-amm/**` — legacy regression code, not shipped
- `contracts/ab-integration-tests/**` — test code, not shipped
- `solana-tfmm-rs/**` — off-chain simulator, not on Solana
- `test/**` — TypeScript test code, not on Solana
- `scripts/**` — ops tooling, not on Solana
- `.github/workflows/**` — CI configuration

### Threat model summary

The program author trusts:
- The Switchboard On-Demand program at `SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv` (mainnet) and its devnet equivalent
- The Jupiter V6 program at `JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4`
- The SPL Token program
- The Squads V4 multisig program (post-D2 flip)
- The Solana validator runtime (account loading, rent, BPFLoader correctness)

The program author DOES NOT trust:
- Any other program ID passed in as a CPI target — verified by exact-match before invoke
- Any account passed in as writable — verified to be SPL Token-owned, key-matching expected vault, before any data read
- Pool authority — limited to fee/cap/pause adjustments within bounds (axis-vault `MAX_FEE_BPS_CEILING = 300`, monotonic cap; pfda-amm-3 has no fee-update path — `base_fee_bps` fixed at init under `MAX_BASE_FEE_BPS = 100`)
- Treasury keypair — limited to sweep operation; no slippage exposure
- User submitted instruction data — bounds-checked, integer overflow-checked

---

## Deployment artifacts

### `axis-vault` — live on mainnet-beta

```
─── Mainnet deploy (axis-vault) ─────────────────────────────
Original deploy:      2026-02-… (slot 416272971,
                      sig 2TBxweDiUk96FAY9hmmPD9cEPcT6NUAsZEbEkvvWQiNLagukYfni7TyZzh3u5CdEHPk9f3Jb9DRbLt6R6wvsMHBY)
Verifiable redeploy:  2026-04-29 (slot 416463163; the
                      original .so was built outside docker so
                      its normalized hash diverged from any
                      reproducible build — redeployed the
                      `solanafoundation/solana-verifiable-build:3.0.14`
                      output via Squads to clear OtterSec's
                      hash check; rent-cycle through buffer
                      `GLapRTYhvTs4gFdnaXVadsZi23pSL49Sa1vBeBcgyyWD`)
Deploying wallet:     6pZuwgM4ZyzWjtjMSGap5Zw4GCUo3q7RxFPsFxSLao5o
                      (single-sig prep only; no upgrade authority)
Audit firm + link:    [TBD — booked before TVL > $100k per R1]

  Program ID:                Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX
  ProgramData PDA:           6szAV5iFQKzJ7BuYZipSeWc3thauWCVi9q26k1WQEjrt
  Upgrade authority:         BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6  (Squads V4 vault, 2-of-2)
  PROTOCOL_TREASURY:         BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6  (same vault; CreateEtf gate active)
  .so size:                  104,656 bytes
  .so raw SHA-256:           e7911f5da477e2416bfe0265312ec94ffe74d82233a16a0f963644af9b4e9c0a
  solana-verify hash:        0253e0316161f40608fdcd5ba325db4b600956f83dd6547c02ef7ae827a5af53
  OtterSec verifier:         is_verified: true (job 6d37f26d-18bf-4141-b808-933de3230599)
  OtterSec status URL:       https://verify.osec.io/status/Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX
  Verify-PDA:                HQpeRmsFC7U7EfuQgrssGuvZ9e5WetsaYkGqoZ2R3XwT
  Source repo + commit:      https://github.com/Axis-pizza/Axis_AMM @ f400d88e236f7c145c6389025058564c3ed0f457
─────────────────────────────────────────────────────────────
```

### `pfda-amm-3` — live on mainnet-beta

```
─── Mainnet deploy (pfda-amm-3) ─────────────────────────────
Initial deploy:       2026-04-30 (slot 416476977,
                      sig 3q2cXvPVskHfWRyEHoF8YZXoodm9Y3Kxwd2HWEU4ys1qCMovAF4NjH6uxYjEmGnzyFUtct8XourUaFqUqHs87fh3)
Build path:           `solana-verify build --base-image
                      solanafoundation/solana-verifiable-build:3.0.14
                      contracts/pfda-amm-3` first; the docker hash
                      matched the on-chain hash on the first try
                      (no redeploy needed, unlike axis-vault).
Deploying wallet:     6pZuwgM4ZyzWjtjMSGap5Zw4GCUo3q7RxFPsFxSLao5o
                      (single-sig prep only; upgrade authority
                      handed to Squads vault immediately after deploy)
Audit firm + link:    [TBD — booked before TVL > $100k per R1]

  Program ID:                3SBbfZgzAHyaijxbUbxBLt89aX6Z2d4ptL5PH6pzMazV
  ProgramData PDA:           Hy46nZWmSzFjyDcQvVdF51RiiT4EcTjBo85t5SKZj6ax
  Upgrade authority:         BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6  (Squads V4 vault, 2-of-2)
  .so size:                  68,744 bytes
  .so raw SHA-256:           0d73e873b5a01b95d219520e967ff4837e9202aa4210ab7d77bad00ae596282e
  solana-verify hash:        b75828371550e3fa7e9955cb6edac4c53a10c0dab6ec0d0ab4ee44150263fa98
  OtterSec verifier:         is_verified: true (job f52b115f-bd3f-4703-a044-f8601b3c3a8f)
  OtterSec status URL:       https://verify.osec.io/status/3SBbfZgzAHyaijxbUbxBLt89aX6Z2d4ptL5PH6pzMazV
  Verify-PDA:                BbhvCmpxtTsfJWySJPBRSaxGV7ap3wNgLJ3s5MVnaLhd
  Source repo + commit:      https://github.com/Axis-pizza/Axis_AMM @ af3c2296e47b205dba8d437ebe89c371902a6544
  Out-of-pocket cost:        ~0.4812 SOL (programData rent locked +
                             ~0.001 program-account rent + tx fees;
                             rent rebatable only via `solana program close`)
─────────────────────────────────────────────────────────────
```

The full Squads-multisig flow (write-buffer, upgrade, verify-PDA export, OtterSec submit-job) is documented step-by-step in [`docs/ops/SOLSCAN_VERIFY_RUNBOOK.md`](../ops/SOLSCAN_VERIFY_RUNBOOK.md).

---

## How to use this document

- **Audit firms:** quote against the In-scope section. Out-of-scope and Deferred are not your responsibility. Use the Threat-model and Accepted-risks sections to scope your engagement letter.
- **Stakeholders / reviewers:** the In-scope and Out-of-scope sections together must equal the contents of `contracts/`. If something in `contracts/` isn't categorized here, the categorization is wrong, not the code.
- **Future authors:** when you ship a v1.1 feature, move its line item from Deferred to In-scope, and bump the v1 → v1.1 designation throughout. Do not silently expand scope without updating this file.

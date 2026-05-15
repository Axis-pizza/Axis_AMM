# RFC + Review: Muse 2026-05-15 ETF deposit report (3 issues)

Status: **PROPOSAL + status review — needs Muse sign-off before any program change.**
Scope: `contracts/axis-vault` (OtterSec-verified, mainnet `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX`) for the program change; consumer app `Axis_Mainnet/axis-agent` + `axis-api` for the client/back-end fixes. **No program code is applied by this RFC.**
Author context: filed in response to Muse's 2026-05-15 report. Originally a single-issue RFC for the SOL-floor bug; expanded so all three reported items are reviewable in one place.

> **One-PR-per-repo reality:** the durable Bug 1 program fix is a proposal in *this* repo (Axis_AMM PR #72, RFC only, no Rust). The interim Bug 1 client mitigation, the Bug 2 fix, and the Bug 3 fix are **written code sitting in Axis_Mainnet PR #4 (consumer repo) — OPEN, unmerged, not in any `main`.** Nothing in this report is live yet.

---

## 0. Summary for Muse

| # | Muse's report | Real cause | Status | Where the code is | Needs from Muse |
|---|---------------|-----------|--------|-------------------|-----------------|
| 1 | wBTC/wETH baskets require too much SOL for the initial deposit | Program design: first-deposit `amount` is one raw-base-unit scalar across mixed-decimal/price tokens vs. a fixed 1.0-ETF floor | Interim client mitigation written (~10× reduction); durable program fix is a **proposal only** | Interim: Axis_Mainnet PR #4 (`jupiterSeed.ts`). Durable: this RFC §3–§5, no code yet | Answer §6 open questions, then approve writing the v1.2 program patch |
| 2 | Spot swaps land in wallet instead of a single ETF token | Single-token *is* implemented; defect is a fragile name-keyed vault/spot decision that **silently falls back to Jupiter-spot-into-wallet** on RPC/decode failure | Fix written | Axis_Mainnet PR #4 (`etfState.ts` tri-state + guards) | Review + merge PR #4 |
| 3 | ETF logo upload doesn't take | Upload UI exists but create hardcodes `uri:''`; `axis-api` `/metadata/:ticker` returns a **hardcoded** logo URL | Fix written, **not runtime-tested** | Axis_Mainnet PR #4 (FE upload wiring + `axis-api` per-ETF metadata) | Review + merge PR #4; accept that e2e needs a real deploy tx |

**Decision checklist** is consolidated in §7.

---

## 1. Issue 1 — Problem

Muse: *"When assets like wBTC or wETH are included, the app ends up requiring too much SOL for the initial deposit."*

Confirmed and root-caused. It is **not** a UI bug — it is a program design issue in the first-deposit path.

### Verified on-chain behavior

- `constants.rs:27` — `MIN_FIRST_DEPOSIT = 1_000_000` (= 1.0 ETF at 6 decimals).
- `constants.rs:18` — `MAX_NAV_DEVIATION_BPS = 300`; `constants.rs:37` — `MINIMUM_LIQUIDITY = 1_000`.
- `create_etf.rs:170,173` — the ETF SPL mint is hardcoded to **6 decimals**.
- `deposit.rs` first-deposit floor:
  ```rust
  if total_supply == 0 && amount < MIN_FIRST_DEPOSIT {
      return Err(VaultError::InsufficientFirstDeposit.into());
  }
  ```
- `deposit.rs` per-leg pull (raw base units, **no decimal or price normalization**):
  ```rust
  token_amounts[i] = (amount as u128) * (weights[i] as u128) / 10_000
  ```
- `deposit.rs` first-deposit mint:
  ```rust
  let mint_amount = if total_supply == 0 { amount } else { /* pro-rata */ };
  ```

### Why this breaks for wBTC / wETH

`amount` is a **single scalar that means three things at once** on the first deposit:

1. the number of ETF tokens minted (6-decimal),
2. the per-leg basket pull `amount × weightᵢ / 10_000` in **token i's own raw base units**,
3. the value gated by the `≥ 1_000_000` floor.

Basket tokens have wildly different (decimals, price). wBTC/wETH are 8-decimal, high-USD-price: a given USD buys very few raw base units. To make the wBTC leg's `amount × weightᵢ / 10_000` clear a meaningful balance while also clearing `amount ≥ 1_000_000`, the depositor must supply a huge SOL seed — and every other (cheaper-per-base-unit) leg is massively over-bought to drag that one scalar up. The required SOL is dominated by the worst-decimals/highest-price leg, not by the basket's actual value.

This is intrinsic to `amount` being a shared raw-base-unit scalar across heterogeneous-decimal tokens with a fixed base-unit floor.

## 2. Issue 1 — Interim mitigation already written (client-only, no redeploy)

> Lives in **Axis_Mainnet PR #4 — OPEN, unmerged.** Not live until that PR merges.

`Axis_Mainnet/axis-agent` — `jupiterSeed.ts` now reallocates the same total SOL across legs to **equalize per-leg deposit candidates** instead of bottlenecking on the lowest-base-unit leg (`buildJupiterSeedPreview` + `reallocateEqualizingCandidates`). Verified with the repo's mock Jupiter client (`axis-agent/scripts/verify-seed-equalize.ts`): on a realistic 3-leg basket with a low-weight wBTC-like leg the required seed dropped ~**10×**; on a 2-equal-leg basket ~2× (the structural ceiling for that shape). tsc clean. Not runtime-tested against live Jupiter.

This makes the seed the *true minimum* SOL to mint a legal first deposit, but it **cannot** make 1.0 ETF cheap when a high-value 8-decimal token sits at a meaningful weight — the USD value of `1_000_000 × weight_wbtc / 10_000` base units of wBTC is intrinsically large. That residual is what the program change below addresses.

## 3. Issue 1 — Options (program change)

| # | Change | Pros | Cons |
|---|--------|------|------|
| A | Lower / make `MIN_FIRST_DEPOSIT` per-ETF configurable | Tiny diff | Doesn't fix the per-leg imbalance; weakens the dust-seed protection that `MINIMUM_LIQUIDITY` + `MIN_FIRST_DEPOSIT` jointly provide |
| B | Decimal-normalize `token_amounts` using stored per-mint decimals (scale every leg to a common 1e9 internal unit) | No oracle dependency | Removes *decimal* skew but not *price* skew — a $100k 8-dec token and a $1 8-dec token still behave very differently |
| **C (recommended)** | **Oracle/value-normalized first deposit**: derive `mint_amount` and the floor from the **USD value** of the deposited basket via Switchboard, decoupling ETF supply from raw base units (the Symmetry model) | Correct in general; floor becomes a sane USD minimum; matches the canonical Solana index design (Symmetry uses Pyth-priced valuation) | Touches audited program; needs oracle accounts threaded into `Deposit`; audit + migration |

### Why C is feasible here (not a from-scratch oracle build)

A Switchboard on-demand price reader **already exists in this repo** and is proven on mainnet for the sibling program:
- `contracts/pfda-amm-3/src/oracle.rs:1` — "Switchboard on-demand oracle price feed reader for 3-token PFDA."
- `contracts/pfda-amm-3/src/oracle.rs:49` — `verify_switchboard_owner(...)` ownership check.

Porting that reader into `axis-vault` reuses an audited pattern and adds no new crate dependency (consistent with the Pinocchio "no extra deps" rule noted in `AXIS_VAULT_V1_1_SPEC.md`).

## 4. Issue 1 — Recommended design (Option C, shipped as **v1.2**)

**Principle:** on the first deposit, `mint_amount` = the basket's total USD value scaled to a fixed 6-decimal unit (e.g. `1 ETF ≈ $1` at genesis, configurable per ETF). Per-leg pulls stay weight-proportional **by value**, not by raw base units. The floor becomes "first deposit ≥ $X" rather than "≥ 1e6 raw units of a scalar."

### Program changes (sketch — exact diff after sign-off)

1. **`CreateEtf`**: store one Switchboard feed pubkey per basket mint (or a single quote-denominated feed set). `EtfState` layout is unchanged on the wire today; `AXIS_VAULT_V1_1_SPEC.md:79` confirms a `_padding: [u8; 4]` tail reserved for v1.2 — feed references likely need more than 4 bytes, so this requires either (a) a new trailing region with a discriminator bump, or (b) a side PDA `EtfOracleConfig`. **(b) is preferred** — keeps `etfstat3` discriminator stable and avoids re-init of existing ETFs.
2. **`Deposit`**: accept the per-leg Switchboard feed accounts; on `total_supply == 0` compute `value_usdₓ = Σ priceᵢ × balance_pulledᵢ / 10^decimalsᵢ`; set `mint_amount = value_usd × 1e6 / GENESIS_PRICE_USD`; replace the raw `amount < MIN_FIRST_DEPOSIT` floor with `value_usd ≥ MIN_FIRST_DEPOSIT_USD`. Subsequent-deposit pro-rata math is unaffected (it is already supply-relative, `deposit.rs` `else` branch).
3. Reuse `MAX_NAV_DEVIATION_BPS` (300) for an oracle staleness/confidence bound, mirroring `pfda-amm-3/src/oracle.rs`.

### Compatibility & migration

- New behavior gated to **new** ETFs created under v1.2 (mirrors the v1.1 approach: `AXIS_VAULT_V1_1_SPEC.md:131` — "v1.1 only affects new CreateEtf calls. No off-chain backfill, no per-ETF migration tx."). Existing v1.1 ETFs keep current semantics.
- Wire-format bump tracked in `MAINNET_SCOPE.md` change-log as a deliberate breaking change (same discipline as the v1.1 `uri_len` addition, `AXIS_VAULT_V1_1_SPEC.md:121`).
- Client (`axis-agent`) deposit/seed plan switches to value math for v1.2 ETFs; the client equalizer mitigation stays as defense-in-depth.

### Audit impact

- Re-review scope: `Deposit` first-deposit branch + new oracle read path + `CreateEtf` feed storage / new `EtfOracleConfig` PDA. Bounded; does not touch withdraw, pause, fee, or subsequent-deposit math.
- Threat additions to cover: oracle manipulation / stale price / confidence interval, feed-account substitution (the `verify_switchboard_owner` check from `pfda-amm-3/src/oracle.rs:49` is the mitigation), genesis-price griefing.

### Test plan

- Extend the existing LiteSVM A/B suite (`contracts/ab-integration-tests/`, already wired to v1.1 wire format + Metaplex per recent commits) with: wBTC/wETH-shaped baskets, oracle-mocked first deposits, staleness/confidence rejection, value-floor boundary, and a v1.1-vs-v1.2 differential to prove subsequent-deposit math is unchanged.
- Mainnet dry-run on a throwaway ETF before announcing.

## 5. Issue 2 — Spot swaps into wallet vs. single ETF token

Muse: *"deposits should produce a single ETF token, not spot swaps into the wallet."*

**Single-ETF-token is already implemented** — this is a fragile-fallback bug, not a missing feature.

### Verified behavior (Axis_Mainnet/axis-agent @ `7443591`, 2026-05-15)

- Create runs `ixCreateEtf` (one SPL mint + Metaplex metadata + vaults), then vault `Deposit` mints the ETF token (`PfmmDeploymentBlueprint.tsx:569,971`).
- Swipe BUY is axis-vault-first and bounces PFMM-only baskets to the detail view (`SwipeDiscoverView.tsx:1147-1160`; the `else if (isPfmm)` Jupiter-to-wallet branch at :1271 is unreachable from there).
- **The real defect:** the vault-vs-spot decision keys off an `etfState` PDA derived from `owner + strategy.name` (`SwipeDiscoverView.tsx:784`, `StrategyDetailView.tsx:838,908`). A name mismatch *or* an RPC/decode failure makes the lookup look "absent", and the code **silently falls back to Jupiter spot-swap into the wallet** (`StrategyDetailView.tsx:833` comment). That is exactly the symptom Muse saw.

### Fix written (Axis_Mainnet PR #4 — OPEN, unmerged)

- New `etfState.ts` `classifyEtfState` returns an explicit **tri-state**: `present` / `absent` / `error`.
- Guards in `SwipeDiscoverView.tsx` and `StrategyDetailView.tsx` so a real ETF whose state lookup *errored* (RPC/decode) is never treated as "no vault → spot-swap into wallet". `error` blocks the spot fallback instead of silently degrading. tsc clean. Not runtime-tested.

Open question for Muse: confirm desired UX when the vault PDA genuinely can't be resolved after retries — hard-fail with a visible error (current PR behavior), or queue/retry?

## 6. Issue 3 — ETF logo upload

Muse: *"uploaded ETF logos don't show up."*

### Verified behavior (2026-05-15)

- `ImageUpload.tsx` + `api.uploadImage` exist, but create hardcodes `uri:''` (`PfmmDeploymentBlueprint.tsx:580`) — the uploaded image is never threaded into `ixCreateEtf`.
- `axis-api/src/routes/misc.ts:27` `/metadata/:ticker` returns Metaplex JSON with a **hardcoded** `image: LOGO_URL` (`app.axis-protocol.xyz/ETFtoken.png`) — it ignores any upload. `POST /upload/image` returns a raw image URL, not metadata JSON. So even a correct upload could never surface.

### Fix written (Axis_Mainnet PR #4 — OPEN, unmerged)

- FE: `ImageUpload` in the create identity step → `StrategyConfig.logoUrl` → `etfMetadataUri()` → set as the `ixCreateEtf` `uri` → persisted via `persistMetadata`.
- `axis-api`: new `/metadata/mint/:mint` serves the stored logo from `strategies.config` JSON; `/deploy` stores `logoUrl`. **No DB schema migration** (reuses existing `config` JSON column).
- Both repos tsc-clean on changed files.
- **NOT runtime-tested**: requires a real on-chain `CreateEtf` tx + wallet/explorer check + a live D1 instance. Cannot be validated from the contracts repo. Flag this explicitly to Muse — "code complete" ≠ "logo verified on an explorer."

## 7. Consolidated decision checklist for Muse

**A. Merge / review (no program risk):**
1. Review and merge **Axis_Mainnet PR #4** — it carries the entire interim Bug 1 mitigation + Bug 2 fix + Bug 3 fix. Nothing in §2/§5/§6 is live until this merges.
2. Accept that Bug 3 ships "code complete, not e2e-tested" (needs a real deploy tx). OK to merge on that basis?
3. Bug 2 UX call: hard-fail on unresolvable vault PDA (current PR behavior) vs. retry/queue?

**B. Program change (needs sign-off before any Rust is written):**
4. **Genesis price unit** — fix `1 ETF = $1` at first deposit, or let the creator set it?
5. **Floor** — replace `MIN_FIRST_DEPOSIT` with `MIN_FIRST_DEPOSIT_USD` (e.g. $10/$25)? Value affects dust-attack economics.
6. **Storage** — side `EtfOracleConfig` PDA (no discriminator bump, recommended) vs. extend `EtfState` (cleaner reads, forces a layout/version bump)?
7. **Sequencing** — is the shipped client equalizer (~10× reduction on realistic baskets) enough to unblock the demo while v1.2 goes through audit, or is Option C needed before launch?
8. **Oracle coverage** — confirm Switchboard on-demand feeds exist for every mint we intend to allow (wBTC, wETH, majors). Mints without a feed would be rejected at `CreateEtf` under v1.2.

Only after items 4–8 are answered does anyone write the v1.2 program patch. This RFC deliberately ships **zero** Rust.

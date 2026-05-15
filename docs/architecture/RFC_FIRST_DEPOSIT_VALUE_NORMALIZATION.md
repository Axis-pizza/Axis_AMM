# RFC: First-deposit value normalization (axis-vault v1.2)

Status: **PROPOSAL — needs Muse sign-off before any program change.**
Scope: `contracts/axis-vault` (OtterSec-verified, mainnet `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX`). No code applied by this RFC.
Author context: filed in response to Muse's 2026-05-15 bug report ("wBTC/wETH baskets require too much SOL for the initial deposit").

---

## 1. Problem

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

## 2. Interim mitigation already shipped (client-only, no redeploy)

`Axis_Mainnet/axis-agent` — `jupiterSeed.ts` now reallocates the same total SOL across legs to **equalize per-leg deposit candidates** instead of bottlenecking on the lowest-base-unit leg (`buildJupiterSeedPreview` + `reallocateEqualizingCandidates`). Verified with the repo's mock Jupiter client: on a realistic 3-leg basket with a low-weight wBTC-like leg the required seed dropped ~**10×**; on a 2-equal-leg basket ~2× (the structural ceiling for that shape).

This makes the seed the *true minimum* SOL to mint a legal first deposit, but it **cannot** make 1.0 ETF cheap when a high-value 8-decimal token sits at a meaningful weight — the USD value of `1_000_000 × weight_wbtc / 10_000` base units of wBTC is intrinsically large. That residual is what this RFC addresses.

## 3. Options

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

## 4. Recommended design (Option C, shipped as **v1.2**)

**Principle:** on the first deposit, `mint_amount` = the basket's total USD value scaled to a fixed 6-decimal unit (e.g. `1 ETF ≈ $1` at genesis, configurable per ETF). Per-leg pulls stay weight-proportional **by value**, not by raw base units. The floor becomes "first deposit ≥ $X" rather than "≥ 1e6 raw units of a scalar."

### Program changes (sketch — exact diff after sign-off)

1. **`CreateEtf`**: store one Switchboard feed pubkey per basket mint (or a single quote-denominated feed set). `EtfState` layout is unchanged on the wire today; `AXIS_VAULT_V1_1_SPEC.md:79` confirms a `_padding: [u8; 4]` tail reserved for v1.2 — feed references likely need more than 4 bytes, so this requires either (a) a new trailing region with a discriminator bump, or (b) a side PDA `EtfOracleConfig`. **(b) is preferred** — keeps `etfstat3` discriminator stable and avoids re-init of existing ETFs.
2. **`Deposit`**: accept the per-leg Switchboard feed accounts; on `total_supply == 0` compute `value_usdₓ = Σ priceᵢ × balance_pulledᵢ / 10^decimalsᵢ`; set `mint_amount = value_usd × 1e6 / GENESIS_PRICE_USD`; replace the raw `amount < MIN_FIRST_DEPOSIT` floor with `value_usd ≥ MIN_FIRST_DEPOSIT_USD`. Subsequent-deposit pro-rata math is unaffected (it is already supply-relative, `deposit.rs` `else` branch).
3. Reuse `MAX_NAV_DEVIATION_BPS` (300) for an oracle staleness/confidence bound, mirroring `pfda-amm-3/src/oracle.rs`.

### Compatibility & migration

- New behavior gated to **new** ETFs created under v1.2 (mirrors the v1.1 approach: `AXIS_VAULT_V1_1_SPEC.md:131` — "v1.1 only affects new CreateEtf calls. No off-chain backfill, no per-ETF migration tx."). Existing v1.1 ETFs keep current semantics.
- Wire-format bump tracked in `MAINNET_SCOPE.md` change-log as a deliberate breaking change (same discipline as the v1.1 `uri_len` addition, `AXIS_VAULT_V1_1_SPEC.md:121`).
- Client (`axis-agent`) deposit/seed plan switches to value math for v1.2 ETFs; the v1.3-equalizer client mitigation stays as defense-in-depth.

### Audit impact

- Re-review scope: `Deposit` first-deposit branch + new oracle read path + `CreateEtf` feed storage / new `EtfOracleConfig` PDA. Bounded; does not touch withdraw, pause, fee, or subsequent-deposit math.
- Threat additions to cover: oracle manipulation / stale price / confidence interval, feed-account substitution (the `verify_switchboard_owner` check from `pfda-amm-3/src/oracle.rs:49` is the mitigation), genesis-price griefing.

### Test plan

- Extend the existing LiteSVM A/B suite (`contracts/ab-integration-tests/`, already wired to v1.1 wire format + Metaplex per recent commits) with: wBTC/wETH-shaped baskets, oracle-mocked first deposits, staleness/confidence rejection, value-floor boundary, and a v1.1-vs-v1.2 differential to prove subsequent-deposit math is unchanged.
- Mainnet dry-run on a throwaway ETF before announcing.

## 5. Open questions for Muse

1. **Genesis price unit** — fix `1 ETF = $1` at first deposit, or let the creator set it?
2. **Floor** — replace `MIN_FIRST_DEPOSIT` with `MIN_FIRST_DEPOSIT_USD` (e.g. $10/$25)? Value affects dust-attack economics.
3. **Storage** — side `EtfOracleConfig` PDA (no discriminator bump, recommended) vs. extend `EtfState` (cleaner reads, forces a layout/version bump)?
4. **Sequencing** — is the shipped client equalizer (≈10× reduction on realistic baskets) enough to unblock the demo while v1.2 goes through audit, or is C needed before launch?
5. **Oracle coverage** — confirm Switchboard on-demand feeds exist for every mint we intend to allow in baskets (wBTC, wETH, majors). Mints without a feed would be rejected at `CreateEtf` under v1.2.

//! Shared constants for pfda-amm-3.

/// Hard ceiling for `base_fee_bps` accepted by `InitializePool`.
///
/// pfda-amm-3 has no post-deploy fee-update instruction: `base_fee_bps`
/// is set once at init and is immutable for the pool's lifetime. The
/// init-time guard is therefore the only bound on `f` that ever
/// applies, which makes a tight ceiling load-bearing for the security
/// story rather than belt-and-suspenders.
///
/// 100 bps (1.0 %) matches the upper Uniswap V3 fee tier and is the
/// number readers immediately recognise as "the high end of a normal
/// AMM fee," removing the "why is this effectively unbounded?" question
/// auditors and VCs raise against the legacy `>= 10_000` guard.
///
/// Raising this cap requires a state migration (existing pools keep
/// their `base_fee_bps` but new pools deploy under the new constant)
/// — a deliberate, versioned decision rather than per-pool slack.
pub const MAX_BASE_FEE_BPS: u16 = 100;

/// Q32.32 fixed-point arithmetic for on-chain G3M calculations.
/// Adapted from pfda-amm's fp64 module for multi-token G3M pools.
///
/// FP_ONE = 2^32 = 4_294_967_296
/// Range: [0, ~4.29 billion] with ~2.3e-10 precision

pub const FP_ONE: u64 = 1u64 << 32;

// ────────────────────────── Core Ops ──────────────────────────

/// Fixed-point multiply: (a * b) >> 32
#[inline]
pub fn fp_mul(a: u64, b: u64) -> Option<u64> {
    let r = (a as u128).checked_mul(b as u128)?;
    Some((r >> 32) as u64)
}

/// Fixed-point divide: (a << 32) / b
#[inline]
pub fn fp_div(a: u64, b: u64) -> Option<u64> {
    if b == 0 {
        return None;
    }
    let r = ((a as u128) << 32).checked_div(b as u128)?;
    if r > u64::MAX as u128 {
        return None;
    }
    Some(r as u64)
}

/// Convert raw u64 to Q32.32
#[inline]
pub fn fp_from_u64(val: u64) -> Option<u64> {
    if val > (1u64 << 32) - 1 {
        return None;
    }
    Some(val << 32)
}

/// Convert Q32.32 to u64 (truncates fractional part)
#[inline]
pub fn fp_to_u64(val: u64) -> u64 {
    val >> 32
}

// ────────────────────────── Log / Exp ──────────────────────────

/// Fixed-point log2(x) where x is Q32.32. Returns Q32.32 (SIGNED via i64).
/// For x < 1.0 (x < FP_ONE), returns negative value.
/// For the power function, we work with signed intermediate results.
fn fp_log2_signed(x: u64) -> Option<i64> {
    if x == 0 {
        return None;
    }

    // Find MSB position
    let mut msb: i64 = 0;
    let mut temp = x;

    if temp >= 1u64 << 32 { temp >>= 32; msb += 32; }
    if temp >= 1u64 << 16 { temp >>= 16; msb += 16; }
    if temp >= 1u64 << 8  { temp >>= 8;  msb += 8;  }
    if temp >= 1u64 << 4  { temp >>= 4;  msb += 4;  }
    if temp >= 1u64 << 2  { temp >>= 2;  msb += 2;  }
    if temp >= 1u64 << 1  { msb += 1; }

    // log2 integer part = msb - 32 (since 1.0 = 2^32)
    let int_part = msb - 32;

    // Normalize to [FP_ONE, 2*FP_ONE)
    let mut val = x;
    if msb > 32 {
        val >>= (msb - 32) as u32;
    } else if msb < 32 {
        val <<= (32 - msb) as u32;
    }

    // Fractional part via iterative squaring (always positive)
    let mut frac: u64 = 0;
    for i in (0..32).rev() {
        val = ((val as u128 * val as u128) >> 32) as u64;
        if val >= (2u64 << 32) {
            val >>= 1;
            frac |= 1u64 << i;
        }
    }

    // Result = int_part * 2^32 + frac (signed)
    Some((int_part << 32) + frac as i64)
}

/// Fixed-point log2(x) where x is Q32.32. Returns Q32.32 (unsigned, for x >= 1.0).
pub fn fp_log2(x: u64) -> Option<u64> {
    let signed = fp_log2_signed(x)?;
    if signed < 0 {
        Some(0)
    } else {
        Some(signed as u64)
    }
}

/// Fixed-point 2^x where x is Q32.32 (unsigned). Returns Q32.32.
pub fn fp_exp2(x: u64) -> Option<u64> {
    let int_part = (x >> 32) as u32;
    let frac = x & 0xFFFF_FFFF;

    // Polynomial coefficients for 2^frac, frac in [0,1):
    let c1: u64 = 2_977_044_472; // ln(2)
    let c2: u64 = 1_031_751_916; // ln(2)²/2
    let c3: u64 = 238_446_879;   // ln(2)³/6

    let mut result = FP_ONE;
    let mut term = fp_mul(frac, c3)?;
    term = term.checked_add(c2)?;
    term = fp_mul(frac, term)?;
    term = term.checked_add(c1)?;
    term = fp_mul(frac, term)?;
    result = result.checked_add(term)?;

    if int_part >= 32 {
        return None;
    }
    result.checked_shl(int_part)
}

/// Signed 2^x where x is Q32.32 signed. For negative x, 2^x < 1.0.
fn fp_exp2_signed(x: i64) -> Option<u64> {
    if x >= 0 {
        return fp_exp2(x as u64);
    }

    // For negative x: 2^x = 1 / 2^|x|
    let abs_x = (-x) as u64;
    let exp_abs = fp_exp2(abs_x)?;
    if exp_abs == 0 {
        return None;
    }
    // FP_ONE / exp_abs in Q32.32
    fp_div(FP_ONE, exp_abs)
}

/// x^w = exp2(w * log2(x)), both Q32.32 unsigned.
/// Works correctly for x < 1.0 (ratio in G3M swap).
pub fn fp_pow(base: u64, exp: u64) -> Option<u64> {
    if base == 0 { return Some(0); }
    if exp == 0 { return Some(FP_ONE); }
    if base == FP_ONE { return Some(FP_ONE); }

    // Use signed log to handle base < 1.0
    let log2_base = fp_log2_signed(base)?;

    // product = exp * log2_base (signed)
    // exp is unsigned Q32.32, log2_base is signed Q32.32
    let product = (exp as i128)
        .checked_mul(log2_base as i128)?;
    let product_q32 = (product >> 32) as i64;

    fp_exp2_signed(product_q32)
}

// ──────────────────── G3M Invariant & Swap ────────────────────

/// Compute G3M invariant: k = ∏ reserve_i^{weight_i}
/// reserves: raw token amounts (NOT fixed-point)
/// weights_bps: basis points (sum to 10_000)
/// Returns k as u128.
///
/// Precision: since Q32.32 can only represent integers in [0, 2^32),
/// reserves ≥ 2^32 cannot be expressed directly. The old code simply
/// right-shifted by `(msb - 32)` and called the result Q32.32 — that
/// collapsed the value into [0, 1) in fixed-point terms and produced an
/// invariant unrelated to the true reserves (a 2^40 reserve was treated
/// as 0.5, so every price / drift / rebalance check downstream was
/// wrong). Kidney flagged this in #33 for reserves ≥ 2^32.
///
/// Fix: extract a common scaling factor `S = 2^shift` across all
/// reserves so every `R_i / S < 2^32`, compute `k' = ∏ (R_i / S)^{w_i}`
/// in Q32.32, and rescale: since the weights sum to 1.0 (10_000 bps),
/// `k = k' * S`. This keeps fp_pow inputs in range without distorting
/// the invariant.
pub fn compute_invariant(
    reserves: &[u64],
    weights_bps: &[u16],
    token_count: usize,
) -> Option<u128> {
    // Any zero reserve makes the geometric mean zero — short-circuit.
    for i in 0..token_count {
        if reserves[i] == 0 {
            return Some(0);
        }
    }

    // Find the widest reserve so we know how far to scale down. MSB
    // position = 64 - leading_zeros. We need every (R_i >> shift) to
    // fit in 31 bits so `(R_i >> shift) << 32` stays inside u64.
    let max_bits = (0..token_count)
        .map(|i| 64 - reserves[i].leading_zeros())
        .max()
        .unwrap_or(0);
    let shift: u32 = if max_bits > 31 { max_bits - 31 } else { 0 };

    let mut k: u64 = FP_ONE;
    for i in 0..token_count {
        let r_scaled = reserves[i] >> shift;
        if r_scaled == 0 {
            // The common-factor rescale dropped this reserve below
            // representable precision. Return None rather than silently
            // producing a wrong invariant.
            return None;
        }
        let reserve_fp = r_scaled << 32;

        let weight_fp = ((weights_bps[i] as u64) << 32).checked_div(10_000)?;
        let pow_result = fp_pow(reserve_fp, weight_fp)?;
        k = fp_mul(k, pow_result)?;
    }

    // k is Q32.32 of `∏ (R_i / 2^shift)^{w_i}`. Since ∑ w_i = 1.0 when
    // weights are valid, the unscaled invariant is `k * 2^shift`. Widen
    // to u128 before the shift so the result doesn't overflow when the
    // caller is comparing invariants on very large pools.
    (k as u128).checked_shl(shift)
}

/// Compute swap output using G3M pricing.
///
/// For tokens in_idx -> out_idx:
///   R_out' = R_out * (R_in / (R_in + effective_in))^(w_in / w_out)
///   amount_out = R_out - R_out'
pub fn compute_swap_output(
    reserves: &[u64],
    weights_bps: &[u16],
    token_count: usize,
    in_idx: usize,
    out_idx: usize,
    amount_in: u64,
    fee_bps: u16,
) -> Option<u64> {
    if in_idx >= token_count || out_idx >= token_count || in_idx == out_idx {
        return None;
    }
    if amount_in == 0 || reserves[out_idx] == 0 {
        return None;
    }

    // Apply fee: effective_in = amount_in * (10_000 - fee_bps) / 10_000
    let effective_in = (amount_in as u128)
        .checked_mul(10_000u128.checked_sub(fee_bps as u128)?)?
        .checked_div(10_000)? as u64;

    let r_in = reserves[in_idx];
    let r_out = reserves[out_idx];
    let w_in = weights_bps[in_idx];
    let w_out = weights_bps[out_idx];

    // ratio = R_in / (R_in + effective_in) in Q32.32
    let new_r_in = r_in.checked_add(effective_in)?;

    // Use u128 to avoid overflow in division
    let ratio_fp = (((r_in as u128) << 32)
        .checked_div(new_r_in as u128)?) as u64;

    // exponent = w_in / w_out in Q32.32
    let exp_fp = ((w_in as u64) << 32)
        .checked_div(w_out as u64)?;

    // ratio^(w_in/w_out)
    let pow_result = fp_pow(ratio_fp, exp_fp)?;

    // R_out' = R_out * pow_result (in Q32.32)
    let r_out_fp = if r_out < (1u64 << 32) {
        r_out << 32
    } else {
        // Large reserve: compute in u128
        let result = ((r_out as u128) * (pow_result as u128)) >> 32;
        let new_r_out = (result >> 32) as u64;
        return r_out.checked_sub(new_r_out);
    };

    let new_r_out_fp = fp_mul(r_out_fp, pow_result)?;
    let new_r_out = fp_to_u64(new_r_out_fp);

    r_out.checked_sub(new_r_out)
}

// ────────────────────────── Tests ──────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fp_basics() {
        let two = fp_from_u64(2).unwrap();
        let three = fp_from_u64(3).unwrap();
        assert_eq!(fp_to_u64(fp_mul(two, three).unwrap()), 6);
        assert_eq!(fp_to_u64(fp_div(fp_from_u64(6).unwrap(), two).unwrap()), 3);
        assert!(fp_div(FP_ONE, 0).is_none());
    }

    #[test]
    fn test_equal_weight_swap() {
        // 50/50 pool, 1M each, swap 10k of token 0, 1% fee
        let reserves = [1_000_000u64, 1_000_000, 0, 0, 0];
        let weights = [5000u16, 5000, 0, 0, 0];
        let out = compute_swap_output(&reserves, &weights, 2, 0, 1, 10_000, 100).unwrap();
        // Should get ~9,800 (fee + price impact)
        assert!(out > 9_500 && out < 10_000, "Got: {}", out);
    }

    #[test]
    fn test_invariant() {
        let reserves = [1_000_000u64, 1_000_000, 0, 0, 0];
        let weights = [5000u16, 5000, 0, 0, 0];
        let k = compute_invariant(&reserves, &weights, 2).unwrap();
        assert!(k > 0, "k should be positive");
    }

    #[test]
    fn test_five_token_invariant() {
        // 5-token equal weight (ETF B spec)
        let reserves = [100_000u64, 100_000, 100_000, 100_000, 100_000];
        let weights = [2000u16, 2000, 2000, 2000, 2000];
        let k = compute_invariant(&reserves, &weights, 5).unwrap();
        assert!(k > 0, "5-token k should be positive");
    }

    #[test]
    fn test_swap_preserves_invariant() {
        let mut reserves = [1_000_000u64, 1_000_000, 0, 0, 0];
        let weights = [5000u16, 5000, 0, 0, 0];

        let k_before = compute_invariant(&reserves, &weights, 2).unwrap();
        let out = compute_swap_output(&reserves, &weights, 2, 0, 1, 10_000, 100).unwrap();

        reserves[0] += 10_000;
        reserves[1] -= out;
        let k_after = compute_invariant(&reserves, &weights, 2).unwrap();

        // k should increase (fee accrual) or stay same
        assert!(k_after >= k_before, "k decreased: {} -> {}", k_before, k_after);
    }

    // ───────── #33 regression: reserves ≥ 2^32 ─────────

    #[test]
    fn invariant_scales_with_equal_reserves_above_2_to_32() {
        // 50/50 pool, both reserves = 2^40. True invariant is exactly
        // 2^40 (∏ R_i^{w_i} with equal R_i and weights summing to 1).
        // The old code produced ~0.5 in fp terms for each reserve, so
        // k came back as roughly 0.5 << 32 instead of 2^40.
        let r: u64 = 1u64 << 40;
        let reserves = [r, r, 0, 0, 0];
        let weights = [5000u16, 5000, 0, 0, 0];
        let k = compute_invariant(&reserves, &weights, 2).expect("invariant");

        // k is Q32.32, so the real-number invariant is k >> 32.
        let k_int = (k >> 32) as u64;

        // Allow 0.1% error for the fractional log2/exp2 path.
        let tol = r / 1_000;
        assert!(
            k_int >= r.saturating_sub(tol) && k_int <= r.saturating_add(tol),
            "k_int {} not within 0.1% of {}",
            k_int,
            r
        );
    }

    #[test]
    fn invariant_monotonic_under_scaling() {
        // If every reserve doubles, invariant should also double
        // (∏ (2R_i)^{w_i} = 2^{∑w_i} ∏ R_i^{w_i} = 2 * k). The old
        // scaling bug broke this property for reserves ≥ 2^32.
        let weights = [3333u16, 3333, 3334, 0, 0];

        let reserves_small = [1_000_000u64, 2_000_000, 3_000_000, 0, 0];
        let k_small = compute_invariant(&reserves_small, &weights, 3).unwrap();

        let reserves_big = [
            1_000_000u64 << 20,
            2_000_000u64 << 20,
            3_000_000u64 << 20,
            0,
            0,
        ];
        let k_big = compute_invariant(&reserves_big, &weights, 3).unwrap();

        // k_big should be k_small * 2^20 (within fp rounding).
        let expected = k_small << 20;
        let tol = expected / 500; // 0.2%
        assert!(
            k_big >= expected.saturating_sub(tol) && k_big <= expected.saturating_add(tol),
            "k_big {} not within 0.2% of {}",
            k_big,
            expected
        );
    }

    #[test]
    fn invariant_rejects_unrepresentable_reserve_mix() {
        // One reserve at 2^40, another at 1. After common-factor shift
        // by (40 - 31) = 9, the small reserve rounds to 0 and the
        // invariant would be computed against a zero — return None
        // rather than a garbage value.
        let reserves = [1u64 << 40, 1, 0, 0, 0];
        let weights = [5000u16, 5000, 0, 0, 0];
        assert!(compute_invariant(&reserves, &weights, 2).is_none());
    }
}

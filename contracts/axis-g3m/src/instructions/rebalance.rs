use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

use crate::error::G3mError;
use crate::jupiter::{read_vault_balance, verify_jupiter_program};
use crate::math::compute_invariant;
use crate::state::G3mPoolState;

/// Maximum per-token reserve change in attestation mode (50% = 5000 bps).
/// Prevents a malicious authority from providing extreme attestation values
/// that shift one token dramatically while keeping global k within tolerance.
const MAX_ATTESTATION_RESERVE_CHANGE_BPS: u64 = 5000;

/// Rebalance — reset reserves to maintain G3M invariant when drift exceeds threshold.
///
/// Two modes:
///   1. **Trustless**: vault accounts provided → read SPL balances (ground truth).
///   2. **Attestation**: authority-signed reserve values in instruction data.
///      Attestation mode uses STRICTER checks (tighter invariant tolerance,
///      per-token reserve change cap) because the trust model is weaker.
///
/// Post-rebalance validation:
///   - Global k tolerance: 1% (trustless) / 0.5% (attestation)
///   - Per-token weight: each token's actual weight must be within drift_threshold
///   - Reserve change cap (attestation only): no token changes more than 50%
///
/// Accounts:
///   0: authority     (signer, must be pool authority)
///   1: pool_state    (writable, PDA)
///   2..2+N: vault token accounts (readable, to verify balances)
pub fn process_rebalance(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_reserves: &[u64],
) -> ProgramResult {
    let authority = &accounts[0];
    let pool_account = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify pool account is owned by this program (prevents spoofed accounts)
    if pool_account.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    // Read pool state
    let pool_data = pool_account.try_borrow_data()?;
    let pool = unsafe { &*(pool_data.as_ptr() as *const G3mPoolState) };

    if !pool.is_initialized() {
        return Err(G3mError::InvalidDiscriminator.into());
    }
    if pool.paused != 0 {
        return Err(G3mError::PoolPaused.into());
    }

    // Verify caller is the pool authority — prevents unauthorized rebalances
    if authority.key().as_ref() != &pool.authority {
        return Err(G3mError::Unauthorized.into());
    }

    let tc = pool.token_count as usize;

    // Enforce cooldown
    let current_slot = Clock::get()?.slot;
    let slots_since = current_slot.saturating_sub(pool.last_rebalance_slot);
    if slots_since < pool.rebalance_cooldown {
        return Err(G3mError::CooldownActive.into());
    }

    // Verify drift threshold is actually exceeded
    if !pool.needs_rebalance() {
        return Err(G3mError::DriftBelowThreshold.into());
    }

    let old_k = pool.invariant_k();

    // Save old reserves for per-token change validation
    let mut old_reserves = [0u64; 5];
    for i in 0..tc {
        old_reserves[i] = pool.reserves[i];
    }

    // Capture pre-rebalance max drift for metrics
    let mut pre_max_drift: u64 = 0;
    for i in 0..tc {
        if let Some(d) = pool.drift_bps(i) {
            if d > pre_max_drift {
                pre_max_drift = d;
            }
        }
    }

    // Determine mode: trustless (vaults provided) vs attestation (instruction data)
    let attestation_mode = accounts.len() < 2 + tc;

    // Determine new reserves
    let mut actual_reserves = [0u64; 5];

    if !attestation_mode {
        // Trustless mode: read vault balances
        for i in 0..tc {
            let vault_account = &accounts[2 + i];

            // Verify this vault matches the pool's recorded vault
            if vault_account.key().as_ref() != &pool.token_vaults[i] {
                return Err(G3mError::PoolMismatch.into());
            }

            actual_reserves[i] = read_vault_balance(vault_account)?;
        }
    } else if new_reserves.len() == tc {
        // Attestation mode: trust authority-provided reserves with stricter validation
        for i in 0..tc {
            actual_reserves[i] = new_reserves[i];
        }

        // === Attestation-mode per-token reserve change cap ===
        // Prevents extreme single-token manipulation via attested values.
        for i in 0..tc {
            if old_reserves[i] > 0 {
                let change = if actual_reserves[i] > old_reserves[i] {
                    actual_reserves[i] - old_reserves[i]
                } else {
                    old_reserves[i] - actual_reserves[i]
                };
                let change_bps = (change as u128)
                    .checked_mul(10_000)
                    .ok_or(ProgramError::from(G3mError::Overflow))?
                    / (old_reserves[i] as u128);
                if change_bps > MAX_ATTESTATION_RESERVE_CHANGE_BPS as u128 {
                    return Err(G3mError::ReserveChangeExceeded.into());
                }
            } else {
                // Zero-reserve token: reject attestation rebalance entirely.
                // A zero reserve could allow unbounded manipulation. The trustless
                // path (vault balances) should be used to recover from this state.
                return Err(G3mError::ZeroReserve.into());
            }
        }
    } else {
        return Err(G3mError::InvalidTokenCount.into());
    }

    // If a Jupiter program account is provided, validate it
    let jupiter_account_idx = 2 + tc;
    if accounts.len() > jupiter_account_idx {
        verify_jupiter_program(&accounts[jupiter_account_idx])?;
    }

    drop(pool_data);

    // Update reserves
    let data = pool_account.try_borrow_mut_data()?;
    let pool = unsafe { &mut *(data.as_ptr() as *mut G3mPoolState) };

    for i in 0..tc {
        pool.reserves[i] = actual_reserves[i];
    }

    // Compute new invariant
    let new_k = compute_invariant(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
    ).ok_or(ProgramError::from(G3mError::Overflow))?;

    let max_drift_bps = pool.max_invariant_drift_bps as u128;
    let min_k = old_k
        .checked_mul(10_000u128.saturating_sub(max_drift_bps))
        .ok_or(ProgramError::from(G3mError::Overflow))?
        .checked_div(10_000)
        .ok_or(ProgramError::from(G3mError::DivisionByZero))?;

    if new_k < min_k {
        return Err(G3mError::InvariantViolation.into());
    }

    // === Per-token weight validation ===
    // Global k tolerance alone is insufficient — individual tokens can have
    // extreme weight changes while maintaining aggregate k. Verify each
    // token's actual weight is within the drift threshold post-rebalance.
    for i in 0..tc {
        let target = pool.target_weights_bps[i] as u64;
        if target == 0 {
            continue;
        }
        let actual = pool.actual_weight_bps(i)
            .ok_or(ProgramError::from(G3mError::Overflow))?;
        let diff = if actual > target {
            actual - target
        } else {
            target - actual
        };
        let post_drift = diff
            .checked_mul(10_000)
            .ok_or(ProgramError::from(G3mError::Overflow))?
            .checked_div(target)
            .ok_or(ProgramError::from(G3mError::DivisionByZero))?;

        // Post-rebalance: each token's drift must be below the threshold.
        // The whole point of rebalancing is to bring weights back in line.
        if post_drift > pool.drift_threshold_bps as u64 {
            return Err(G3mError::PerTokenDriftExceeded.into());
        }
    }

    pool.set_invariant_k(new_k);
    pool.last_rebalance_slot = current_slot;

    // === Emit rebalance metrics via return_data ===
    // Layout (40 bytes):
    //   [0..16]:  old_k (u128 LE)
    //   [16..32]: new_k (u128 LE)
    //   [32..40]: pre_max_drift_bps (u64 LE)
    let mut return_buf = [0u8; 40];
    return_buf[0..16].copy_from_slice(&old_k.to_le_bytes());
    return_buf[16..32].copy_from_slice(&new_k.to_le_bytes());
    return_buf[32..40].copy_from_slice(&pre_max_drift.to_le_bytes());
    pinocchio::program::set_return_data(&return_buf);

    Ok(())
}

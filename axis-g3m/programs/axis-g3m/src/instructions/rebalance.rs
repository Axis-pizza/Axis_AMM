use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

use crate::error::G3mError;
use crate::jupiter::read_vault_balance;
use crate::math::compute_invariant;
use crate::state::G3mPoolState;

/// Rebalance — current rehearsal/state-sync path when drift exceeds threshold.
///
/// This instruction does not execute a Jupiter CPI today. It only supports
/// updating reserves from either:
///   1. the actual SPL vault balances passed in the accounts list, or
///   2. caller-supplied reserve values used by the current rehearsal scripts.
///
/// A future spec-aligned implementation should replace the fallback attestation
/// path with a same-transaction Jupiter CPI plus post-swap vault verification.
///
/// Accounts:
///   0: authority     (signer, must be pool authority)
///   1: pool_state    (writable, PDA)
///   2..2+N: vault token accounts (readable, to verify balances)
///
/// Instruction data (after 1-byte discriminant): none needed
/// (reserves are read from vault accounts, not from instruction data)
pub fn process_rebalance(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_reserves: &[u64],
) -> ProgramResult {
    let authority = &accounts[0];
    let pool_account = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
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

    // Verify authority
    if authority.key().as_ref() != &pool.authority {
        return Err(G3mError::OwnerMismatch.into());
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

    // Determine new reserves:
    // If vault accounts are provided (accounts.len() >= 2 + tc),
    // read actual balances from the SPL token accounts (trustless).
    // Otherwise, use the reserves from instruction data (authority attestation).
    let mut actual_reserves = [0u64; 5];

    if accounts.len() >= 2 + tc {
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
        // Attestation mode (fallback): trust authority-provided reserves
        for i in 0..tc {
            actual_reserves[i] = new_reserves[i];
        }
    } else {
        return Err(G3mError::InvalidTokenCount.into());
    }

    drop(pool_data);

    // Update reserves
    let data = pool_account.try_borrow_mut_data()?;
    let pool = unsafe { &mut *(data.as_ptr() as *mut G3mPoolState) };

    for i in 0..tc {
        pool.reserves[i] = actual_reserves[i];
    }

    // Verify invariant within tolerance (allow up to 1% decrease from slippage)
    let new_k = compute_invariant(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
    ).ok_or(ProgramError::from(G3mError::Overflow))?;

    let min_k = old_k
        .checked_mul(99)
        .ok_or(ProgramError::from(G3mError::Overflow))?
        .checked_div(100)
        .ok_or(ProgramError::from(G3mError::DivisionByZero))?;

    if new_k < min_k {
        return Err(G3mError::InvariantViolation.into());
    }

    pool.set_invariant_k(new_k);
    pool.last_rebalance_slot = current_slot;

    Ok(())
}

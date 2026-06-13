use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::error::G3mError;
use crate::state::G3mPoolState;

/// CheckDrift — read-only instruction that returns structured drift metrics.
/// Complexity: O(n_tokens) where n <= 5.
///
/// Accounts:
///   0: pool_state (read-only, PDA)
///
/// No instruction data beyond discriminant.
///
/// Return data layout (20 bytes):
///   [0..8]:   max_drift_bps (u64 LE)
///   [8]:      max_drift_token_index (u8)
///   [9..11]:  threshold_bps (u16 LE)
///   [11]:     needs_rebalance (u8, 0 or 1)
///   [12..20]: invariant_k_lo (u64 LE) — low 8 bytes of G3M invariant
pub fn process_check_drift(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let pool_account = &accounts[0];

    let pool_data = pool_account.try_borrow_data()?;
    let pool = unsafe { &*(pool_data.as_ptr() as *const G3mPoolState) };

    if !pool.is_initialized() {
        return Err(G3mError::InvalidDiscriminator.into());
    }

    let tc = pool.token_count as usize;
    let mut max_drift: u64 = 0;
    let mut max_drift_idx: u8 = 0;

    for i in 0..tc {
        let drift = pool.drift_bps(i)
            .ok_or(ProgramError::from(G3mError::Overflow))?;

        if drift > max_drift {
            max_drift = drift;
            max_drift_idx = i as u8;
        }
    }

    let needs_rebalance = max_drift > pool.drift_threshold_bps as u64;

    // Emit structured return data for client parsing.
    // Layout: [max_drift_bps: u64][idx: u8][threshold_bps: u16][needs_rebalance: u8][k_lo: u64]
    let mut return_buf = [0u8; 20];
    return_buf[0..8].copy_from_slice(&max_drift.to_le_bytes());
    return_buf[8] = max_drift_idx;
    return_buf[9..11].copy_from_slice(&pool.drift_threshold_bps.to_le_bytes());
    return_buf[11] = needs_rebalance as u8;
    return_buf[12..20].copy_from_slice(&pool.invariant_k_lo.to_le_bytes());

    // set_return_data via Solana syscall
    pinocchio::program::set_return_data(&return_buf);

    Ok(())
}

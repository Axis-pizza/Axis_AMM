use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::error::G3mError;
use crate::state::G3mPoolState;

/// SetPaused — authority-gated toggle of `pool.paused` on an axis-g3m
/// pool (#59). The pool already carries a `paused` byte used by Swap /
/// Rebalance to short-circuit during outages; without this ix there
/// was no way to flip it post-deploy.
///
/// Accounts:
///   0: [signer]   authority (must equal pool.authority)
///   1: [writable] pool PDA
///
/// Data: [paused: u8]  (any non-zero value normalizes to 1)
pub fn process_set_paused(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    paused: u8,
) -> ProgramResult {
    let [authority, pool_ai, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if pool_ai.owner() != program_id {
        return Err(G3mError::InvalidDiscriminator.into());
    }

    let stored_authority = {
        let data = pool_ai.try_borrow_data()?;
        if data.len() < core::mem::size_of::<G3mPoolState>() {
            return Err(ProgramError::InvalidAccountData);
        }
        let pool = unsafe { &*(data.as_ptr() as *const G3mPoolState) };
        if !pool.is_initialized() {
            return Err(G3mError::InvalidDiscriminator.into());
        }
        pool.authority
    };

    if authority.key().as_ref() != &stored_authority {
        return Err(G3mError::Unauthorized.into());
    }

    let normalized = if paused == 0 { 0 } else { 1 };
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        if data.len() < core::mem::size_of::<G3mPoolState>() {
            return Err(ProgramError::InvalidAccountData);
        }
        let pool = unsafe { &mut *(data.as_mut_ptr() as *mut G3mPoolState) };
        pool.paused = normalized;
    }

    Ok(())
}

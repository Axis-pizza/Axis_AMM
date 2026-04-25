use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, PoolState3};

/// SetPaused3 — flip `pool.paused` on a pfda-amm-3 pool (#59).
///
/// Authority-gated: only the account equal to `pool.authority` may call
/// this. Mirrors the pattern established for axis-vault in PR #51 and
/// pfda-amm at instruction discriminator 6. Without this ix, a pfda-amm-3
/// pool that went live could not be emergency-stopped — the `paused`
/// field existed on PoolState3 but nothing could mutate it.
///
/// Accounts:
///   0: [signer]   authority (must equal pool.authority)
///   1: [writable] pool_state PDA
///
/// Data: [paused: u8]  (any non-zero value normalizes to 1)
pub fn process_set_paused_3(
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
        return Err(Pfda3Error::InvalidDiscriminator.into());
    }

    let stored_authority = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        pool.authority
    };

    if authority.key().as_ref() != &stored_authority {
        return Err(Pfda3Error::Unauthorized.into());
    }

    let normalized = if paused == 0 { 0 } else { 1 };
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.paused = normalized;
    }

    Ok(())
}

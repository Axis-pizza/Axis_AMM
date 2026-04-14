use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::{
    error::PfmmError,
    state::{load_mut, PoolState},
};

/// Accounts for SetPaused:
/// 0. `[signer]`    authority
/// 1. `[writable]`  pool_state PDA
pub fn process_set_paused(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    paused: bool,
) -> ProgramResult {
    let [authority, pool_state_ai, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut data = pool_state_ai.try_borrow_mut_data()?;
    let pool = unsafe { load_mut::<PoolState>(&mut data) }
        .ok_or(ProgramError::InvalidAccountData)?;

    if !pool.is_initialized() {
        return Err(PfmmError::InvalidDiscriminator.into());
    }

    if authority.key() != &pool.authority {
        return Err(PfmmError::Unauthorized.into());
    }

    let (expected, _) = pubkey::find_program_address(
        &[b"pool", &pool.token_a_mint, &pool.token_b_mint],
        program_id,
    );
    if pool_state_ai.key() != &expected {
        return Err(ProgramError::InvalidSeeds);
    }

    pool.paused = if paused { 1 } else { 0 };

    Ok(())
}

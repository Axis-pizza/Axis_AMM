use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load, PoolState3};

/// WithdrawFees — authority withdraws accumulated protocol fees from vaults.
///
/// Fees accumulate in vaults as the difference between deposits and claims.
/// Only the pool authority can withdraw. Withdrawals go to the treasury.
///
/// Accounts:
///   0: [signer]   authority (must match pool.authority)
///   1: []          pool_state PDA
///   2..2+N: [writable] vault token accounts
///   5..5+N: [writable] treasury token accounts (destinations)
///   8: []          token_program
///
/// Data: [amounts: [u64; 3]] — how much to withdraw from each vault
pub fn process_withdraw_fees(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    amounts: [u64; 3],
) -> ProgramResult {
    let authority = &accounts[0];
    let pool_ai = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Validate authority
    let (pool_key, pool_authority, token_mints, bump) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { crate::state::load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if authority.key().as_ref() != &pool.authority {
            return Err(Pfda3Error::OwnerMismatch.into());
        }
        (*pool_ai.key(), pool.authority, pool.token_mints, pool.bump)
    };

    // Read mint keys for PDA signer seeds
    let mints = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { crate::state::load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.token_mints
    };

    let bump_bytes = [bump];
    let pool_signer_seeds = [
        Seed::from(b"pool3".as_ref()),
        Seed::from(mints[0].as_ref()),
        Seed::from(mints[1].as_ref()),
        Seed::from(mints[2].as_ref()),
        Seed::from(bump_bytes.as_ref()),
    ];

    for i in 0..3 {
        if amounts[i] > 0 {
            let vault = &accounts[2 + i];
            let treasury_token = &accounts[5 + i];

            Transfer {
                from: vault,
                to: treasury_token,
                authority: pool_ai,
                amount: amounts[i],
            }
            .invoke_signed(&[Signer::from(&pool_signer_seeds)])?;
        }
    }

    Ok(())
}

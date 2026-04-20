use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load_mut, PoolState3};

/// WithdrawFees — authority withdraws accumulated protocol fees from vaults.
///
/// Fees accumulate in vaults as the difference between deposits and claims.
/// Only the pool authority can withdraw. Withdrawals go to the treasury.
///
/// Accounts:
///   0: [signer]   authority (must match pool.authority)
///   1: [writable]  pool_state PDA
///   2..2+N: [writable] vault token accounts
///   5..5+N: [writable] treasury token accounts (destinations)
///   8: []          token_program
///
/// Data: [amounts: [u64; 3]] — how much to withdraw from each vault
///
/// Accounting: kidneyweakx flagged in #33 that the original version
/// transferred tokens out of the vaults without decrementing
/// `pool.reserves[i]`. Every subsequent `ClearBatch` / `SwapRequest`
/// then priced against a reserve snapshot that no longer matched the
/// real vault balance — leading to oversold clearings and eventual
/// transfer failures once the real vault ran out.
///
/// Fix: decrement `pool.reserves[i]` by `amounts[i]` in the same
/// instruction, with an `InsufficientBalance` guard up-front so the
/// authority can't accidentally under-flow the accounting.
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

    // Validate authority + capture the values we need for CPIs. We
    // also pre-check that every requested withdrawal fits inside the
    // tracked reserves so we don't start transferring before we know
    // the whole batch can succeed.
    let (mints, bump) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { crate::state::load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if authority.key().as_ref() != &pool.authority {
            return Err(Pfda3Error::OwnerMismatch.into());
        }
        for i in 0..3 {
            if amounts[i] > pool.reserves[i] {
                return Err(Pfda3Error::FeeWithdrawExceedsReserves.into());
            }
        }
        (pool.token_mints, pool.bump)
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

    // Decrement tracked reserves so subsequent ClearBatch / SwapRequest
    // price against the real post-withdrawal vault balance.
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        for i in 0..3 {
            // checked_sub is belt-and-braces: we already validated the
            // amount above, but the mutable borrow is a new read so
            // guard against a concurrent-modification false assumption.
            pool.reserves[i] = pool.reserves[i]
                .checked_sub(amounts[i])
                .ok_or(Pfda3Error::FeeWithdrawExceedsReserves)?;
        }
    }

    Ok(())
}

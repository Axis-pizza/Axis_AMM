use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, PoolState3};

/// AddLiquidity3 — deposit tokens into pool vaults and update reserves.
///
/// Accounts:
/// 0: [signer, writable] user
/// 1: [writable]          pool_state PDA
/// 2: [writable]          vault_0
/// 3: [writable]          vault_1
/// 4: [writable]          vault_2
/// 5: [writable]          user_token_0
/// 6: [writable]          user_token_1
/// 7: [writable]          user_token_2
/// 8: []                  token_program
///
/// Data: [amount_0: u64 LE][amount_1: u64 LE][amount_2: u64 LE]
pub fn process_add_liquidity_3(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    amounts: [u64; 3],
) -> ProgramResult {
    let [user, pool_ai, vault0, vault1, vault2, ut0, ut1, ut2, _tok, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
    }

    let vaults = [vault0, vault1, vault2];
    let user_tokens = [ut0, ut1, ut2];

    for i in 0..3 {
        if amounts[i] > 0 {
            Transfer {
                from: user_tokens[i],
                to: vaults[i],
                authority: user,
                amount: amounts[i],
            }
            .invoke()?;
        }
    }

    // Update reserves
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        for i in 0..3 {
            pool.reserves[i] = pool.reserves[i]
                .checked_add(amounts[i])
                .ok_or(Pfda3Error::Overflow)?;
        }
    }

    Ok(())
}

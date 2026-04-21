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

    // #33: the original version skipped paused / reentrancy / vault
    // cross-checks. Gate on all three so AddLiquidity shares the same
    // safety posture as SwapRequest.
    let pool_vaults = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if pool.paused != 0 {
            return Err(Pfda3Error::PoolPaused.into());
        }
        if pool.reentrancy_guard != 0 {
            return Err(Pfda3Error::ReentrancyDetected.into());
        }
        pool.vaults
    };

    let vaults = [vault0, vault1, vault2];
    let user_tokens = [ut0, ut1, ut2];

    // Vault mismatch check: every passed-in vault account must equal
    // the pool's stored vault for that index. Otherwise a user could
    // deposit into an attacker-controlled ATA of the right mint.
    for i in 0..3 {
        if vaults[i].key() != &pool_vaults[i] {
            return Err(Pfda3Error::VaultMismatch.into());
        }
    }

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

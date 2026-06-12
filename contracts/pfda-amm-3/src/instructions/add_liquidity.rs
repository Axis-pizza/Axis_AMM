use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, PoolState3};

/// AddLiquidity3 — AUTHORITY-ONLY seed / top-up of pool vaults + reserves.
///
/// There is no LP-share accounting, so liquidity added here cannot be
/// redeemed by the caller; it is operator-supplied pool liquidity. Gated to
/// `pool.authority` (H2) — a non-authority caller would lose their tokens and
/// could also use direct reserve writes to skew clearing prices.
///
/// Accounts:
/// 0: [signer, writable] authority (must equal pool.authority)
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
    program_id: &Pubkey,
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

    // H2 (no LP accounting): AddLiquidity mints no LP shares and creates no
    // claim ticket — liquidity added here is recoverable only through the
    // pool's own fee/clearing rails, and writing `reserves` directly outside a
    // batch is also a clearing-price-manipulation primitive. A non-authority
    // caller would simply lose their tokens. Restrict the whole instruction to
    // the pool authority: this is a seed / top-up-liquidity operation for the
    // pool operator, NOT a user-facing deposit.
    //
    // #59: pool_ai feeds both the authority check and the vaults[i] match, so
    // it must be program-owned (no fake-pool substitution).
    if pool_ai.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    // #33: gate on paused / reentrancy / vault cross-checks so AddLiquidity
    // shares the same safety posture as SwapRequest.
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
        // H2: authority-only seeding.
        if user.key().as_ref() != &pool.authority {
            return Err(Pfda3Error::Unauthorized.into());
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

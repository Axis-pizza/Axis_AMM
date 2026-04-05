use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, ClearedBatchHistory3, PoolState3, UserOrderTicket3};

/// Claim3 — O(1) proportional withdrawal for 3-token pool.
///
/// Accounts:
/// 0: [signer]    user
/// 1: []           pool_state PDA
/// 2: []           history PDA
/// 3: [writable]   ticket PDA
/// 4: [writable]   vault_0
/// 5: [writable]   vault_1
/// 6: [writable]   vault_2
/// 7: [writable]   user_token_0
/// 8: [writable]   user_token_1
/// 9: [writable]   user_token_2
/// 10: []          token_program
pub fn process_claim_3(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let [user, pool_ai, history_ai, ticket_ai,
         vault0, vault1, vault2,
         user_tok0, user_tok1, user_tok2,
         _tok, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Load pool for PDA signer seeds
    let (pool_key, pool_bump, token_mints) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        (*pool_ai.key(), pool.bump, pool.token_mints)
    };

    // Load history
    let (batch_id, clearing_prices, total_in, total_out, fee_bps) = {
        let data = history_ai.try_borrow_data()?;
        let hist = unsafe { load::<ClearedBatchHistory3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !hist.is_initialized() || !hist.is_cleared {
            return Err(Pfda3Error::BatchNotCleared.into());
        }
        if &hist.pool != pool_key.as_ref() {
            return Err(Pfda3Error::PoolMismatch.into());
        }
        (hist.batch_id, hist.clearing_prices, hist.total_in, hist.total_out, hist.fee_bps)
    };

    // Load ticket
    let (in_idx, amount_in, out_idx, min_out) = {
        let data = ticket_ai.try_borrow_data()?;
        let ticket = unsafe { load::<UserOrderTicket3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !ticket.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if ticket.is_claimed {
            return Err(Pfda3Error::TicketAlreadyClaimed.into());
        }
        if ticket.batch_id != batch_id {
            return Err(Pfda3Error::BatchIdMismatch.into());
        }
        if &ticket.owner != user.key().as_ref() {
            return Err(Pfda3Error::OwnerMismatch.into());
        }

        // Find which token was deposited
        let mut found_idx = 0u8;
        let mut found_amount = 0u64;
        for i in 0..3 {
            if ticket.amounts_in[i] > 0 {
                found_idx = i as u8;
                found_amount = ticket.amounts_in[i];
                break;
            }
        }

        (found_idx, found_amount, ticket.out_token_idx, ticket.min_amount_out)
    };

    // Compute output amount using clearing prices.
    // The clearing prices are Q32.32 prices of each token in numeraire terms.
    // Exchange rate from token in → token out:
    //   rate = clearing_price[in] / clearing_price[out]
    // amount_out = amount_in * rate (already includes fee from clearing)
    let in_i = in_idx as usize;
    let out_i = out_idx as usize;

    if total_in[in_i] == 0 {
        let mut data = ticket_ai.try_borrow_mut_data()?;
        let ticket = unsafe { load_mut::<UserOrderTicket3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        ticket.is_claimed = true;
        return Ok(());
    }

    // amount_out = amount_in * price_in / price_out * (1 - fee)
    // Prices are RAW Q32.32 (no fee baked in). Fee applied here.
    let price_in = clearing_prices[in_i] as u128;
    let price_out = clearing_prices[out_i] as u128;
    if price_in == 0 || price_out == 0 {
        return Err(Pfda3Error::DivisionByZero.into());
    }
    let fee_factor = (10_000u128).checked_sub(fee_bps as u128)
        .ok_or(Pfda3Error::Overflow)?;
    let raw_out = (amount_in as u128)
        .checked_mul(price_in).ok_or(Pfda3Error::Overflow)?
        .checked_div(price_out).ok_or(Pfda3Error::DivisionByZero)?;
    let amount_out = (raw_out.checked_mul(fee_factor).ok_or(Pfda3Error::Overflow)?
        .checked_div(10_000).ok_or(Pfda3Error::DivisionByZero)?) as u64;

    if amount_out < min_out {
        return Err(Pfda3Error::SlippageExceeded.into());
    }

    // Transfer from vault to user
    let vault_accounts = [vault0, vault1, vault2];
    let user_accounts = [user_tok0, user_tok1, user_tok2];

    let pool_bump_seed = [pool_bump];
    let pool_signer = [
        Seed::from(b"pool3".as_ref()),
        Seed::from(token_mints[0].as_ref()),
        Seed::from(token_mints[1].as_ref()),
        Seed::from(token_mints[2].as_ref()),
        Seed::from(pool_bump_seed.as_ref()),
    ];

    if amount_out > 0 {
        Transfer {
            from: vault_accounts[out_i],
            to: user_accounts[out_i],
            authority: pool_ai,
            amount: amount_out,
        }
        .invoke_signed(&[Signer::from(&pool_signer)])?;
    }

    // Mark claimed
    {
        let mut data = ticket_ai.try_borrow_mut_data()?;
        let ticket = unsafe { load_mut::<UserOrderTicket3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        ticket.is_claimed = true;
    }

    Ok(())
}

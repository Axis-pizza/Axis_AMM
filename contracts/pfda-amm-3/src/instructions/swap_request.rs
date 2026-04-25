use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::Transfer;

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, BatchQueue3, PoolState3, UserOrderTicket3};

/// Accounts:
/// 0: [signer, writable] user
/// 1: []                  pool_state PDA
/// 2: [writable]          batch_queue PDA
/// 3: [writable]          user_order_ticket PDA (new)
/// 4: [writable]          user_token_account (source)
/// 5: [writable]          vault (destination — vault for the input token)
/// 6: []                  token_program
/// 7: []                  system_program
///
/// Data: [in_token_idx: u8][amount_in: u64][out_token_idx: u8][min_out: u64]
pub fn process_swap_request_3(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    in_token_idx: u8,
    amount_in: u64,
    out_token_idx: u8,
    min_amount_out: u64,
) -> ProgramResult {
    if in_token_idx >= 3 || out_token_idx >= 3 || in_token_idx == out_token_idx {
        return Err(Pfda3Error::InvalidTokenIndex.into());
    }
    if amount_in == 0 {
        return Err(Pfda3Error::InvalidSwapInput.into());
    }

    let [user, pool_ai, queue_ai, ticket_ai, user_token, vault, _tok, _sys, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // #59 round 2: the vault-key check below assumes pool_ai is a
    // real pool owned by this program. Without an explicit owner
    // check, an attacker (or malicious frontend) could supply a
    // forged pool_ai whose data carries the right discriminator
    // and an attacker-controlled value in pool.vaults[in_token_idx],
    // pass a matching `vault` AccountInfo, and the equality check
    // would pass while user tokens flowed to the attacker's vault.
    // Same pattern as close_batch_history.rs and set_paused.rs.
    if pool_ai.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    // Load pool
    let (pool_key, current_batch_id, current_window_end) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if pool.reentrancy_guard != 0 {
            return Err(Pfda3Error::ReentrancyDetected.into());
        }
        if pool.paused != 0 {
            // #33: was InvalidDiscriminator — mis-surfacing a pause as a
            // discriminator error confused operators in logs. Return
            // the dedicated PoolPaused code instead.
            return Err(Pfda3Error::PoolPaused.into());
        }
        // #59: previously this only checked the vault mint via
        // verify_token_account_mint, which left SwapRequest open to
        // vault-key spoofing — a crafted token account of the right
        // mint could be passed in as the destination and user tokens
        // would land there while the batch queue still incremented.
        // Assert the key equals the pool's stored vault for this token
        // index before any Transfer runs.
        if vault.key().as_ref() != &pool.vaults[in_token_idx as usize] {
            return Err(Pfda3Error::VaultMismatch.into());
        }
        // Security rule 7: verify vault mint matches pool token
        crate::security::verify_token_account_mint(vault, &pool.token_mints[in_token_idx as usize])?;
        (*pool_ai.key(), pool.current_batch_id, pool.current_window_end)
    };

    // Check within window
    let current_slot = Clock::get()?.slot;
    if current_slot > current_window_end {
        return Err(Pfda3Error::BatchWindowNotEnded.into());
    }

    // Validate queue PDA
    let batch_id_bytes = current_batch_id.to_le_bytes();
    let (expected_queue, _) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if queue_ai.key() != &expected_queue {
        return Err(ProgramError::InvalidSeeds);
    }

    // Transfer tokens to vault
    Transfer {
        from: user_token,
        to: vault,
        authority: user,
        amount: amount_in,
    }
    .invoke()?;

    // Update batch queue (O(1))
    {
        let mut data = queue_ai.try_borrow_mut_data()?;
        let queue = unsafe { load_mut::<BatchQueue3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !queue.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        queue.total_in[in_token_idx as usize] = queue.total_in[in_token_idx as usize]
            .checked_add(amount_in)
            .ok_or(Pfda3Error::Overflow)?;
    }

    // Create ticket PDA
    let user_key = user.key();
    let (expected_ticket, ticket_bump) = pubkey::find_program_address(
        &[b"ticket3", &pool_key, user_key, &batch_id_bytes],
        program_id,
    );
    if ticket_ai.key() != &expected_ticket {
        return Err(ProgramError::InvalidSeeds);
    }

    let rent = Rent::get()?;
    let ticket_bump_seed = [ticket_bump];
    let ticket_signer = [
        Seed::from(b"ticket3".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(user_key.as_ref()),
        Seed::from(batch_id_bytes.as_ref()),
        Seed::from(ticket_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: user,
        to: ticket_ai,
        lamports: rent.minimum_balance(UserOrderTicket3::LEN),
        space: UserOrderTicket3::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&ticket_signer)])?;

    {
        let mut data = ticket_ai.try_borrow_mut_data()?;
        let ticket = unsafe { load_mut::<UserOrderTicket3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;

        ticket.discriminator = UserOrderTicket3::DISCRIMINATOR;
        ticket.owner = *user_key;
        ticket.pool = pool_key;
        ticket.batch_id = current_batch_id;
        ticket.amounts_in = [0; 3];
        ticket.amounts_in[in_token_idx as usize] = amount_in;
        ticket.out_token_idx = out_token_idx;
        ticket.min_amount_out = min_amount_out;
        ticket.is_claimed = false;
        ticket.bump = ticket_bump;
        ticket._padding = [0; 5];
    }

    Ok(())
}

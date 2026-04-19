use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::error::Pfda3Error;
use crate::state::{load, PoolState3, UserOrderTicket3};

/// Number of batches after which an unclaimed ticket can be closed.
const TICKET_EXPIRY_BATCHES: u64 = 200;

/// CloseExpiredTicket — reclaim rent from an expired, unclaimed UserOrderTicket3.
///
/// The rent is returned to the original ticket owner (rent_recipient must match).
///
/// Accounts:
/// 0: [signer]   caller (anyone can crank this)
/// 1: []          pool_state PDA
/// 2: [writable]  ticket PDA (to be closed)
/// 3: [writable]  rent_recipient (must be the original ticket owner)
pub fn process_close_expired_ticket_3(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let [caller, pool_ai, ticket_ai, rent_recipient, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !caller.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Read current_batch_id from pool
    let current_batch_id = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        pool.current_batch_id
    };

    // Read ticket data
    let (ticket_owner, ticket_batch_id) = {
        let data = ticket_ai.try_borrow_data()?;
        let ticket = unsafe { load::<UserOrderTicket3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !ticket.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        (ticket.owner, ticket.batch_id)
    };

    // Verify PDA derivation
    let batch_id_bytes = ticket_batch_id.to_le_bytes();
    let (expected_ticket, _) = pubkey::find_program_address(
        &[b"ticket3", pool_ai.key().as_ref(), &ticket_owner, &batch_id_bytes],
        program_id,
    );
    if ticket_ai.key() != &expected_ticket {
        return Err(ProgramError::InvalidSeeds);
    }

    // Verify rent_recipient is the original ticket owner
    if rent_recipient.key().as_ref() != &ticket_owner {
        return Err(Pfda3Error::OwnerMismatch.into());
    }

    // Enforce ticket expiry
    if current_batch_id < ticket_batch_id.saturating_add(TICKET_EXPIRY_BATCHES) {
        return Err(Pfda3Error::BatchWindowNotEnded.into());
    }

    // Close the ticket account: transfer lamports to original owner, zero data
    let ticket_lamports = ticket_ai.lamports();
    unsafe {
        *ticket_ai.borrow_mut_lamports_unchecked() = 0;
    }
    unsafe {
        *rent_recipient.borrow_mut_lamports_unchecked() =
            rent_recipient.lamports().checked_add(ticket_lamports)
                .ok_or(Pfda3Error::Overflow)?;
    }
    let mut data = ticket_ai.try_borrow_mut_data()?;
    data.fill(0);

    Ok(())
}

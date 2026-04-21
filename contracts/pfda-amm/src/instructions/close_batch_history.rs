use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::error::PfmmError;
use crate::state::{load, ClearedBatchHistory, PoolState};

/// Minimum number of batches that must elapse before a history PDA can be closed.
const CLOSE_DELAY: u64 = 100;

/// CloseBatchHistory — reclaim rent from an old ClearedBatchHistory PDA.
///
/// Accounts:
/// 0: [signer]   rent_recipient
/// 1: []          pool_state PDA
/// 2: [writable]  history PDA (to be closed)
pub fn process_close_batch_history(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let [rent_recipient, pool_ai, history_ai, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !rent_recipient.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // #33 flagged that `pool_ai` was never re-derived as a PDA and
    // `history.pool` was never compared to `pool_ai.key()`. An attacker
    // could hand in a fake program-owned account whose `current_batch_id`
    // makes the close-delay already look elapsed, then close a real
    // history account siphoned from an unrelated pool. Verify pool
    // ownership + PDA seeds, and require the stored `history.pool` to
    // match.
    if pool_ai.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    // Read current_batch_id from pool and verify the pool itself is
    // a canonical PDA derived from its own recorded mint pair.
    let current_batch_id = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(PfmmError::InvalidDiscriminator.into());
        }
        let (expected_pool, _) = pubkey::find_program_address(
            &[b"pool", &pool.token_a_mint, &pool.token_b_mint],
            program_id,
        );
        if pool_ai.key() != &expected_pool {
            return Err(ProgramError::InvalidSeeds);
        }
        pool.current_batch_id
    };

    // Read batch_id from history and verify its stored pool matches
    // the pool_ai we just validated.
    let history_batch_id = {
        let data = history_ai.try_borrow_data()?;
        let hist = unsafe { load::<ClearedBatchHistory>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !hist.is_initialized() {
            return Err(PfmmError::InvalidDiscriminator.into());
        }
        if &hist.pool != pool_ai.key().as_ref() {
            return Err(PfmmError::PoolMismatch.into());
        }
        hist.batch_id
    };

    // Verify PDA derivation
    let batch_id_bytes = history_batch_id.to_le_bytes();
    let (expected_history, _) = pubkey::find_program_address(
        &[b"history", pool_ai.key().as_ref(), &batch_id_bytes],
        program_id,
    );
    if history_ai.key() != &expected_history {
        return Err(ProgramError::InvalidSeeds);
    }

    // Enforce close delay
    if current_batch_id < history_batch_id.saturating_add(CLOSE_DELAY) {
        return Err(PfmmError::BatchWindowNotEnded.into());
    }

    // Close the history account: transfer lamports, zero data
    let history_lamports = history_ai.lamports();
    // Subtract from history
    unsafe {
        *history_ai.borrow_mut_lamports_unchecked() = 0;
    }
    // Add to rent_recipient
    unsafe {
        *rent_recipient.borrow_mut_lamports_unchecked() =
            rent_recipient.lamports().checked_add(history_lamports)
                .ok_or(PfmmError::Overflow)?;
    }
    // Zero account data
    let mut data = history_ai.try_borrow_mut_data()?;
    data.fill(0);

    Ok(())
}

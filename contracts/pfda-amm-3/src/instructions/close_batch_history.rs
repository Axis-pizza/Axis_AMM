use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::error::Pfda3Error;
use crate::state::{load, ClearedBatchHistory3, PoolState3};

/// Minimum number of batches that must elapse before a history PDA can be closed.
const CLOSE_DELAY: u64 = 100;

/// CloseBatchHistory — reclaim rent from an old ClearedBatchHistory3 PDA.
///
/// Accounts:
/// 0: [signer]   rent_recipient
/// 1: []          pool_state PDA
/// 2: [writable]  history PDA (to be closed)
pub fn process_close_batch_history_3(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let [rent_recipient, pool_ai, history_ai, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !rent_recipient.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // #33: pool_ai was trusted blindly — no program-ownership check,
    // no PDA re-derivation, and `history.pool` was never compared to
    // it. That allowed an attacker to feed in a fake pool with a
    // cooked current_batch_id (to bypass CLOSE_DELAY) and close any
    // real history PDA whose own pool-seed happened to match. Verify
    // pool ownership, re-derive its canonical PDA, require
    // hist.pool == pool_ai.key, and restrict rent_recipient to the
    // pool authority so the reclaimed lamports cannot be siphoned.
    if pool_ai.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    let (current_batch_id, pool_authority) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        let (expected_pool, _) = pubkey::find_program_address(
            &[
                b"pool3",
                &pool.token_mints[0],
                &pool.token_mints[1],
                &pool.token_mints[2],
            ],
            program_id,
        );
        if pool_ai.key() != &expected_pool {
            return Err(ProgramError::InvalidSeeds);
        }
        (pool.current_batch_id, pool.authority)
    };

    // Rent recipient must be the pool authority — otherwise anyone who
    // wins the close-delay race could pocket the reclaimed rent that
    // the pool creator paid.
    if rent_recipient.key().as_ref() != &pool_authority {
        return Err(Pfda3Error::Unauthorized.into());
    }

    // Read batch_id from history and verify stored pool matches.
    let history_batch_id = {
        let data = history_ai.try_borrow_data()?;
        let hist = unsafe { load::<ClearedBatchHistory3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !hist.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if &hist.pool != pool_ai.key().as_ref() {
            return Err(Pfda3Error::PoolMismatch.into());
        }
        hist.batch_id
    };

    // Verify PDA derivation
    let batch_id_bytes = history_batch_id.to_le_bytes();
    let (expected_history, _) = pubkey::find_program_address(
        &[b"history3", pool_ai.key().as_ref(), &batch_id_bytes],
        program_id,
    );
    if history_ai.key() != &expected_history {
        return Err(ProgramError::InvalidSeeds);
    }

    // Enforce close delay
    if current_batch_id < history_batch_id.saturating_add(CLOSE_DELAY) {
        return Err(Pfda3Error::BatchWindowNotEnded.into());
    }

    // Close the history account: transfer lamports, zero data
    let history_lamports = history_ai.lamports();
    unsafe {
        *history_ai.borrow_mut_lamports_unchecked() = 0;
    }
    unsafe {
        *rent_recipient.borrow_mut_lamports_unchecked() =
            rent_recipient.lamports().checked_add(history_lamports)
                .ok_or(Pfda3Error::Overflow)?;
    }
    let mut data = history_ai.try_borrow_mut_data()?;
    data.fill(0);

    Ok(())
}

//! SetBatchId — TEST-ONLY ix for fast-forwarding `pool.current_batch_id`
//! past the close-delay windows used by `CloseBatchHistory` (100 batches)
//! and `CloseExpiredTicket` (200 batches). Issue #61 item 6.
//!
//! # Why this exists
//!
//! LiteSVM cannot warp `current_batch_id` directly — the only way to
//! advance it through normal protocol flows is to drive 100+ full batch
//! cycles, which is infeasible in test time. Without an out-of-band
//! way to set the value, the success-path coverage for the close-delay
//! gates would have to be skipped, leaving only reject-path tests in
//! the suite.
//!
//! # Why a feature flag instead of a bench helper
//!
//! Bench-only helpers either leak test-only logic into production
//! source files (anyone reading `pool_state.rs` would see them) or
//! get duplicated across test fixtures. A `cfg(feature = "test-time-warp")`
//! gate makes the entire instruction — handler, dispatcher arm, mod
//! export — disappear from the build graph when the feature isn't
//! passed. CI's mainnet `cargo build-sbf` invocation does NOT pass the
//! feature, so the resulting `.so` cannot reach this code path.
//!
//! Authority-gated even in test builds, mirroring `SetPaused3`, so an
//! accidental feature-on devnet build would still be safe.
//!
//! # Accounts
//!
//! ```text
//! 0: [signer]    authority (must equal pool.authority)
//! 1: [writable]  pool_state PDA
//! ```
//!
//! # Instruction data
//!
//! ```text
//! [new_batch_id: u64 LE]
//! ```

use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, PoolState3};

pub fn process_set_batch_id(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_batch_id: u64,
) -> ProgramResult {
    let [authority, pool_ai, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if pool_ai.owner() != program_id {
        return Err(ProgramError::IllegalOwner);
    }

    let stored_authority = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        pool.authority
    };

    if authority.key().as_ref() != &stored_authority {
        return Err(Pfda3Error::Unauthorized.into());
    }

    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.current_batch_id = new_batch_id;
    }

    Ok(())
}

//! ProposeWeights — stage a new target-weight vector behind a timelock.
//!
//! `weights_bps` has been immutable since CreateEtf. This instruction
//! makes it mutable under two rate limits (P-6 anti-rug):
//!   - per-entry move capped at `MAX_WEIGHT_DELTA_BPS` per proposal
//!   - `WEIGHT_TIMELOCK_SLOTS` between proposal and ApplyWeights, so
//!     holders who disagree can redeem at current NAV first
//!
//! The pending proposal lives in the rebalance sidecar PDA (lazily
//! created here if Rebalance hasn't already done so). Re-proposing
//! overwrites the pending vector and restarts the timelock.
//!
//! Allowed while paused: staging a proposal moves no funds, and the
//! timelock should keep running during an operational pause.
//!
//! # Account layout
//!
//! ```text
//! 0: [signer, writable]  authority (pays sidecar rent on first use)
//! 1: []                  etf_state PDA
//! 2: [writable]          rebalance_state PDA ([b"rebal", etf_state])
//! 3: []                  system_program
//! ```
//!
//! # Instruction data (after the discriminator byte)
//!
//! ```text
//! [token_count: u8][weights: u16 LE × token_count]
//! ```

use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

use crate::constants::{MAX_WEIGHT_DELTA_BPS, WEIGHT_TIMELOCK_SLOTS};
use crate::error::VaultError;
use crate::instructions::rebalance::ensure_rebalance_state;
use crate::state::{load, load_mut, EtfState, RebalanceState, MAX_BASKET_TOKENS};

pub fn process_propose_weights(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_weights: &[u16],
) -> ProgramResult {
    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];
    let rebalance_state_ai = &accounts[2];
    let _system_program = &accounts[3];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    let (token_count, stored_auth, stored_bump, name_buf, current_weights) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf =
            unsafe { load::<EtfState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (
            etf.token_count as usize,
            etf.authority,
            etf.bump,
            etf.name,
            etf.weights_bps,
        )
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
    }
    if new_weights.len() != token_count {
        return Err(VaultError::WeightsMismatch.into());
    }

    // Sum must hold AND every basket entry must stay strictly positive:
    // a zero weight would zero that token's per-vault mint candidate in
    // Deposit, making the NAV-deviation spread check unsatisfiable and
    // bricking direct deposits. Phasing an asset out stays bounded at
    // MAX_WEIGHT_DELTA_BPS per proposal and floors at 1 bps.
    let mut weight_sum: u32 = 0;
    for i in 0..token_count {
        if new_weights[i] == 0 {
            return Err(VaultError::WeightsMismatch.into());
        }
        weight_sum += new_weights[i] as u32;
    }
    if weight_sum != 10_000 {
        return Err(VaultError::WeightsMismatch.into());
    }

    for i in 0..token_count {
        let old = current_weights[i];
        let new = new_weights[i];
        let delta = if new >= old { new - old } else { old - new };
        if delta > MAX_WEIGHT_DELTA_BPS {
            return Err(VaultError::WeightDeltaExceeded.into());
        }
    }

    // PDA re-derivation (SetFee idiom).
    let name_len = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
    let (expected_pda, expected_bump) = pubkey::find_program_address(
        &[b"etf", &stored_auth, &name_buf[..name_len]],
        program_id,
    );
    if etf_state_ai.key() != &expected_pda || expected_bump != stored_bump {
        return Err(ProgramError::InvalidSeeds);
    }

    ensure_rebalance_state(program_id, authority, rebalance_state_ai, etf_state_ai.key())?;

    let current_slot = Clock::get()?.slot;
    let eta = current_slot
        .checked_add(WEIGHT_TIMELOCK_SLOTS)
        .ok_or(VaultError::Overflow)?;

    {
        let mut data = rebalance_state_ai.try_borrow_mut_data()?;
        let st = unsafe { load_mut::<RebalanceState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        let mut staged = [0u16; MAX_BASKET_TOKENS];
        staged[..token_count].copy_from_slice(&new_weights[..token_count]);
        st.proposed_weights = staged;
        st.proposal_eta_slot = eta;
    }

    Ok(())
}

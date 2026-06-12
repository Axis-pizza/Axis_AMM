//! ApplyWeights — activate a timelocked weight proposal.
//!
//! Writes the sidecar's `proposed_weights` into `etf_state.weights_bps`
//! (a value-only update of an existing field — no layout change) once
//! `proposal_eta_slot` has passed, then clears the proposal.
//!
//! The proposal's delta bound was validated against the weights active
//! at ProposeWeights time; weights can only change through this very
//! instruction (which consumes the proposal), so the bound still holds
//! at apply time.
//!
//! # Account layout
//!
//! ```text
//! 0: [signer]    authority
//! 1: [writable]  etf_state PDA
//! 2: [writable]  rebalance_state PDA ([b"rebal", etf_state])
//! ```
//!
//! No instruction data.

use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

use crate::error::VaultError;
use crate::instructions::rebalance::check_rebalance_state;
use crate::state::{load, load_mut, EtfState, RebalanceState, MAX_BASKET_TOKENS};

pub fn process_apply_weights(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if accounts.len() < 3 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];
    let rebalance_state_ai = &accounts[2];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    let (token_count, stored_auth, stored_bump, name_buf) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf =
            unsafe { load::<EtfState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (etf.token_count as usize, etf.authority, etf.bump, etf.name)
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
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

    check_rebalance_state(program_id, rebalance_state_ai, etf_state_ai.key())?;

    let current_slot = Clock::get()?.slot;

    let staged = {
        let mut data = rebalance_state_ai.try_borrow_mut_data()?;
        let st = unsafe { load_mut::<RebalanceState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if st.proposal_eta_slot == 0 {
            return Err(VaultError::NoPendingProposal.into());
        }
        if current_slot < st.proposal_eta_slot {
            return Err(VaultError::TimelockNotElapsed.into());
        }
        let staged = st.proposed_weights;
        st.proposed_weights = [0u16; MAX_BASKET_TOKENS];
        st.proposal_eta_slot = 0;
        staged
    };

    // Defense in depth: the staged vector was validated at propose time
    // against this same token_count; re-assert the invariant before the
    // value-only write.
    let mut weight_sum: u32 = 0;
    for i in 0..token_count {
        weight_sum += staged[i] as u32;
    }
    if weight_sum != 10_000 {
        return Err(VaultError::WeightsMismatch.into());
    }

    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.weights_bps = staged;
    }

    Ok(())
}

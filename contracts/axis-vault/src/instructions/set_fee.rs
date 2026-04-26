use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::constants::MAX_FEE_BPS_CEILING;
use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// SetFee — adjust the ETF's `fee_bps` within `[0, max_fee_bps]`.
///
/// Why this exists: `fee_bps` is hard-coded to 30 in CreateEtf. Without
/// this instruction, changing the fee on a live mainnet ETF would
/// require a program upgrade — a heavyweight, multisig-gated motion
/// for a value the authority should be able to tune in response to
/// market conditions.
///
/// Why a per-ETF ceiling: we don't trust an authority to dial the fee
/// to 100 % and drain a pool, even if they own the key. The per-ETF
/// `max_fee_bps` field (set at CreateEtf time, not adjustable) caps
/// the worst case. The program-wide `MAX_FEE_BPS_CEILING` is a second
/// guard against a CreateEtf payload that tries to bypass it.
///
/// PDA re-derivation matches SetPaused — clients can't substitute a
/// crafted writable account with the right discriminator.
///
/// Accounts:
///   0: [signer]   authority
///   1: [writable] etf_state PDA
///
/// Data: [new_fee_bps: u16 LE]
pub fn process_set_fee(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_fee_bps: u16,
) -> ProgramResult {
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    // Hard ceiling check before we even touch the account: programmer
    // error / malicious bypass attempt fails fast.
    if new_fee_bps > MAX_FEE_BPS_CEILING {
        return Err(VaultError::FeeTooHigh.into());
    }

    let (name_buf, stored_auth, stored_bump, stored_max_fee) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (etf.name, etf.authority, etf.bump, etf.max_fee_bps)
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
    }

    if new_fee_bps > stored_max_fee {
        return Err(VaultError::FeeTooHigh.into());
    }

    let name_len = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
    let (expected_pda, expected_bump) = pubkey::find_program_address(
        &[b"etf", &stored_auth, &name_buf[..name_len]],
        program_id,
    );
    if etf_state_ai.key() != &expected_pda || expected_bump != stored_bump {
        return Err(ProgramError::InvalidSeeds);
    }

    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.fee_bps = new_fee_bps;
    }

    Ok(())
}

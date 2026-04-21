use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// SetPaused — flip the ETF's `paused` flag. Only the ETF authority may
/// call this. Instruction data is a single byte (0 = active, 1 = paused);
/// any non-zero value is normalized to 1.
///
/// Accounts:
///   0: [signer]   authority
///   1: [writable] etf_state PDA
///
/// Data: [paused: u8]
pub fn process_set_paused(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    paused: u8,
) -> ProgramResult {
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    // Re-derive the ETF PDA from the name stored on the account itself,
    // so clients that hand in a crafted writable account with the right
    // discriminator bytes can't masquerade as a real ETF. Also enforces
    // that `authority` is the stored etf.authority.
    let (name_buf, stored_auth, stored_bump) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (etf.name, etf.authority, etf.bump)
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
    }

    // The CreateEtf PDA seeds are [b"etf", authority, name_bytes]. The
    // `name` stored on-chain is MAX_ETF_NAME_LEN, zero-padded — trim to
    // the actual byte length used at CreateEtf time before comparing.
    let name_len = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
    let (expected_pda, expected_bump) = pubkey::find_program_address(
        &[b"etf", &stored_auth, &name_buf[..name_len]],
        program_id,
    );
    if etf_state_ai.key() != &expected_pda || expected_bump != stored_bump {
        return Err(ProgramError::InvalidSeeds);
    }

    let normalized = if paused == 0 { 0 } else { 1 };
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.paused = normalized;
    }

    Ok(())
}

use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    ProgramResult,
};

use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// SetCap — raise (never lower) the ETF's TVL cap. Zero means uncapped.
///
/// Why this exists: closed-beta launches benefit from a TVL ramp curve.
/// Authority sets a low initial cap (e.g. $1M equivalent in `total_supply`
/// units), monitors live behaviour, raises it as confidence grows.
///
/// Why monotonic: lowering the cap on a pool currently above it would
/// brick further deposits without any in-protocol drain path back to
/// the lower cap. Withdrawals would still pay each user the
/// proportional share against current state, so existing LPs aren't
/// trapped — but new deposits revert. Bad UX, no upside. Keep the
/// rule simple: cap goes up, never down. Closing a pool entirely is
/// the proper "drain to zero" path (Withdraw + SweepTreasury).
///
/// PDA re-derivation matches SetPaused / SetFee.
///
/// Accounts:
///   0: [signer]   authority
///   1: [writable] etf_state PDA
///
/// Data: [new_cap: u64 LE]
pub fn process_set_cap(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_cap: u64,
) -> ProgramResult {
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    let (name_buf, stored_auth, stored_bump, current_cap) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (etf.name, etf.authority, etf.bump, etf.tvl_cap)
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
    }

    // Special case: setting cap to 0 is "remove cap entirely". Allow
    // this transition unconditionally — opting out of the gate is
    // always strictly more permissive than the current state.
    //
    // Otherwise: cap is monotonically non-decreasing. Setting the
    // same value is a no-op-but-not-an-error (idempotent).
    if new_cap != 0 && new_cap < current_cap {
        return Err(VaultError::InvalidCapDecrease.into());
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
        etf.tvl_cap = new_cap;
    }

    Ok(())
}

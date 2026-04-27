//! Mock Jupiter program used by axis-vault WithdrawSol bound tests.
//!
//! The on-chain Jupiter binary is too heavy for LiteSVM unit tests
//! (full route plans, ALTs, AMM accounts), so we ship a minimal
//! substitute that lets a test prove axis-vault enforces the per-leg
//! input-side bound without needing a real swap.
//!
//! The mock implements two deliberately-unsafe behaviours that only
//! make sense as a test fixture:
//!
//!   1. Drain `in_amount` from the source token account into a sink.
//!      `in_amount` is read straight from the instruction data, with
//!      no relationship to any oracle or pool state. The vault PDA's
//!      signed status (granted by axis-vault at the parent CPI
//!      boundary) propagates into the inner SPL Token Transfer, so
//!      the mock can drain whatever the test asks it to.
//!   2. Optionally emit `out_amount` of wSOL from a pre-funded
//!      `wsol_source` account to a `wsol_destination` ATA, so the
//!      "honest" test path can produce non-zero wSOL output and pass
//!      the slippage gate.
//!
//! Account layout (axis-vault auto-prepends `vault[i]` at slot 0):
//!
//! ```text
//! 0: source              (vault[i],     writable)
//! 1: source_authority    (etf_state PDA, signed via parent CPI seeds)
//! 2: sink                (drain target, writable)
//! 3: token_program       (executable)
//! 4: wsol_source         (writable)            ── only if out_amount > 0
//! 5: wsol_source_auth    (signer)              ── only if out_amount > 0
//! 6: wsol_destination    (withdrawer wsol_ata) ── only if out_amount > 0
//! ```
//!
//! Instruction data: `[in_amount: u64 LE][out_amount: u64 LE]`
//!
//! NOT FOR PRODUCTION USE. Lives under the test crate so cargo
//! build-sbf only compiles it when integration tests are run.

#![cfg_attr(not(test), no_std)]

#[cfg(all(not(test), target_os = "solana"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};
use pinocchio_token::instructions::Transfer;

#[cfg(not(feature = "no-entrypoint"))]
pinocchio::entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if data.len() < 16 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let in_amount = u64::from_le_bytes(
        data[0..8]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );
    let out_amount = u64::from_le_bytes(
        data[8..16]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let source = &accounts[0];
    let source_authority = &accounts[1];
    let sink = &accounts[2];
    // accounts[3] = token_program — implicitly used by pinocchio_token's
    // Transfer CPI; the runtime requires the program be present in the
    // account list whether or not it's referenced by the AccountInfo
    // chain.

    Transfer {
        from: source,
        to: sink,
        authority: source_authority,
        amount: in_amount,
    }
    .invoke()?;

    if out_amount > 0 {
        if accounts.len() < 7 {
            return Err(ProgramError::NotEnoughAccountKeys);
        }
        let wsol_source = &accounts[4];
        let wsol_source_auth = &accounts[5];
        let wsol_destination = &accounts[6];
        Transfer {
            from: wsol_source,
            to: wsol_destination,
            authority: wsol_source_auth,
            amount: out_amount,
        }
        .invoke()?;
    }

    Ok(())
}

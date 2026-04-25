//! Jupiter V6 integration helpers for axis-vault DepositSol / WithdrawSol.
//!
//! Provides:
//!   - `JUPITER_PROGRAM_ID` for CPI validation
//!   - `WSOL_MINT` for wsol_mint validation
//!   - `MAX_JUPITER_CPI_ACCOUNTS` per-leg account budget
//!   - `read_token_account_balance` SPL token balance reader
//!   - `invoke_jupiter_leg` helper that builds + invokes a single Jupiter CPI
//!
//! The main difference from `axis-g3m/src/jupiter.rs` is that DepositSol
//! depositor-signs (no PDA seeds) while WithdrawSol PDA-signs through
//! the etf_state PDA. `invoke_jupiter_leg` accepts an optional signer.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::error::VaultError;

/// Jupiter V6 program ID bytes (base58
/// `JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4`). Mirrors
/// axis-g3m/src/jupiter.rs so the on-chain SOL ixes target the same
/// program the Rebalance flow already trusts.
pub const JUPITER_PROGRAM_ID: [u8; 32] = [
    0x04, 0x79, 0xd5, 0x5b, 0xf2, 0x31, 0xc0, 0x6e,
    0xee, 0x74, 0xc5, 0x6e, 0xce, 0x68, 0x15, 0x07,
    0xfd, 0xb1, 0xb2, 0xde, 0xa3, 0xf4, 0x8e, 0x51,
    0x02, 0xb1, 0xcd, 0xa2, 0x56, 0xbc, 0x13, 0x8f,
];

/// Wrapped-SOL mint (`So11111111111111111111111111111111111111112`).
pub const WSOL_MINT: [u8; 32] = [
    0x06, 0x9b, 0x88, 0x57, 0xfe, 0xab, 0x81, 0x84,
    0xfb, 0x68, 0x7f, 0x63, 0x46, 0x18, 0xc0, 0x35,
    0xda, 0xc4, 0x39, 0xdc, 0x1a, 0xeb, 0x3b, 0x55,
    0x98, 0xa0, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x01,
];

/// Per-leg account-list budget. Jupiter routes typically carry 20-32
/// accounts; keep one slot for the wSOL ATA and bound the tail. With
/// `MAX_ONCHAIN_SOL_IX_LEGS = 3` and 32 per leg the worst-case account
/// count stays inside the versioned-tx envelope (96 + fixed-prefix
/// accounts).
pub const MAX_JUPITER_CPI_ACCOUNTS: usize = 32;

/// Read u64 balance from an SPL token account at offset 64. Mirrors
/// axis-g3m's helper. Returns InvalidAccountData on truncated data.
pub fn read_token_account_balance(account: &AccountInfo) -> Result<u64, ProgramError> {
    let data = account.try_borrow_data()?;
    if data.len() < 72 {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(u64::from_le_bytes(
        data[64..72]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ))
}

/// Invoke a single Jupiter V6 swap leg with the provided route bytes
/// and account slice. The first `route_accounts.len()` AccountInfo
/// references in `route_accounts` become both the AccountMeta list and
/// the AccountInfo list passed to the CPI; metas are derived from each
/// AccountInfo's signer/writable flags. Optionally accepts a PDA
/// signer for vault-side WithdrawSol legs.
///
/// Returns the bound used internally so callers can size their CU
/// budget against `MAX_JUPITER_CPI_ACCOUNTS`.
#[allow(clippy::too_many_arguments)]
pub fn invoke_jupiter_leg(
    jupiter_program: &AccountInfo,
    route_accounts: &[&AccountInfo],
    route_bytes: &[u8],
    pda_signer: Option<&Signer>,
) -> Result<(), ProgramError> {
    if jupiter_program.key().as_ref() != &JUPITER_PROGRAM_ID {
        return Err(VaultError::InvalidJupiterProgram.into());
    }
    if route_accounts.len() > MAX_JUPITER_CPI_ACCOUNTS {
        return Err(ProgramError::InvalidArgument);
    }

    // Build AccountMeta list. We use writable + signer flags from the
    // AccountInfo itself; clients must build the tx with the right
    // flags, the same way Jupiter expects when called directly.
    let mut metas_storage: [core::mem::MaybeUninit<AccountMeta>; MAX_JUPITER_CPI_ACCOUNTS] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    for (i, ai) in route_accounts.iter().enumerate() {
        metas_storage[i].write(AccountMeta::from(*ai));
    }
    let metas: &[AccountMeta] = unsafe {
        core::slice::from_raw_parts(
            metas_storage.as_ptr() as *const AccountMeta,
            route_accounts.len(),
        )
    };

    let jup_pid = unsafe { &*(&JUPITER_PROGRAM_ID as *const [u8; 32] as *const Pubkey) };
    let cpi_ix = Instruction {
        program_id: jup_pid,
        accounts: metas,
        data: route_bytes,
    };

    match pda_signer {
        Some(signer) => {
            let signers = [signer.clone()];
            pinocchio::cpi::invoke_signed_with_bounds::<MAX_JUPITER_CPI_ACCOUNTS>(
                &cpi_ix,
                route_accounts,
                &signers,
            )
        }
        None => {
            // No PDA signature needed; depositor signs at the tx level.
            pinocchio::cpi::invoke_signed_with_bounds::<MAX_JUPITER_CPI_ACCOUNTS>(
                &cpi_ix,
                route_accounts,
                &[],
            )
        }
    }
}

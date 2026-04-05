/// Security validation helpers.
/// Implements rules 6, 7 from the Axis Protocol security checklist.

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};
use crate::error::Pfda3Error;

/// SPL Token Program ID
const TOKEN_PROGRAM_ID: [u8; 32] = [
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93,
    0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91,
    0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
];

/// Rule 6: Verify a token account is owned by the SPL Token Program.
pub fn verify_token_account_owner(account: &AccountInfo) -> Result<(), ProgramError> {
    if account.owner() != &TOKEN_PROGRAM_ID {
        return Err(Pfda3Error::InvalidDiscriminator.into());
    }
    Ok(())
}

/// Rule 7: Verify a token account's mint matches the expected mint.
/// SPL token account layout: mint is at offset 0, 32 bytes.
pub fn verify_token_account_mint(
    token_account: &AccountInfo,
    expected_mint: &[u8; 32],
) -> Result<(), ProgramError> {
    let data = token_account.try_borrow_data()?;
    if data.len() < 32 {
        return Err(ProgramError::InvalidAccountData);
    }
    if &data[0..32] != expected_mint {
        return Err(Pfda3Error::PoolMismatch.into());
    }
    Ok(())
}

/// Verify a vault account: owned by token program, mint matches, owner matches pool PDA.
/// SPL token account layout: owner is at offset 32, 32 bytes.
pub fn verify_vault(
    vault: &AccountInfo,
    expected_mint: &[u8; 32],
    expected_owner: &[u8; 32],
) -> Result<(), ProgramError> {
    verify_token_account_owner(vault)?;
    let data = vault.try_borrow_data()?;
    if data.len() < 64 {
        return Err(ProgramError::InvalidAccountData);
    }
    // Check mint
    if &data[0..32] != expected_mint {
        return Err(Pfda3Error::PoolMismatch.into());
    }
    // Check owner
    if &data[32..64] != expected_owner {
        return Err(Pfda3Error::OwnerMismatch.into());
    }
    Ok(())
}

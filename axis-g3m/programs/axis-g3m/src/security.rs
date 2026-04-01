/// Security validation helpers for axis-g3m.

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};
use crate::error::G3mError;

const TOKEN_PROGRAM_ID: [u8; 32] = [
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93,
    0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91,
    0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
];

pub fn verify_token_account_owner(account: &AccountInfo) -> Result<(), ProgramError> {
    if account.owner() != &TOKEN_PROGRAM_ID {
        return Err(G3mError::InvalidDiscriminator.into());
    }
    Ok(())
}

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
    if &data[0..32] != expected_mint {
        return Err(G3mError::PoolMismatch.into());
    }
    if &data[32..64] != expected_owner {
        return Err(G3mError::OwnerMismatch.into());
    }
    Ok(())
}

/// Helpers and constants for future Jupiter-backed rebalancing.
///
/// Important: this module does not currently perform a Jupiter CPI.
/// The live code path only exposes:
///   - the Jupiter program id constant for future validation, and
///   - SPL token vault balance readers used by the current rehearsal flow.
///
/// A real same-transaction Jupiter rebalance will need a follow-up instruction
/// design that accepts route accounts and swap instruction data, then invokes
/// Jupiter with the pool PDA as signer.
///
/// Jupiter V6 program ID (for reference/validation):
///   JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4

/// Jupiter V6 program ID bytes (for client-side validation)
pub const JUPITER_PROGRAM_ID: [u8; 32] = [
    0x04, 0x58, 0x99, 0x26, 0x88, 0x1e, 0xd7, 0x10,
    0x0e, 0x13, 0x14, 0x20, 0x8b, 0x1e, 0x54, 0x25,
    0x42, 0x79, 0x4f, 0x5d, 0x08, 0x16, 0x31, 0x76,
    0xa0, 0x73, 0xb2, 0x79, 0x81, 0x40, 0x2e, 0x72,
];

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// Read token account balance from SPL token account data.
/// SPL token account layout: amount is at offset 64, 8 bytes LE.
pub fn read_vault_balance(account: &AccountInfo) -> Result<u64, ProgramError> {
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

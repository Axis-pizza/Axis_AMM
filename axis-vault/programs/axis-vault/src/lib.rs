//! Axis Vault — ETF token lifecycle management.
//!
//! Manages baskets of SPL tokens as ETFs:
//!   - create_etf: Initialize vault, create SPL token mint, store basket composition
//!   - deposit: Accept basket tokens proportionally, mint ETF tokens
//!   - withdraw: Burn ETF tokens, return proportional basket tokens

#![cfg_attr(not(test), no_std)]

#[cfg(all(not(test), target_os = "solana"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

pub mod error;
pub mod instructions;
pub mod state;

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

#[cfg(not(feature = "no-entrypoint"))]
pinocchio::entrypoint!(process_instruction);

#[repr(u8)]
enum Instruction {
    CreateEtf = 0,
    Deposit = 1,
    Withdraw = 2,
}

impl Instruction {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Instruction::CreateEtf),
            1 => Some(Instruction::Deposit),
            2 => Some(Instruction::Withdraw),
            _ => None,
        }
    }
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let disc = Instruction::from_u8(instruction_data[0])
        .ok_or(ProgramError::InvalidInstructionData)?;
    let data = &instruction_data[1..];

    match disc {
        Instruction::CreateEtf => {
            // Data: [token_count: u8][weights: [u16 LE; N]][name_len: u8][name: bytes]
            if data.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            let token_count = data[0];
            let tc = token_count as usize;
            let weights_end = 1 + tc * 2;
            if data.len() < weights_end + 1 {
                return Err(ProgramError::InvalidInstructionData);
            }

            let mut weights = [0u16; 5];
            for i in 0..tc {
                let off = 1 + i * 2;
                weights[i] = u16::from_le_bytes([data[off], data[off + 1]]);
            }

            let name_len = data[weights_end] as usize;
            let name_start = weights_end + 1;
            if data.len() < name_start + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[name_start..name_start + name_len];

            instructions::process_create_etf(
                program_id, accounts, token_count, &weights[..tc], name,
            )
        }

        Instruction::Deposit => {
            // Data: [amount: u64 LE][name_len: u8][name: bytes]
            if data.len() < 9 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let amount = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let name_len = data[8] as usize;
            if data.len() < 9 + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[9..9 + name_len];

            instructions::process_deposit(program_id, accounts, amount, name)
        }

        Instruction::Withdraw => {
            // Data: [burn_amount: u64 LE][name_len: u8][name: bytes]
            if data.len() < 9 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let burn_amount = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let name_len = data[8] as usize;
            if data.len() < 9 + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[9..9 + name_len];

            instructions::process_withdraw(program_id, accounts, burn_amount, name)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::*;
    #[test]
    fn print_sizes() {
        let size = core::mem::size_of::<EtfState>();
        eprintln!("EtfState: {} bytes", size);
        let e = unsafe { core::mem::zeroed::<EtfState>() };
        let b = &e as *const _ as usize;
        eprintln!("  authority: {}", (&e.authority as *const _ as usize) - b);
        eprintln!("  etf_mint: {}", (&e.etf_mint as *const _ as usize) - b);
        eprintln!("  token_count: {}", (&e.token_count as *const _ as usize) - b);
        eprintln!("  token_mints: {}", (&e.token_mints as *const _ as usize) - b);
        eprintln!("  token_vaults: {}", (&e.token_vaults as *const _ as usize) - b);
        eprintln!("  weights_bps: {}", (&e.weights_bps as *const _ as usize) - b);
        eprintln!("  total_supply: {}", (&e.total_supply as *const _ as usize) - b);
        eprintln!("  treasury: {}", (&e.treasury as *const _ as usize) - b);
        eprintln!("  bump: {}", (&e.bump as *const _ as usize) - b);
    }
}

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

pub mod constants;
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
    SweepTreasury = 3,
    /// SetPaused — authority-gated flip of the `paused` flag (#33).
    SetPaused = 4,
    /// DepositSol — SOL-in variant with on-chain Jupiter CPI + strict
    /// slippage gate. Scaffolding only — returns NotYetImplemented
    /// until the follow-up design review (#36).
    DepositSol = 5,
    /// WithdrawSol — SOL-out variant, same status as DepositSol.
    WithdrawSol = 6,
}

impl Instruction {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Instruction::CreateEtf),
            1 => Some(Instruction::Deposit),
            2 => Some(Instruction::Withdraw),
            3 => Some(Instruction::SweepTreasury),
            4 => Some(Instruction::SetPaused),
            5 => Some(Instruction::DepositSol),
            6 => Some(Instruction::WithdrawSol),
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
            // Data layout (#37):
            //   [token_count: u8]
            //   [weights: [u16 LE; N]]
            //   [ticker_len: u8][ticker: bytes]
            //   [name_len: u8][name: bytes]
            //
            // Ticker is laid out before name so clients can parse the
            // metadata in one forward pass. All length-prefixed fields
            // are u8-prefixed (16/32 byte maxima enforced by the
            // instruction handler).
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

            let ticker_len = data[weights_end] as usize;
            let ticker_start = weights_end + 1;
            if data.len() < ticker_start + ticker_len + 1 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let ticker = &data[ticker_start..ticker_start + ticker_len];

            let name_len_off = ticker_start + ticker_len;
            let name_len = data[name_len_off] as usize;
            let name_start = name_len_off + 1;
            if data.len() < name_start + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[name_start..name_start + name_len];

            instructions::process_create_etf(
                program_id, accounts, token_count, &weights[..tc], ticker, name,
            )
        }

        Instruction::Deposit => {
            // Data: [amount: u64 LE][min_mint_out: u64 LE][name_len: u8][name: bytes]
            if data.len() < 17 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let amount = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let min_mint_out = u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]);
            let name_len = data[16] as usize;
            if data.len() < 17 + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[17..17 + name_len];

            instructions::process_deposit(program_id, accounts, amount, min_mint_out, name)
        }

        Instruction::Withdraw => {
            // Data: [burn_amount: u64 LE][min_tokens_out: u64 LE][name_len: u8][name: bytes]
            if data.len() < 17 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let burn_amount = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let min_tokens_out = u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]);
            let name_len = data[16] as usize;
            if data.len() < 17 + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[17..17 + name_len];

            instructions::process_withdraw(program_id, accounts, burn_amount, min_tokens_out, name)
        }

        Instruction::SweepTreasury => {
            // Data: [name_len: u8][name: bytes]
            // Burn amount is read on-chain from treasury_etf_ata balance
            // so the cranker doesn't need to fetch-then-submit.
            if data.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name_len = data[0] as usize;
            if data.len() < 1 + name_len {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[1..1 + name_len];
            instructions::process_sweep_treasury(program_id, accounts, name)
        }

        Instruction::SetPaused => {
            // Data: [paused: u8]
            if data.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            instructions::process_set_paused(program_id, accounts, data[0])
        }

        Instruction::DepositSol => {
            // Data: [sol_in: u64][min_etf_out: u64][name_len: u8][name]
            //       [leg_count: u8][per-leg routes: variable]
            if data.len() < 18 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let sol_in = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let min_etf_out = u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]);
            let name_len = data[16] as usize;
            if data.len() < 17 + name_len + 1 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[17..17 + name_len];
            let leg_count = data[17 + name_len];
            instructions::process_deposit_sol(
                program_id, accounts, sol_in, min_etf_out, name, leg_count,
            )
        }

        Instruction::WithdrawSol => {
            // Data: [burn_amount: u64][min_sol_out: u64][name_len: u8][name]
            //       [leg_count: u8][per-leg routes: variable]
            if data.len() < 18 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let burn_amount = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            let min_sol_out = u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]);
            let name_len = data[16] as usize;
            if data.len() < 17 + name_len + 1 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let name = &data[17..17 + name_len];
            let leg_count = data[17 + name_len];
            instructions::process_withdraw_sol(
                program_id, accounts, burn_amount, min_sol_out, name, leg_count,
            )
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
        eprintln!("  name: {}", (&e.name as *const _ as usize) - b);
        eprintln!("  ticker: {}", (&e.ticker as *const _ as usize) - b);
        eprintln!("  created_at_slot: {}", (&e.created_at_slot as *const _ as usize) - b);
    }
}

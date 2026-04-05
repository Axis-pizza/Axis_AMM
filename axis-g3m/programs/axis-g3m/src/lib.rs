//! Axis G3M — Geometric Mean Market Maker for ETF B
//!
//! A Solana on-chain AMM using the G3M invariant (∏ x_i^{w_i} = k) with
//! drift-based rebalancing for CEX-unlisted memecoins.
//!
//! Part of the Axis Protocol A/B test:
//!   ETF A = PFDA + Switchboard (pfda-amm)
//!   ETF B = G3M, no auction (this program)

#![cfg_attr(not(test), no_std)]

#[cfg(all(not(test), target_os = "solana"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

pub mod error;
pub mod instructions;
pub mod jupiter;
pub mod security;
pub mod math;
pub mod state;

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

#[cfg(not(feature = "no-entrypoint"))]
pinocchio::entrypoint!(process_instruction);

/// Instruction discriminators (first 1 byte)
#[repr(u8)]
enum Instruction {
    InitializePool = 0,
    Swap = 1,
    CheckDrift = 2,
    Rebalance = 3,
}

impl Instruction {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Instruction::InitializePool),
            1 => Some(Instruction::Swap),
            2 => Some(Instruction::CheckDrift),
            3 => Some(Instruction::Rebalance),
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

    let discriminant = Instruction::from_u8(instruction_data[0])
        .ok_or(ProgramError::InvalidInstructionData)?;
    let data = &instruction_data[1..];

    match discriminant {
        Instruction::InitializePool => {
            // Layout:
            //   [0]:       token_count: u8
            //   [1..3]:    fee_rate_bps: u16 LE
            //   [3..5]:    drift_threshold_bps: u16 LE
            //   [5..13]:   rebalance_cooldown: u64 LE
            //   [13..13+N*2]:  weights_bps: [u16 LE; N]
            //   [13+N*2..]:    initial_reserves: [u64 LE; N]
            if data.is_empty() {
                return Err(ProgramError::InvalidInstructionData);
            }
            let token_count = data[0];
            let tc = token_count as usize;
            let min_len = 1 + 2 + 2 + 8 + tc * 2 + tc * 8;
            if data.len() < min_len {
                return Err(ProgramError::InvalidInstructionData);
            }

            let fee_rate_bps = u16::from_le_bytes([data[1], data[2]]);
            let drift_threshold_bps = u16::from_le_bytes([data[3], data[4]]);
            let rebalance_cooldown = u64::from_le_bytes([
                data[5], data[6], data[7], data[8],
                data[9], data[10], data[11], data[12],
            ]);

            let mut weights_bps = [0u16; 5];
            for i in 0..tc {
                let off = 13 + i * 2;
                weights_bps[i] = u16::from_le_bytes([data[off], data[off + 1]]);
            }

            let mut initial_reserves = [0u64; 5];
            let res_offset = 13 + tc * 2;
            for i in 0..tc {
                let off = res_offset + i * 8;
                initial_reserves[i] = u64::from_le_bytes([
                    data[off], data[off+1], data[off+2], data[off+3],
                    data[off+4], data[off+5], data[off+6], data[off+7],
                ]);
            }

            instructions::process_initialize_pool(
                program_id,
                accounts,
                token_count,
                fee_rate_bps,
                drift_threshold_bps,
                rebalance_cooldown,
                &weights_bps[..tc],
                &initial_reserves[..tc],
            )
        }

        Instruction::Swap => {
            // Layout:
            //   [0]: in_token_index: u8
            //   [1]: out_token_index: u8
            //   [2..10]: amount_in: u64 LE
            //   [10..18]: min_amount_out: u64 LE
            if data.len() < 18 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let in_idx = data[0];
            let out_idx = data[1];
            let amount_in = u64::from_le_bytes([
                data[2], data[3], data[4], data[5],
                data[6], data[7], data[8], data[9],
            ]);
            let min_amount_out = u64::from_le_bytes([
                data[10], data[11], data[12], data[13],
                data[14], data[15], data[16], data[17],
            ]);

            instructions::process_swap(
                program_id, accounts, in_idx, out_idx, amount_in, min_amount_out,
            )
        }

        Instruction::CheckDrift => {
            // No additional data
            instructions::process_check_drift(program_id, accounts)
        }

        Instruction::Rebalance => {
            // Layout: [0..N*8]: new_reserves: [u64 LE; token_count]
            // token_count is read from pool state, not instruction data
            // Require at least 2*8 = 16 bytes (minimum 2 tokens)
            if data.len() < 16 {
                return Err(ProgramError::InvalidInstructionData);
            }

            // Parse up to 5 reserves from data
            let num_reserves = data.len() / 8;
            let mut new_reserves = [0u64; 5];
            for i in 0..num_reserves.min(5) {
                let off = i * 8;
                new_reserves[i] = u64::from_le_bytes([
                    data[off], data[off+1], data[off+2], data[off+3],
                    data[off+4], data[off+5], data[off+6], data[off+7],
                ]);
            }

            instructions::process_rebalance(
                program_id, accounts, &new_reserves[..num_reserves.min(5)],
            )
        }
    }
}

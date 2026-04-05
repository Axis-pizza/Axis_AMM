//! PFDA AMM 3-Token — Batch auction for SOL/BONK/WIF (ETF A)
//!
//! Extension of pfda-amm to support 3-token pools as required
//! by the Axis Protocol A/B test specification.

#![cfg_attr(not(test), no_std)]

#[cfg(all(not(test), target_os = "solana"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

pub mod error;
pub mod instructions;
pub mod jito;
pub mod oracle;
pub mod security;
pub mod state;

use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

#[cfg(not(feature = "no-entrypoint"))]
pinocchio::entrypoint!(process_instruction);

#[repr(u8)]
enum Instruction {
    InitializePool = 0,
    SwapRequest = 1,
    ClearBatch = 2,
    Claim = 3,
    AddLiquidity = 4,
    WithdrawFees = 5,
}

impl Instruction {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Instruction::InitializePool),
            1 => Some(Instruction::SwapRequest),
            2 => Some(Instruction::ClearBatch),
            3 => Some(Instruction::Claim),
            4 => Some(Instruction::AddLiquidity),
            5 => Some(Instruction::WithdrawFees),
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
        Instruction::InitializePool => {
            // Data: [base_fee_bps: u16][window_slots: u64][w0: u32][w1: u32][w2: u32]
            if data.len() < 22 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let base_fee_bps = u16::from_le_bytes([data[0], data[1]]);
            let window_slots = u64::from_le_bytes([
                data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9],
            ]);
            let w0 = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
            let w1 = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
            let w2 = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);

            instructions::process_initialize_pool_3(
                program_id, accounts, base_fee_bps, window_slots, [w0, w1, w2],
            )
        }

        Instruction::SwapRequest => {
            // Data: [in_idx: u8][amount_in: u64][out_idx: u8][min_out: u64]
            if data.len() < 18 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let in_idx = data[0];
            let amount_in = u64::from_le_bytes([
                data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
            ]);
            let out_idx = data[9];
            let min_out = u64::from_le_bytes([
                data[10], data[11], data[12], data[13], data[14], data[15], data[16], data[17],
            ]);

            instructions::process_swap_request_3(
                program_id, accounts, in_idx, amount_in, out_idx, min_out,
            )
        }

        Instruction::ClearBatch => {
            // Data: [bid_lamports: u64 LE] (optional, 0 if no bid)
            let bid_lamports = if data.len() >= 8 {
                u64::from_le_bytes([
                    data[0], data[1], data[2], data[3],
                    data[4], data[5], data[6], data[7],
                ])
            } else {
                0
            };
            instructions::process_clear_batch_3(program_id, accounts, bid_lamports)
        }

        Instruction::Claim => {
            instructions::process_claim_3(program_id, accounts)
        }

        Instruction::AddLiquidity => {
            // Data: [amount_0: u64 LE][amount_1: u64 LE][amount_2: u64 LE]
            if data.len() < 24 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let a0 = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
            let a1 = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
            let a2 = u64::from_le_bytes([data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]]);
            instructions::process_add_liquidity_3(program_id, accounts, [a0, a1, a2])
        }

        Instruction::WithdrawFees => {
            if data.len() < 24 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let a0 = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
            let a1 = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
            let a2 = u64::from_le_bytes([data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]]);
            instructions::process_withdraw_fees(program_id, accounts, [a0, a1, a2])
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::*;
    #[test]
    fn print_sizes() {
        eprintln!("PoolState3: {} bytes", core::mem::size_of::<PoolState3>());
        eprintln!("BatchQueue3: {} bytes", core::mem::size_of::<BatchQueue3>());
        eprintln!("UserOrderTicket3: {} bytes", core::mem::size_of::<UserOrderTicket3>());
        eprintln!("ClearedBatchHistory3: {} bytes", core::mem::size_of::<ClearedBatchHistory3>());
        
        // Print PoolState3 offsets
        let p = unsafe { core::mem::zeroed::<PoolState3>() };
        let base = &p as *const _ as usize;
        eprintln!("PoolState3 offsets:");
        eprintln!("  token_mints: {}", (&p.token_mints as *const _ as usize) - base);
        eprintln!("  vaults: {}", (&p.vaults as *const _ as usize) - base);
        eprintln!("  reserves: {}", (&p.reserves as *const _ as usize) - base);
        eprintln!("  weights: {}", (&p.weights as *const _ as usize) - base);
        eprintln!("  window_slots: {}", (&p.window_slots as *const _ as usize) - base);
        eprintln!("  current_batch_id: {}", (&p.current_batch_id as *const _ as usize) - base);
        eprintln!("  current_window_end: {}", (&p.current_window_end as *const _ as usize) - base);
        eprintln!("  base_fee_bps: {}", (&p.base_fee_bps as *const _ as usize) - base);
        eprintln!("  bump: {}", (&p.bump as *const _ as usize) - base);

        let q = unsafe { core::mem::zeroed::<BatchQueue3>() };
        let qbase = &q as *const _ as usize;
        eprintln!("BatchQueue3 offsets:");
        eprintln!("  batch_id: {}", (&q.batch_id as *const _ as usize) - qbase);
        eprintln!("  total_in: {}", (&q.total_in as *const _ as usize) - qbase);
        eprintln!("  window_end_slot: {}", (&q.window_end_slot as *const _ as usize) - qbase);
    }
}

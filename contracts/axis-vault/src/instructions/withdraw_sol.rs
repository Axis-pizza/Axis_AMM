//! WithdrawSol — SOL-out variant of Withdraw. Issue #36.
//!
//! # Status: SCAFFOLDING
//!
//! Dispatcher hook + account layout + parameter validation. Jupiter CPI
//! body returns `VaultError::NotYetImplemented` pending design review
//! on PR #58 (the sibling DepositSol scaffold).
//!
//! # Account layout (design target)
//!
//! ```text
//! 0: [signer]            withdrawer
//! 1: [writable]          etf_state PDA
//! 2: [writable]          etf_mint
//! 3: [writable]          withdrawer_etf_ata (burned)
//! 4: []                  token_program
//! 5: [writable]          treasury_etf_ata (fee recipient)
//! 6: [writable]          wsol_ata (withdrawer-owned, temporary)
//! 7: []                  wsol_mint
//! 8: []                  jupiter_program
//! 9: []                  system_program
//! 10..10+tc: [writable]  per-leg basket vaults
//! 11+tc..: variable       per-leg Jupiter route accounts
//! ```
//!
//! # Instruction data (design target)
//!
//! ```text
//! [burn_amount:   u64 LE]
//! [min_sol_out:   u64 LE]
//! [name_len:      u8]   [name: bytes]
//! [leg_count:     u8]   (must equal etf.token_count, <= 3)
//! per leg:
//!   [route_len:   u32 LE]
//!   [route_bytes: route_len bytes]
//! ```
//!
//! (No per-leg amount needed on Withdraw — the amount is derived from
//! the burn's proportional basket share.)

use pinocchio::{
    account_info::AccountInfo, pubkey::Pubkey, ProgramResult,
};

use crate::error::VaultError;
use crate::instructions::deposit_sol::MAX_ONCHAIN_SOL_IX_LEGS;

pub fn process_withdraw_sol(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    burn_amount: u64,
    _min_sol_out: u64,
    _name: &[u8],
    leg_count: u8,
) -> ProgramResult {
    if burn_amount == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }
    if leg_count == 0 || leg_count > MAX_ONCHAIN_SOL_IX_LEGS {
        return Err(VaultError::BasketTooLargeForOnchainSol.into());
    }

    // TODO(#36, followup after design review):
    //   1. Compute per-vault basket amounts from burn share (existing
    //      Withdraw math).
    //   2. Burn etf tokens + mint fee to treasury (existing Withdraw
    //      accounting).
    //   3. For each leg i:
    //      - Invoke Jupiter V6 CPI with route_bytes[i] swapping
    //        basket_out[i] -> wSOL.
    //      - Jupiter writes into wsol_ata; accumulate total_wsol_out.
    //   4. Enforce total_wsol_out >= min_sol_out (strict on-chain).
    //   5. sync_native + close wsol_ata -> withdrawer (SOL to user).

    Err(VaultError::NotYetImplemented.into())
}

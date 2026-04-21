//! DepositSol — SOL-in variant of Deposit. Issue #36.
//!
//! # Status: SCAFFOLDING
//!
//! This file ships the instruction dispatcher hook + account layout +
//! error path so downstream clients can start planning. The Jupiter CPI
//! body is **intentionally not implemented** and returns
//! `VaultError::NotYetImplemented` until Muse / kidney sign off on the
//! design discussed in PR body #58.
//!
//! # Why a scaffold and not the full thing
//!
//! PR #42's author explicitly deferred on-chain SOL ixes because of:
//!
//! 1. **Account-list blow-up** — one Jupiter route carries 20-40
//!    accounts. At tc=5 the per-leg Jupiter expansion pushes a single
//!    versioned tx past the 64-signer / ~250-account envelope even
//!    with ALT.
//! 2. **CU budget** — a Jupiter swap costs 200-400k CU. 5-leg on-chain
//!    CPI = 1.0-2.0M CU, straddling the 1.4M cap.
//! 3. **Testability** — Jupiter V6 is mainnet-only, so every e2e has
//!    to go through a mainnet-fork.
//!
//! The client-bundled flow in `scripts/axis-vault/deposit-sol.ts`
//! avoids all three by letting the user sign a single versioned tx
//! with `[ComputeBudget][Jupiter × N][axis-vault Deposit]`. Atomicity
//! is preserved by the tx envelope itself.
//!
//! # What the on-chain variant gives
//!
//! Strict on-chain `min_etf_out` / `min_sol_out` enforcement with
//! single-signer UX. Useful when the caller is another program (not
//! a wallet-signed user) and can't rely on client-side slippage checks.
//!
//! # Scope limits (deliberate)
//!
//! - `tc <= 3` basket size — reserves full CU + account envelope for
//!   the axis-vault Deposit tail + compute budget + rent. tc=4,5 uses
//!   client-bundled.
//! - Single-Jupiter-program assumption — we pin to `JUPITER_PROGRAM_ID`
//!   (mirrors axis-g3m).
//!
//! # Account layout (design target)
//!
//! ```text
//! 0: [signer, writable]  depositor (pays SOL)
//! 1: [writable]          etf_state PDA
//! 2: [writable]          etf_mint
//! 3: [writable]          depositor_etf_ata
//! 4: []                  token_program
//! 5: [writable]          treasury_etf_ata
//! 6: [writable]          wsol_ata (depositor-owned, holds wrapped SOL for CPI)
//! 7: []                  wsol_mint (So11111111111111111111111111111111111111112)
//! 8: []                  jupiter_program
//! 9: []                  system_program
//! 10..10+tc: [writable]  per-leg basket vaults
//! 11+tc..: variable       per-leg Jupiter route accounts
//! ```
//!
//! # Instruction data (design target)
//!
//! ```text
//! [sol_in:        u64 LE]
//! [min_etf_out:   u64 LE]
//! [name_len:      u8]   [name: bytes]
//! [leg_count:     u8]   (must equal etf.token_count, <= 3)
//! per leg:
//!   [leg_sol_amount: u64 LE]
//!   [route_len:      u32 LE]
//!   [route_bytes:    route_len bytes — opaque to axis-vault]
//! ```

use pinocchio::{
    account_info::AccountInfo, pubkey::Pubkey, ProgramResult,
};

use crate::error::VaultError;

/// Max basket size the on-chain SOL ixes will accept. Larger baskets
/// should route through `scripts/axis-vault/deposit-sol.ts`.
pub const MAX_ONCHAIN_SOL_IX_LEGS: u8 = 3;

/// DepositSol — see module docs.
///
/// This body is a placeholder. It performs basic parameter validation
/// so clients can integrate against the interface, then returns
/// `NotYetImplemented` until the Jupiter CPI body lands in a follow-up.
pub fn process_deposit_sol(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    sol_in: u64,
    _min_etf_out: u64,
    _name: &[u8],
    leg_count: u8,
) -> ProgramResult {
    if sol_in == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }
    if leg_count == 0 || leg_count > MAX_ONCHAIN_SOL_IX_LEGS {
        return Err(VaultError::BasketTooLargeForOnchainSol.into());
    }

    // TODO(#36, followup after design review):
    //   1. Wrap sol_in lamports into wsol_ata via SystemTransfer + sync_native.
    //   2. For each leg i:
    //      - Invoke Jupiter V6 CPI with route_bytes[i] swapping
    //        leg_sol_amount[i] wSOL -> basket_mint[i].
    //      - Jupiter writes to the declared destination ATA; we verify
    //        vault_i balance delta matches the per-leg otherAmountThreshold
    //        embedded in route_bytes[i].
    //   3. Compute mint_amount via existing proportional math (same
    //      formula as Deposit, shared via a helper).
    //   4. Enforce mint_amount >= min_etf_out (strict on-chain).
    //   5. Mint etf tokens net of 30-bps fee; fee goes to treasury_etf_ata.
    //   6. Write back total_supply.

    Err(VaultError::NotYetImplemented.into())
}

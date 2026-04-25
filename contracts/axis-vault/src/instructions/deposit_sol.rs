//! DepositSol — SOL-in variant of Deposit. Issue #36.
//!
//! Wraps `sol_in` lamports into wSOL, executes one Jupiter V6 swap per
//! basket leg (writing into the corresponding vault), then mints ETF
//! tokens to the depositor proportional to the realised vault deltas.
//!
//! The Jupiter `route_bytes` are opaque to axis-vault — the caller is
//! responsible for crafting them with the leg's source mint = wSOL,
//! destination mint = `etf.token_mints[i]`, and slippage/threshold
//! parameters baked in. Per-leg CPI metas are built as
//! `[vault[i]] + caller_route_accounts[i]`, so the caller's
//! `route_bytes` MUST assume vault[i] sits at metas index 0.
//!
//! # Account layout
//!
//! ```text
//! 0:  [signer, writable]  depositor
//! 1:  [writable]          etf_state PDA
//! 2:  [writable]          etf_mint
//! 3:  [writable]          depositor_etf_ata
//! 4:  []                  token_program
//! 5:  [writable]          treasury_etf_ata
//! 6:  [writable]          wsol_ata (depositor-owned)
//! 7:  []                  wsol_mint
//! 8:  []                  jupiter_program
//! 9:  []                  system_program
//! 10..10+tc:    [writable]   per-leg basket vaults (used for validation
//!                            + pre/post balance snapshot, NOT passed to
//!                            Jupiter — that copy is prepended at CPI time)
//! 10+tc..:      []           concatenated per-leg Jupiter route accounts
//! ```
//!
//! # Instruction data
//!
//! ```text
//! [sol_in:        u64 LE]
//! [min_etf_out:   u64 LE]
//! [name_len:      u8]   [name: bytes]
//! [leg_count:     u8]   (must equal etf.token_count, <= 3)
//! per leg:
//!   [leg_sol_amount:        u64 LE]
//!   [route_account_count:   u8]      (count of route accounts EXCLUDING vault[i])
//!   [route_len:             u32 LE]
//!   [route_bytes:           route_len bytes — opaque to axis-vault]
//! ```
//!
//! # First-deposit handling
//!
//! `total_supply == 0` returns `EtfNotBootstrapped`. Bootstrap must use
//! the basket-token `Deposit` path so the seed composition matches
//! target weights — DepositSol is for ongoing additions, not creation.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{MintTo, SyncNative};

use crate::constants::{MAX_NAV_DEVIATION_BPS, TOKEN_PROGRAM_ID};
use crate::error::VaultError;
use crate::jupiter::{
    invoke_jupiter_leg, read_token_account_balance, JUPITER_PROGRAM_ID, MAX_JUPITER_CPI_ACCOUNTS,
    WSOL_MINT,
};
use crate::state::{load, load_mut, EtfState};

/// Max basket size the on-chain SOL ixes will accept. Larger baskets
/// route through `scripts/axis-vault/deposit-sol.ts`.
pub const MAX_ONCHAIN_SOL_IX_LEGS: u8 = 3;

const FIXED_ACCOUNTS: usize = 10;

#[allow(clippy::too_many_arguments)]
pub fn process_deposit_sol(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    sol_in: u64,
    min_etf_out: u64,
    name: &[u8],
    leg_count: u8,
    leg_data: &[u8],
) -> ProgramResult {
    // ─── parameter gates (cheap fail) ──────────────────────────────
    if sol_in == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }
    if leg_count == 0 || leg_count > MAX_ONCHAIN_SOL_IX_LEGS {
        return Err(VaultError::BasketTooLargeForOnchainSol.into());
    }
    let tc = leg_count as usize;

    if accounts.len() < FIXED_ACCOUNTS + tc {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let depositor = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let depositor_etf_ata = &accounts[3];
    let _tok = &accounts[4];
    let treasury_etf_ata = &accounts[5];
    let wsol_ata = &accounts[6];
    let wsol_mint_ai = &accounts[7];
    let jupiter_program = &accounts[8];
    let _system_program = &accounts[9];

    if !depositor.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }
    if jupiter_program.key().as_ref() != &JUPITER_PROGRAM_ID {
        return Err(VaultError::InvalidJupiterProgram.into());
    }
    if wsol_mint_ai.key().as_ref() != &WSOL_MINT {
        return Err(VaultError::WsolMintMismatch.into());
    }

    // ─── load etf state ────────────────────────────────────────────
    let (token_count, total_supply, authority, weights, bump_seed, fee_bps, treasury, etf_mint, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf =
            unsafe { load::<EtfState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        if etf.paused != 0 {
            return Err(VaultError::PoolPaused.into());
        }
        (
            etf.token_count as usize,
            etf.total_supply,
            etf.authority,
            etf.weights_bps,
            etf.bump,
            etf.fee_bps,
            etf.treasury,
            etf.etf_mint,
            etf.token_vaults,
        )
    };

    if tc != token_count {
        return Err(VaultError::LegCountMismatch.into());
    }
    if total_supply == 0 {
        return Err(VaultError::EtfNotBootstrapped.into());
    }
    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

    // Treasury ATA validation (same pattern as Deposit). Without it
    // anyone could route the protocol fee to an attacker-owned ATA.
    if treasury_etf_ata.owner() != &TOKEN_PROGRAM_ID {
        return Err(VaultError::TreasuryMismatch.into());
    }
    {
        let data = treasury_etf_ata.try_borrow_data()?;
        if data.len() < 64 {
            return Err(ProgramError::InvalidAccountData);
        }
        if &data[32..64] != &treasury {
            return Err(VaultError::TreasuryMismatch.into());
        }
    }

    // Vault keys must match etf.token_vaults[i] one-for-one.
    for i in 0..tc {
        if accounts[FIXED_ACCOUNTS + i].key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    // ─── parse leg_data ────────────────────────────────────────────
    // Per leg: [leg_sol_amount u64][route_account_count u8][route_len u32][route_bytes]
    let mut leg_amounts = [0u64; 5];
    let mut leg_route_account_counts = [0usize; 5];
    let mut leg_route_byte_offsets = [0usize; 5];
    let mut leg_route_byte_lens = [0usize; 5];
    let mut sum: u64 = 0;
    let mut total_route_accounts = 0usize;
    let mut cursor = 0usize;
    for i in 0..tc {
        if leg_data.len() < cursor + 8 + 1 + 4 {
            return Err(VaultError::MalformedLegData.into());
        }
        let leg_amount = u64::from_le_bytes(
            leg_data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| VaultError::MalformedLegData)?,
        );
        cursor += 8;
        let route_account_count = leg_data[cursor] as usize;
        cursor += 1;
        let route_len = u32::from_le_bytes(
            leg_data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| VaultError::MalformedLegData)?,
        ) as usize;
        cursor += 4;
        if leg_data.len() < cursor + route_len {
            return Err(VaultError::MalformedLegData.into());
        }
        // +1 to account_count: we prepend vault[i] at CPI time, so caller's
        // route_account_count + 1 must fit MAX_JUPITER_CPI_ACCOUNTS.
        if route_account_count + 1 > MAX_JUPITER_CPI_ACCOUNTS {
            return Err(ProgramError::InvalidArgument);
        }
        leg_amounts[i] = leg_amount;
        leg_route_account_counts[i] = route_account_count;
        leg_route_byte_offsets[i] = cursor;
        leg_route_byte_lens[i] = route_len;
        cursor += route_len;
        sum = sum.checked_add(leg_amount).ok_or(VaultError::Overflow)?;
        total_route_accounts = total_route_accounts
            .checked_add(route_account_count)
            .ok_or(VaultError::Overflow)?;
    }
    if sum != sol_in {
        return Err(VaultError::LegSumMismatch.into());
    }
    if accounts.len() < FIXED_ACCOUNTS + tc + total_route_accounts {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // ─── wrap SOL ──────────────────────────────────────────────────
    // Snapshot wSOL ATA balance before Transfer to detect stale residuals.
    // sync_native after a system Transfer makes the SPL token balance
    // equal `lamports - rent_exempt_minimum`, so any pre-existing
    // wSOL would be folded into our calculations and inflate mint.
    let wsol_pre = read_token_account_balance(wsol_ata)?;

    pinocchio_system::instructions::Transfer {
        from: depositor,
        to: wsol_ata,
        lamports: sol_in,
    }
    .invoke()?;

    SyncNative { native_token: wsol_ata }.invoke()?;

    let wsol_post_wrap = read_token_account_balance(wsol_ata)?;
    let expected_post = wsol_pre.checked_add(sol_in).ok_or(VaultError::Overflow)?;
    if wsol_post_wrap != expected_post {
        // Stale balance or sync_native didn't behave as expected — abort
        // before any Jupiter CPI runs. LegSumMismatch is the closest
        // semantic fit for "the books don't balance".
        return Err(VaultError::LegSumMismatch.into());
    }

    // ─── snapshot pre-CPI vault balances ───────────────────────────
    let mut pre_balances = [0u64; 5];
    for i in 0..tc {
        pre_balances[i] = read_token_account_balance(&accounts[FIXED_ACCOUNTS + i])?;
    }

    // ─── per-leg Jupiter CPIs ──────────────────────────────────────
    let mut route_cursor = FIXED_ACCOUNTS + tc;
    for i in 0..tc {
        let cnt = leg_route_account_counts[i];

        // Build route_accounts slice: [vault[i]] + caller route accounts.
        // Use MaybeUninit storage to keep this allocation-free.
        let mut refs_storage: [core::mem::MaybeUninit<&AccountInfo>; MAX_JUPITER_CPI_ACCOUNTS] =
            unsafe { core::mem::MaybeUninit::uninit().assume_init() };
        refs_storage[0].write(&accounts[FIXED_ACCOUNTS + i]);
        for j in 0..cnt {
            refs_storage[j + 1].write(&accounts[route_cursor + j]);
        }
        let refs: &[&AccountInfo] = unsafe {
            core::slice::from_raw_parts(refs_storage.as_ptr() as *const &AccountInfo, cnt + 1)
        };

        let route_bytes_start = leg_route_byte_offsets[i];
        let route_bytes_end = route_bytes_start + leg_route_byte_lens[i];
        let route_bytes = &leg_data[route_bytes_start..route_bytes_end];

        invoke_jupiter_leg(jupiter_program, refs, route_bytes, None)?;

        route_cursor += cnt;
    }

    // ─── verify vault deltas + compute mint amount ─────────────────
    // Per-vault mint candidates = delta * total_supply / pre_balance.
    // Take min with NAV deviation guard (mirrors Deposit). Reject any
    // leg that produced no output — Jupiter aborted the route silently
    // or `route_bytes` were stale and Jupiter's own slippage fired.
    let mut min_mint: Option<u128> = None;
    let mut max_mint: Option<u128> = None;
    for i in 0..tc {
        let post = read_token_account_balance(&accounts[FIXED_ACCOUNTS + i])?;
        let delta = post
            .checked_sub(pre_balances[i])
            .ok_or(VaultError::Overflow)?;
        if delta == 0 {
            return Err(VaultError::JupiterCpiNoOutput.into());
        }
        if pre_balances[i] == 0 {
            // Should be unreachable given total_supply > 0 + bootstrapped
            // basket, but guard against the divide by zero just in case.
            return Err(VaultError::DivisionByZero.into());
        }
        let candidate = (delta as u128)
            .checked_mul(total_supply as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(pre_balances[i] as u128)
            .ok_or(VaultError::DivisionByZero)?;
        min_mint = Some(match min_mint {
            Some(cur) => cur.min(candidate),
            None => candidate,
        });
        max_mint = Some(match max_mint {
            Some(cur) => cur.max(candidate),
            None => candidate,
        });
    }
    let lo = min_mint.ok_or(VaultError::DivisionByZero)?;
    let hi = max_mint.ok_or(VaultError::DivisionByZero)?;
    if lo == 0 {
        return Err(VaultError::JupiterCpiNoOutput.into());
    }
    let spread = hi.checked_sub(lo).ok_or(VaultError::Overflow)?;
    if spread
        .checked_mul(10_000)
        .ok_or(VaultError::Overflow)?
        > lo.checked_mul(MAX_NAV_DEVIATION_BPS as u128)
            .ok_or(VaultError::Overflow)?
    {
        return Err(VaultError::NavDeviationExceeded.into());
    }
    let mint_amount = lo as u64;

    if mint_amount < min_etf_out {
        return Err(VaultError::SlippageExceeded.into());
    }

    // ─── fee + mint ────────────────────────────────────────────────
    let fee_amount = mint_amount
        .checked_mul(fee_bps as u64)
        .ok_or(VaultError::Overflow)?
        / 10_000;
    let net_mint = mint_amount
        .checked_sub(fee_amount)
        .ok_or(VaultError::Overflow)?;

    let bump_bytes = [bump_seed];
    let mint_signer_seeds = [
        Seed::from(b"etf".as_ref()),
        Seed::from(authority.as_ref()),
        Seed::from(name),
        Seed::from(bump_bytes.as_ref()),
    ];

    MintTo {
        mint: etf_mint_ai,
        account: depositor_etf_ata,
        mint_authority: etf_state_ai,
        amount: net_mint,
    }
    .invoke_signed(&[Signer::from(&mint_signer_seeds)])?;

    if fee_amount > 0 {
        MintTo {
            mint: etf_mint_ai,
            account: treasury_etf_ata,
            mint_authority: etf_state_ai,
            amount: fee_amount,
        }
        .invoke_signed(&[Signer::from(&mint_signer_seeds)])?;
    }

    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf =
            unsafe { load_mut::<EtfState>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;
        etf.total_supply = etf
            .total_supply
            .checked_add(mint_amount)
            .ok_or(VaultError::Overflow)?;
    }

    // Suppress unused-binding warnings on the captured weights — kept
    // in scope so future per-leg weight checks (#36 follow-up) can
    // land without a re-read of etf_state.
    let _ = weights;

    Ok(())
}

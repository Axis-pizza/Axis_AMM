//! WithdrawSol — SOL-out variant of Withdraw. Issue #36.
//!
//! Burns ETF tokens, executes one Jupiter V6 swap per basket leg
//! (sourcing from the corresponding vault into wSOL), then unwraps
//! the accumulated wSOL into SOL for the withdrawer.
//!
//! As with DepositSol, `route_bytes` are opaque and the caller is
//! responsible for crafting them with `source_mint = etf.token_mints[i]`,
//! `destination_mint = wSOL`. Per-leg CPI metas are
//! `[vault[i]] + caller_route_accounts[i]`.
//!
//! # Account layout
//!
//! ```text
//! 0:  [signer, writable]  withdrawer
//! 1:  [writable]          etf_state PDA
//! 2:  [writable]          etf_mint
//! 3:  [writable]          withdrawer_etf_ata (burned)
//! 4:  []                  token_program
//! 5:  [writable]          treasury_etf_ata (fee recipient)
//! 6:  [writable]          wsol_ata (withdrawer-owned, accumulator)
//! 7:  []                  wsol_mint
//! 8:  []                  jupiter_program
//! 9:  []                  system_program
//! 10..10+tc:    [writable]   per-leg basket vaults (validation + delta)
//! 10+tc..:      []           concatenated per-leg Jupiter route accounts
//! ```
//!
//! # Instruction data
//!
//! ```text
//! [burn_amount:   u64 LE]
//! [min_sol_out:   u64 LE]
//! [name_len:      u8]   [name: bytes]
//! [leg_count:     u8]   (must equal etf.token_count, <= 3)
//! per leg:
//!   [route_account_count: u8]
//!   [route_len:           u32 LE]
//!   [route_bytes:         route_len bytes]
//! ```
//!
//! No per-leg amount on Withdraw — the per-vault amount is computed
//! from the burn share.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{Burn, CloseAccount, Transfer};

use crate::constants::TOKEN_PROGRAM_ID;
use crate::error::VaultError;
use crate::instructions::deposit_sol::MAX_ONCHAIN_SOL_IX_LEGS;
use crate::jupiter::{
    invoke_jupiter_leg, read_token_account_balance, JUPITER_PROGRAM_ID, MAX_JUPITER_CPI_ACCOUNTS,
    WSOL_MINT,
};
use crate::state::{load, load_mut, EtfState};

const FIXED_ACCOUNTS: usize = 10;

#[allow(clippy::too_many_arguments)]
pub fn process_withdraw_sol(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    burn_amount: u64,
    min_sol_out: u64,
    name: &[u8],
    leg_count: u8,
    leg_data: &[u8],
) -> ProgramResult {
    if burn_amount == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }
    if leg_count == 0 || leg_count > MAX_ONCHAIN_SOL_IX_LEGS {
        return Err(VaultError::BasketTooLargeForOnchainSol.into());
    }
    let tc = leg_count as usize;

    if accounts.len() < FIXED_ACCOUNTS + tc {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let withdrawer = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let withdrawer_etf_ata = &accounts[3];
    let _tok = &accounts[4];
    let treasury_etf_ata = &accounts[5];
    let wsol_ata = &accounts[6];
    let wsol_mint_ai = &accounts[7];
    let jupiter_program = &accounts[8];
    let _system_program = &accounts[9];

    if !withdrawer.is_signer() {
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
    let (token_count, total_supply, authority, bump_seed, fee_bps, treasury, etf_mint, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf =
            unsafe { load::<EtfState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        if etf.paused != 0 {
            return Err(VaultError::PoolPaused.into());
        }
        if etf.total_supply == 0 {
            return Err(VaultError::DivisionByZero.into());
        }
        (
            etf.token_count as usize,
            etf.total_supply,
            etf.authority,
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
    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

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

    for i in 0..tc {
        if accounts[FIXED_ACCOUNTS + i].key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    if burn_amount > total_supply {
        return Err(VaultError::InsufficientBalance.into());
    }

    // ─── parse leg_data ────────────────────────────────────────────
    // Per leg: [route_account_count u8][route_len u32][route_bytes]
    let mut leg_route_account_counts = [0usize; 5];
    let mut leg_route_byte_offsets = [0usize; 5];
    let mut leg_route_byte_lens = [0usize; 5];
    let mut total_route_accounts = 0usize;
    let mut cursor = 0usize;
    for i in 0..tc {
        if leg_data.len() < cursor + 1 + 4 {
            return Err(VaultError::MalformedLegData.into());
        }
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
        if route_account_count + 1 > MAX_JUPITER_CPI_ACCOUNTS {
            return Err(ProgramError::InvalidArgument);
        }
        leg_route_account_counts[i] = route_account_count;
        leg_route_byte_offsets[i] = cursor;
        leg_route_byte_lens[i] = route_len;
        cursor += route_len;
        total_route_accounts = total_route_accounts
            .checked_add(route_account_count)
            .ok_or(VaultError::Overflow)?;
    }
    if accounts.len() < FIXED_ACCOUNTS + tc + total_route_accounts {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // ─── fee + effective burn (mirrors Withdraw) ───────────────────
    let fee_amount = burn_amount
        .checked_mul(fee_bps as u64)
        .ok_or(VaultError::Overflow)?
        / 10_000;
    let effective_burn = burn_amount
        .checked_sub(fee_amount)
        .ok_or(VaultError::Overflow)?;

    // Compute per-vault swap amount = vault_balance * effective_burn / total_supply
    let mut per_vault_amount = [0u64; 5];
    for i in 0..tc {
        let vault_balance = read_token_account_balance(&accounts[FIXED_ACCOUNTS + i])?;
        let amt = (vault_balance as u128)
            .checked_mul(effective_burn as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(total_supply as u128)
            .ok_or(VaultError::DivisionByZero)? as u64;
        per_vault_amount[i] = amt;
    }

    // Transfer fee portion (ETF tokens) to treasury, burn effective_burn.
    if fee_amount > 0 {
        Transfer {
            from: withdrawer_etf_ata,
            to: treasury_etf_ata,
            authority: withdrawer,
            amount: fee_amount,
        }
        .invoke()?;
    }

    Burn {
        account: withdrawer_etf_ata,
        mint: etf_mint_ai,
        authority: withdrawer,
        amount: effective_burn,
    }
    .invoke()?;

    // ─── snapshot pre-CPI wSOL balance ─────────────────────────────
    let wsol_pre = read_token_account_balance(wsol_ata)?;

    // ─── per-leg Jupiter CPIs (vault PDA signs) ────────────────────
    // Vault PDA = etf_state PDA. Seeds: [b"etf", authority, name, bump].
    let bump_bytes = [bump_seed];
    let vault_signer_seeds = [
        Seed::from(b"etf".as_ref()),
        Seed::from(authority.as_ref()),
        Seed::from(name),
        Seed::from(bump_bytes.as_ref()),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    let mut route_cursor = FIXED_ACCOUNTS + tc;
    for i in 0..tc {
        let cnt = leg_route_account_counts[i];

        // Skip legs with zero swap amount — small dust withdrawals on
        // a token whose vault is empty would otherwise hit Jupiter
        // with a no-op route and burn CU.
        if per_vault_amount[i] == 0 {
            route_cursor += cnt;
            continue;
        }

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

        invoke_jupiter_leg(jupiter_program, refs, route_bytes, Some(&vault_signer))?;

        route_cursor += cnt;
    }

    // ─── verify wSOL accumulator + slippage gate ───────────────────
    let wsol_post = read_token_account_balance(wsol_ata)?;
    let total_wsol_out = wsol_post
        .checked_sub(wsol_pre)
        .ok_or(VaultError::Overflow)?;
    if total_wsol_out == 0 {
        return Err(VaultError::JupiterCpiNoOutput.into());
    }
    if total_wsol_out < min_sol_out {
        return Err(VaultError::SlippageExceeded.into());
    }

    // ─── unwrap wSOL → SOL by closing the wsol_ata to withdrawer ───
    // CloseAccount sends the entire lamport balance (rent + wSOL) to
    // the destination. wsol_ata is withdrawer-owned, so withdrawer
    // signs the close. Withdrawer ends up with a slightly higher SOL
    // balance than `total_wsol_out` (rent reserve is reclaimed). For
    // a strict slippage gate, we already enforced
    // total_wsol_out >= min_sol_out above.
    CloseAccount {
        account: wsol_ata,
        destination: withdrawer,
        authority: withdrawer,
    }
    .invoke()?;

    // ─── update total_supply ───────────────────────────────────────
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf =
            unsafe { load_mut::<EtfState>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;
        // Only the burned portion leaves circulation. Fee tokens were
        // transferred (not burned) so they stay in supply.
        etf.total_supply = etf
            .total_supply
            .checked_sub(effective_burn)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}

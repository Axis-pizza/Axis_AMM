use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{Burn, Transfer};

use crate::constants::TOKEN_PROGRAM_ID;
use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// Withdraw — burn ETF tokens, return proportional basket tokens.
///
/// share = effective_burn / total_supply (after fee deduction)
/// For each token: output = vault_balance * share
///
/// Fee design mirrors Deposit: the fee portion of the user's ETF tokens
/// is transferred to the treasury's ETF ATA rather than destroyed, so
/// the protocol accumulates ETF tokens from both the deposit and
/// withdraw fee rails symmetrically.
///
/// Accounts:
///   0: [signer]    withdrawer
///   1: [writable]  etf_state PDA
///   2: [writable]  etf_mint
///   3: [writable]  withdrawer_etf_token_account (source of burn + fee transfer)
///   4: []          token_program
///   5: [writable]  treasury_etf_ata (receives fee ETF tokens)
///   6..6+N: [writable] vault token accounts (source)
///   6+N..6+2N: [writable] withdrawer's basket token accounts (destination)
///
/// Data: [burn_amount: u64][min_tokens_out: u64][name: bytes for PDA derivation]
pub fn process_withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    burn_amount: u64,
    min_tokens_out: u64,
    name: &[u8],
) -> ProgramResult {
    if burn_amount == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }

    let withdrawer = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let withdrawer_etf_ata = &accounts[3];
    let _tok = &accounts[4];
    let treasury_etf_ata = &accounts[5];

    if !withdrawer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    let (tc, total_supply, authority, bump, fee_bps, treasury, etf_mint, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
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

    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

    // Validate treasury_etf_ata matches etf.treasury (same check as Deposit).
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

    if accounts.len() < 6 + tc * 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // Account layout note: Withdraw puts vaults in [6..6+tc] and user ATAs in
    // [6+tc..6+2*tc] — the reverse of Deposit, because funds flow vault → user
    // here. Keep the two in sync.
    for i in 0..tc {
        let vault = &accounts[6 + i];
        if vault.key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    if burn_amount > total_supply {
        return Err(VaultError::InsufficientBalance.into());
    }

    // Compute fee on burn
    let fee_amount = burn_amount
        .checked_mul(fee_bps as u64)
        .ok_or(VaultError::Overflow)?
        / 10_000;
    let effective_burn = burn_amount
        .checked_sub(fee_amount)
        .ok_or(VaultError::Overflow)?;

    // Pre-transfer slippage check: compute expected total output from vault
    // balances up front and reject before any CPI if below min_tokens_out.
    // Saves CUs on stale quotes and mirrors Deposit's pre-transfer guard.
    let mut expected_total: u64 = 0;
    let mut per_vault_amount = [0u64; 5];
    for i in 0..tc {
        let vault = &accounts[6 + i];
        let data = vault.try_borrow_data()?;
        if data.len() < 72 {
            return Err(ProgramError::InvalidAccountData);
        }
        let vault_balance = u64::from_le_bytes(
            data[64..72].try_into().map_err(|_| ProgramError::InvalidAccountData)?,
        );
        let withdraw_amount = (vault_balance as u128)
            .checked_mul(effective_burn as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(total_supply as u128)
            .ok_or(VaultError::DivisionByZero)? as u64;
        per_vault_amount[i] = withdraw_amount;
        expected_total = expected_total
            .checked_add(withdraw_amount)
            .ok_or(VaultError::Overflow)?;
    }

    if expected_total < min_tokens_out {
        return Err(VaultError::SlippageExceeded.into());
    }

    // Transfer fee portion to treasury (withdrawer signs). Treasury accrues
    // real ETF tokens instead of the fee being silently destroyed.
    if fee_amount > 0 {
        Transfer {
            from: withdrawer_etf_ata,
            to: treasury_etf_ata,
            authority: withdrawer,
            amount: fee_amount,
        }
        .invoke()?;
    }

    // Burn only the effective (post-fee) portion from withdrawer.
    Burn {
        account: withdrawer_etf_ata,
        mint: etf_mint_ai,
        authority: withdrawer,
        amount: effective_burn,
    }
    .invoke()?;

    // Transfer proportional basket tokens from vaults to withdrawer
    let bump_bytes = [bump];
    let vault_signer_seeds = [
        Seed::from(b"etf".as_ref()),
        Seed::from(authority.as_ref()),
        Seed::from(name),
        Seed::from(bump_bytes.as_ref()),
    ];

    for i in 0..tc {
        let vault = &accounts[6 + i];
        let dest = &accounts[6 + tc + i];
        let withdraw_amount = per_vault_amount[i];

        if withdraw_amount > 0 {
            Transfer {
                from: vault,
                to: dest,
                authority: etf_state_ai,
                amount: withdraw_amount,
            }
            .invoke_signed(&[Signer::from(&vault_signer_seeds)])?;
        }
    }

    // Update total supply: only the burned portion leaves circulation.
    // Fee tokens were transferred (not burned), so they still count in supply.
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.total_supply = etf.total_supply
            .checked_sub(effective_burn)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}

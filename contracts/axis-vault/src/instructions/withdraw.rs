use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{Burn, Transfer};

use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// Withdraw — burn ETF tokens, return proportional basket tokens.
///
/// share = burn_amount / total_supply
/// For each token: output = vault_balance * share
///
/// Accounts:
///   0: [signer]    withdrawer
///   1: [writable]  etf_state PDA
///   2: [writable]  etf_mint
///   3: [writable]  withdrawer_etf_token_account (ETF tokens to burn)
///   4: []          token_program
///   5..5+N: [writable] vault token accounts (source)
///   5+N..5+2N: [writable] withdrawer's basket token accounts (destination)
///
/// Data: [burn_amount: u64][name: bytes for PDA derivation]
pub fn process_withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    burn_amount: u64,
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

    if !withdrawer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    let (tc, total_supply, authority, bump, etf_mint, token_vaults) = {
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
            etf.etf_mint,
            etf.token_vaults,
        )
    };

    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

    if accounts.len() < 5 + tc * 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // Account layout note: Withdraw puts vaults in [5..5+tc] and user ATAs in
    // [5+tc..5+2*tc] — the reverse of Deposit, because funds flow vault → user
    // here. Keep the two in sync.
    for i in 0..tc {
        let vault = &accounts[5 + i];
        if vault.key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    if burn_amount > total_supply {
        return Err(VaultError::InsufficientBalance.into());
    }

    // Burn ETF tokens from withdrawer
    Burn {
        account: withdrawer_etf_ata,
        mint: etf_mint_ai,
        authority: withdrawer,
        amount: burn_amount,
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
        let vault = &accounts[5 + i];
        let dest = &accounts[5 + tc + i];

        // Read vault balance at offset 64 (SPL token account amount)
        let vault_balance = {
            let data = vault.try_borrow_data()?;
            if data.len() < 72 {
                return Err(ProgramError::InvalidAccountData);
            }
            u64::from_le_bytes(
                data[64..72].try_into().map_err(|_| ProgramError::InvalidAccountData)?
            )
        };

        // Proportional share: vault_balance * burn_amount / total_supply
        let withdraw_amount = (vault_balance as u128)
            .checked_mul(burn_amount as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(total_supply as u128)
            .ok_or(VaultError::DivisionByZero)? as u64;

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

    // Update total supply (checked_sub prevents underflow to u64::MAX)
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.total_supply = etf.total_supply
            .checked_sub(burn_amount)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}

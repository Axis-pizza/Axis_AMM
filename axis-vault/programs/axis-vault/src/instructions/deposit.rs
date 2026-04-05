use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{MintTo, Transfer};

use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState};

/// Deposit — accept basket tokens, mint ETF tokens proportionally.
///
/// First depositor: mint_amount = base amount
/// Subsequent: mint_amount = deposit_share * total_supply
///
/// For V1 (equal-weight baskets): deposit must be proportional to weights.
/// The user deposits `amount` of each token scaled by weight.
///
/// Accounts:
///   0: [signer]    depositor
///   1: [writable]  etf_state PDA
///   2: [writable]  etf_mint
///   3: [writable]  depositor_etf_token_account (receives minted ETF tokens)
///   4: []          token_program
///   5..5+N: [writable] depositor's basket token accounts (source)
///   5+N..5+2N: [writable] vault token accounts (destination)
///
/// Data: [amount: u64] — base amount per token (scaled by weight)
pub fn process_deposit(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
    name: &[u8],
) -> ProgramResult {
    if amount == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }

    let depositor = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let depositor_etf_ata = &accounts[3];
    let _tok = &accounts[4];

    if !depositor.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Load ETF state
    let (tc, total_supply, authority, weights, bump_seed) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        if etf.paused != 0 {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        (
            etf.token_count as usize,
            etf.total_supply,
            etf.authority,
            etf.weights_bps,
            etf.bump,
        )
    };

    if accounts.len() < 5 + tc * 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let mut token_amounts = [0u64; 5];
    for i in 0..tc {
        token_amounts[i] = (amount as u128)
            .checked_mul(weights[i] as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(10_000)
            .ok_or(VaultError::DivisionByZero)? as u64;
    }

    let mint_amount = if total_supply == 0 {
        amount
    } else {
        let mut min_mint: Option<u128> = None;
        for i in 0..tc {
            let vault = &accounts[5 + tc + i];
            let data = vault.try_borrow_data()?;
            if data.len() < 72 {
                return Err(ProgramError::InvalidAccountData);
            }

            let vault_balance = u64::from_le_bytes(
                data[64..72]
                    .try_into()
                    .map_err(|_| ProgramError::InvalidAccountData)?,
            );
            if vault_balance == 0 {
                return Err(VaultError::DivisionByZero.into());
            }

            let candidate = (token_amounts[i] as u128)
                .checked_mul(total_supply as u128)
                .ok_or(VaultError::Overflow)?
                .checked_div(vault_balance as u128)
                .ok_or(VaultError::DivisionByZero)?;

            min_mint = Some(match min_mint {
                Some(current) => current.min(candidate),
                None => candidate,
            });
        }

        let candidate = min_mint.ok_or(VaultError::DivisionByZero)?;
        if candidate == 0 {
            return Err(VaultError::ZeroDeposit.into());
        }
        candidate as u64
    };

    // Transfer basket tokens from depositor to vaults
    for i in 0..tc {
        let source = &accounts[5 + i];
        let vault = &accounts[5 + tc + i];
        let token_amount = token_amounts[i];

        if token_amount > 0 {
            Transfer {
                from: source,
                to: vault,
                authority: depositor,
                amount: token_amount,
            }
            .invoke()?;
        }
    }

    // Mint ETF tokens to depositor (EtfState PDA signs as mint authority)
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
        amount: mint_amount,
    }
    .invoke_signed(&[Signer::from(&mint_signer_seeds)])?;

    // Update total supply
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.total_supply = etf.total_supply
            .checked_add(mint_amount)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}

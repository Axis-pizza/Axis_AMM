use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::{MintTo, Transfer};

use crate::constants::{MAX_NAV_DEVIATION_BPS, TOKEN_PROGRAM_ID};
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
///   5: [writable]  treasury_etf_ata (receives fee ETF tokens)
///   6..6+N: [writable] depositor's basket token accounts (source)
///   6+N..6+2N: [writable] vault token accounts (destination)
///
/// Data: [amount: u64][min_mint_out: u64] — base amount per token (scaled by weight)
pub fn process_deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
    min_mint_out: u64,
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
    let treasury_etf_ata = &accounts[5];

    if !depositor.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Program ownership: etf_state must be owned by this program
    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    // Load ETF state
    let (tc, total_supply, authority, weights, bump_seed, fee_bps, treasury, etf_mint, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
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

    // Validate etf_mint against stored state
    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

    // Validate treasury_etf_ata: must be a Token Program account whose
    // stored owner matches etf.treasury. Without this, anyone could route
    // the protocol fee to an attacker-controlled ATA.
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

    // Account layout note: Deposit puts user ATAs in [6..6+tc] and vaults in
    // [6+tc..6+2*tc] (treasury_etf_ata occupies slot 5). Withdraw flips user/
    // vault ordering because funds flow the opposite direction. Keep in sync.
    for i in 0..tc {
        let vault = &accounts[6 + tc + i];
        if vault.key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
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
        // Per-vault mint candidates. Under a healthy basket these match
        // (the deposit mirrors target weights, vault balances match target
        // weights). If the vault has drifted — e.g. post-rebalance or a
        // MEV sandwich between the quote and the tx — the candidates
        // diverge and the safe mint is the minimum. We additionally bound
        // the spread: if the widest candidate exceeds the narrowest by
        // more than MAX_NAV_DEVIATION_BPS, the implied per-token NAV has
        // drifted too far and we refuse to mint at a stale composition.
        let mut min_mint: Option<u128> = None;
        let mut max_mint: Option<u128> = None;
        for i in 0..tc {
            let vault = &accounts[6 + tc + i];
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
            max_mint = Some(match max_mint {
                Some(current) => current.max(candidate),
                None => candidate,
            });
        }

        let lo = min_mint.ok_or(VaultError::DivisionByZero)?;
        let hi = max_mint.ok_or(VaultError::DivisionByZero)?;
        if lo == 0 {
            return Err(VaultError::ZeroDeposit.into());
        }
        // NAV deviation: (hi - lo) / lo > MAX_NAV_DEVIATION_BPS / 10_000
        // Rearranged as (hi - lo) * 10_000 > lo * MAX to stay in u128.
        let spread = hi.checked_sub(lo).ok_or(VaultError::Overflow)?;
        if spread
            .checked_mul(10_000)
            .ok_or(VaultError::Overflow)?
            > lo
                .checked_mul(MAX_NAV_DEVIATION_BPS as u128)
                .ok_or(VaultError::Overflow)?
        {
            return Err(VaultError::NavDeviationExceeded.into());
        }
        lo as u64
    };

    // Slippage check (fires before any transfer — cheap failure on stale quotes)
    if mint_amount < min_mint_out {
        return Err(VaultError::SlippageExceeded.into());
    }

    // Compute fee
    let fee_amount = mint_amount
        .checked_mul(fee_bps as u64)
        .ok_or(VaultError::Overflow)?
        / 10_000;
    let net_mint = mint_amount
        .checked_sub(fee_amount)
        .ok_or(VaultError::Overflow)?;

    // Transfer basket tokens from depositor to vaults
    for i in 0..tc {
        let source = &accounts[6 + i];
        let vault = &accounts[6 + tc + i];
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
        amount: net_mint,
    }
    .invoke_signed(&[Signer::from(&mint_signer_seeds)])?;

    // Mint fee to treasury
    if fee_amount > 0 {
        MintTo {
            mint: etf_mint_ai,
            account: treasury_etf_ata,
            mint_authority: etf_state_ai,
            amount: fee_amount,
        }
        .invoke_signed(&[Signer::from(&mint_signer_seeds)])?;
    }

    // Update total supply (full mint_amount including fee)
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

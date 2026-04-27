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

/// SweepTreasury — burn the treasury's entire ETF balance and return
/// its proportional share of basket tokens back to the treasury.
///
/// This is the treasury's redemption path: `Deposit` and `Withdraw` both
/// accrue ETF tokens into the treasury's ETF ATA via the 30 bps fee. Left
/// alone, those tokens sit forever. A regular `Withdraw` by the treasury
/// would work but it would charge the fee back to itself (a circular
/// 30 bps round-trip with no real economic effect, but uglier accounting).
/// `SweepTreasury` is the fee-free variant for exactly this case.
///
/// No slippage guard: the treasury accepts whatever proportional share
/// the vault composition yields at the moment of the sweep. Typically
/// called by a cranker right after accumulation crosses a threshold.
///
/// Accounts:
///   0: [signer]    treasury (must match `etf.treasury`)
///   1: [writable]  etf_state PDA
///   2: [writable]  etf_mint
///   3: [writable]  treasury_etf_ata (source of burn; owner == treasury)
///   4: []          token_program
///   5..5+N: [writable] vaults (source basket tokens)
///   5+N..5+2N: [writable] treasury_basket_atas (destination; owner == treasury)
///
/// Data: [name_len: u8][name: bytes] — same PDA-seed convention as
/// Deposit/Withdraw so the EtfState PDA can sign the vault → treasury
/// `Transfer`s.
pub fn process_sweep_treasury(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    name: &[u8],
) -> ProgramResult {
    let treasury_signer = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let treasury_etf_ata = &accounts[3];
    let _tok = &accounts[4];

    if !treasury_signer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    // Load the ETF state once and pull everything we need — the borrow
    // must drop before any CPI or second borrow.
    let (tc, total_supply, authority, bump, treasury, etf_mint, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf = unsafe { load::<EtfState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        if etf.total_supply == 0 {
            return Err(VaultError::DivisionByZero.into());
        }
        (
            etf.token_count as usize,
            etf.total_supply,
            etf.authority,
            etf.bump,
            etf.treasury,
            etf.etf_mint,
            etf.token_vaults,
        )
    };

    // Only the stored treasury pubkey may trigger a sweep. Using the
    // signer key means this works whether the treasury is an EOA or a
    // Squads vault — the multisig signs the tx, and the derived signer
    // is what SPL Token sees.
    if treasury_signer.key() != &treasury {
        return Err(VaultError::SweepForbidden.into());
    }

    if etf_mint_ai.key() != &etf_mint {
        return Err(VaultError::MintMismatch.into());
    }

    // treasury_etf_ata — Token Program account whose stored owner is the
    // treasury pubkey. Mirrors the deposit/withdraw fee-dest guard.
    if treasury_etf_ata.owner() != &TOKEN_PROGRAM_ID {
        return Err(VaultError::TreasuryMismatch.into());
    }
    let burn_amount = {
        let data = treasury_etf_ata.try_borrow_data()?;
        if data.len() < 72 {
            return Err(ProgramError::InvalidAccountData);
        }
        if &data[32..64] != &treasury {
            return Err(VaultError::TreasuryMismatch.into());
        }
        u64::from_le_bytes(
            data[64..72]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        )
    };

    if burn_amount == 0 {
        return Err(VaultError::NothingToSweep.into());
    }
    if burn_amount > total_supply {
        // Defensive: total_supply tracks the program's internal view of
        // circulating ETF. If the treasury somehow holds more than
        // total_supply (shouldn't happen under normal flows) we refuse
        // rather than underflow on the subtract at the bottom.
        return Err(VaultError::InsufficientBalance.into());
    }

    if accounts.len() < 5 + tc * 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // Match vaults to stored token_vaults AND verify SPL Token ownership.
    // vault_balance is read from data[64..72] below; without the owner
    // check, a crafted account with the right key but different owner
    // could feed arbitrary bytes into the proportional-payout math.
    for i in 0..tc {
        let vault = &accounts[5 + i];
        if vault.key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
        if vault.owner() != &TOKEN_PROGRAM_ID {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    // Match destinations: must be Token Program accounts whose stored
    // owner is the treasury. Prevents someone routing the basket payout
    // to an attacker-owned ATA via a crafted account list.
    for i in 0..tc {
        let dest = &accounts[5 + tc + i];
        if dest.owner() != &TOKEN_PROGRAM_ID {
            return Err(VaultError::TreasuryMismatch.into());
        }
        let data = dest.try_borrow_data()?;
        if data.len() < 64 {
            return Err(ProgramError::InvalidAccountData);
        }
        if &data[32..64] != &treasury {
            return Err(VaultError::TreasuryMismatch.into());
        }
    }

    // Compute per-vault payouts up front so we can detect zero-output
    // edge cases before we start moving tokens.
    let mut per_vault_amount = [0u64; 5];
    for i in 0..tc {
        let vault = &accounts[5 + i];
        let data = vault.try_borrow_data()?;
        if data.len() < 72 {
            return Err(ProgramError::InvalidAccountData);
        }
        let vault_balance = u64::from_le_bytes(
            data[64..72]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );
        let amount_out = (vault_balance as u128)
            .checked_mul(burn_amount as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(total_supply as u128)
            .ok_or(VaultError::DivisionByZero)? as u64;
        per_vault_amount[i] = amount_out;
    }

    // Burn the ETF tokens. Treasury signs as the account authority.
    Burn {
        account: treasury_etf_ata,
        mint: etf_mint_ai,
        authority: treasury_signer,
        amount: burn_amount,
    }
    .invoke()?;

    // Transfer basket tokens from vaults to treasury destinations.
    // EtfState PDA signs as vault authority.
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
        let amount_out = per_vault_amount[i];
        if amount_out > 0 {
            Transfer {
                from: vault,
                to: dest,
                authority: etf_state_ai,
                amount: amount_out,
            }
            .invoke_signed(&[Signer::from(&vault_signer_seeds)])?;
        }
    }

    // total_supply decreases by the burned amount. Consistent with
    // Withdraw's bookkeeping — the sweep removes burn_amount ETF tokens
    // from circulation.
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        etf.total_supply = etf
            .total_supply
            .checked_sub(burn_amount)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}

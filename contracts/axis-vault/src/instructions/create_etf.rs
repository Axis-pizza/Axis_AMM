use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::{InitializeAccount3, InitializeMint2};

use crate::error::VaultError;
use crate::state::{load, load_mut, EtfState, MAX_BASKET_TOKENS};

/// CreateEtf — initialize an ETF vault with a basket of tokens.
///
/// Creates:
///   1. EtfState PDA (stores basket config)
///   2. SPL token mint for the ETF token (EtfState PDA is mint authority)
///   3. Vault token accounts for each basket token (EtfState PDA is owner)
///
/// Accounts:
///   0: [signer, writable] authority (creator, pays rent)
///   1: [writable]          etf_state PDA
///   2: [writable]          etf_mint (uninitialized, will become SPL mint)
///   3: []                  treasury
///   4: []                  system_program
///   5: []                  token_program
///   6..6+N: []             basket token mints
///   6+N..6+2N: [writable]  basket vault accounts (uninitialized)
///
/// Data: [token_count: u8][weights_bps: [u16; N]][name: up to 32 bytes]
pub fn process_create_etf(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    token_count: u8,
    weights_bps: &[u16],
    name: &[u8],
) -> ProgramResult {
    let tc = token_count as usize;
    if tc < 2 || tc > MAX_BASKET_TOKENS {
        return Err(VaultError::InvalidBasketSize.into());
    }
    if weights_bps.len() != tc {
        return Err(VaultError::WeightsMismatch.into());
    }
    let weight_sum: u32 = weights_bps.iter().map(|&w| w as u32).sum();
    if weight_sum != 10_000 {
        return Err(VaultError::WeightsMismatch.into());
    }
    if name.is_empty() || name.len() > 32 {
        return Err(VaultError::InvalidTickerLength.into());
    }

    let min_accounts = 6 + tc * 2;
    if accounts.len() < min_accounts {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];
    let etf_mint_ai = &accounts[2];
    let treasury_ai = &accounts[3];
    let _sys = &accounts[4];
    let _tok = &accounts[5];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Derive EtfState PDA: [b"etf", authority, name]
    let (expected_pda, pda_bump) = pubkey::find_program_address(
        &[b"etf", authority.key(), name],
        program_id,
    );
    if etf_state_ai.key() != &expected_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Check not already initialized
    {
        let data = etf_state_ai.try_borrow_data()?;
        if data.len() >= 8 && data[..8] == EtfState::DISCRIMINATOR {
            return Err(VaultError::AlreadyInitialized.into());
        }
    }

    let rent = Rent::get()?;

    // Create EtfState account
    let bump_seed = [pda_bump];
    let etf_signer_seeds = [
        Seed::from(b"etf".as_ref()),
        Seed::from(authority.key().as_ref()),
        Seed::from(name),
        Seed::from(bump_seed.as_ref()),
    ];

    CreateAccount {
        from: authority,
        to: etf_state_ai,
        lamports: rent.minimum_balance(EtfState::LEN),
        space: EtfState::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&etf_signer_seeds)])?;

    // Initialize the ETF SPL token mint (6 decimals, EtfState PDA as authority)
    InitializeMint2 {
        mint: etf_mint_ai,
        decimals: 6,
        mint_authority: &expected_pda,
        freeze_authority: None,
    }
    .invoke()?;

    // Initialize vault token accounts (EtfState PDA as owner)
    let mut token_mints = [[0u8; 32]; MAX_BASKET_TOKENS];
    let mut token_vaults = [[0u8; 32]; MAX_BASKET_TOKENS];

    for i in 0..tc {
        let basket_mint = &accounts[6 + i];
        let vault = &accounts[6 + tc + i];

        InitializeAccount3 {
            account: vault,
            mint: basket_mint,
            owner: &expected_pda,
        }
        .invoke()?;

        token_mints[i] = *basket_mint.key();
        token_vaults[i] = *vault.key();
    }

    // Check for duplicate mints in basket
    for i in 0..tc {
        for j in (i + 1)..tc {
            if token_mints[i] == token_mints[j] {
                return Err(VaultError::DuplicateMint.into());
            }
        }
    }

    // Write EtfState
    {
        let mut data = etf_state_ai.try_borrow_mut_data()?;
        let etf = unsafe { load_mut::<EtfState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;

        etf.discriminator = EtfState::DISCRIMINATOR;
        etf.authority = *authority.key();
        etf.etf_mint = *etf_mint_ai.key();
        etf.token_count = token_count;
        etf.token_mints = token_mints;
        etf.token_vaults = token_vaults;
        let mut wb = [0u16; MAX_BASKET_TOKENS];
        for i in 0..tc { wb[i] = weights_bps[i]; }
        etf.weights_bps = wb;
        etf.total_supply = 0;
        etf.treasury = *treasury_ai.key();
        etf.fee_bps = 30;
        etf.paused = 0;
        etf.bump = pda_bump;
        etf._padding = [0; 4];
    }

    Ok(())
}

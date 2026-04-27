use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::{InitializeAccount3, InitializeMint2};

use crate::constants::{
    protocol_treasury_is_active, DEFAULT_FEE_BPS, DEFAULT_MAX_FEE_BPS, PROTOCOL_TREASURY,
};
use crate::error::VaultError;
use crate::state::{
    load_mut, EtfState, MAX_BASKET_TOKENS, MAX_ETF_NAME_LEN, MAX_ETF_TICKER_LEN,
};

/// CreateEtf — initialize an ETF vault with a basket of tokens.
///
/// Creates:
///   1. EtfState PDA (stores basket config + metadata)
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
/// Data:
///   [token_count: u8]
///   [weights_bps: [u16 LE; N]]
///   [ticker_len: u8][ticker: bytes (2..=16, ASCII upper/digit)]
///   [name_len: u8][name: bytes (1..=32, UTF-8; also used as PDA seed)]
pub fn process_create_etf(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    token_count: u8,
    weights_bps: &[u16],
    ticker: &[u8],
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

    // Name: 1..=32 bytes, valid UTF-8. Stored on-chain as-is and reused
    // as the `name` PDA seed; UTF-8 validation keeps wallets/explorers
    // from rendering garbage.
    if name.is_empty() || name.len() > MAX_ETF_NAME_LEN {
        return Err(VaultError::InvalidName.into());
    }
    if core::str::from_utf8(name).is_err() {
        return Err(VaultError::InvalidName.into());
    }

    // Ticker: 2..=16 bytes, ASCII uppercase A-Z or digits 0-9. Mirrors
    // traditional-finance ticker conventions; no lowercase, spaces, or
    // symbols. Reject here rather than silently normalizing so on-chain
    // state is a faithful record of what the creator signed.
    if ticker.len() < 2 || ticker.len() > MAX_ETF_TICKER_LEN {
        return Err(VaultError::InvalidTicker.into());
    }
    for &b in ticker {
        let ascii_upper = (b'A'..=b'Z').contains(&b);
        let ascii_digit = (b'0'..=b'9').contains(&b);
        if !(ascii_upper || ascii_digit) {
            return Err(VaultError::InvalidTicker.into());
        }
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

    // #38 governance gate: once PROTOCOL_TREASURY is flipped from zeros to
    // the deployed Squads V4 multisig, every ETF must route fees there. The
    // gate is inert while the constant is all-zeros so devnet tests using
    // throwaway treasuries still work until ops is ready.
    if protocol_treasury_is_active() && treasury_ai.key() != &PROTOCOL_TREASURY {
        return Err(VaultError::TreasuryNotApproved.into());
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

    // Capture creation slot for on-chain provenance (issue #37). Reading
    // Clock before we take the mutable state borrow keeps the CPI-free
    // region tight.
    let created_at_slot = Clock::get()?.slot;

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
        etf.fee_bps = DEFAULT_FEE_BPS;
        etf.paused = 0;
        etf.bump = pda_bump;
        // SetFee gate: authority can change fee_bps within
        // [0, max_fee_bps]. Hard-set at create time, not adjustable
        // afterwards — locks the worst-case fee for this ETF.
        etf.max_fee_bps = DEFAULT_MAX_FEE_BPS;
        etf._pad = [0; 2];
        // TVL cap: 0 = uncapped. Authority opts in via SetCap once a
        // ramp curve is decided. Off by default for backwards-compat
        // with the existing test flow.
        etf.tvl_cap = 0;

        // Zero-pad name + ticker into their fixed-size slots so clients
        // can decode a deterministic blob regardless of actual length.
        let mut name_buf = [0u8; MAX_ETF_NAME_LEN];
        name_buf[..name.len()].copy_from_slice(name);
        etf.name = name_buf;

        let mut ticker_buf = [0u8; MAX_ETF_TICKER_LEN];
        ticker_buf[..ticker.len()].copy_from_slice(ticker);
        etf.ticker = ticker_buf;

        etf.created_at_slot = created_at_slot;
        etf._padding = [0; 4];
    }

    Ok(())
}

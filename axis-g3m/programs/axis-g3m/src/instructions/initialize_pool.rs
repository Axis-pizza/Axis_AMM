use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::Transfer;

use crate::error::G3mError;
use crate::state::{G3mPoolState, MAX_POOL_TOKENS};

/// InitializePool — create a G3M pool with up to 5 tokens.
///
/// Accounts:
///   0: authority          (signer, mutable, pays rent)
///   1: pool_state         (writable, PDA: [b"g3m_pool", authority])
///   2: system_program
///   3: token_program
///   4..4+N: source token accounts (authority-owned, one per token)
///   4+N..4+2N: vault token accounts (pool-PDA-owned, one per token)
///
/// Instruction data (after 1-byte discriminant):
///   [0]:       token_count: u8
///   [1..3]:    fee_rate_bps: u16 LE
///   [3..5]:    drift_threshold_bps: u16 LE
///   [5..13]:   rebalance_cooldown: u64 LE
///   [13..13+N*2]:  weights_bps: [u16 LE; N]
///   [13+N*2..13+N*2+N*8]: initial_reserves: [u64 LE; N]
pub fn process_initialize_pool(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    token_count: u8,
    fee_rate_bps: u16,
    drift_threshold_bps: u16,
    rebalance_cooldown: u64,
    weights_bps: &[u16],
    initial_reserves: &[u64],
) -> ProgramResult {
    let tc = token_count as usize;

    // Validate token count
    if tc < 2 || tc > MAX_POOL_TOKENS {
        return Err(G3mError::InvalidTokenCount.into());
    }
    if weights_bps.len() != tc || initial_reserves.len() != tc {
        return Err(G3mError::InvalidTokenCount.into());
    }

    // Validate weights sum to 10_000
    let weight_sum: u32 = weights_bps.iter().map(|&w| w as u32).sum();
    if weight_sum != 10_000 {
        return Err(G3mError::WeightsMismatch.into());
    }

    // Validate fee
    if fee_rate_bps > 1_000 {
        return Err(G3mError::InvalidFeeRate.into());
    }

    // Validate reserves
    for i in 0..tc {
        if initial_reserves[i] == 0 {
            return Err(G3mError::ZeroReserve.into());
        }
    }

    // Accounts
    let authority = &accounts[0];
    let pool_account = &accounts[1];
    let _system_program = &accounts[2];
    let _token_program = &accounts[3];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Derive PDA
    let (pda, bump) = pinocchio::pubkey::find_program_address(
        &[b"g3m_pool", authority.key().as_ref()],
        program_id,
    );
    if pool_account.key() != &pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create pool account via CPI
    let bump_bytes = [bump];
    let signer_seeds = [
        Seed::from(b"g3m_pool".as_slice()),
        Seed::from(authority.key().as_ref()),
        Seed::from(&bump_bytes),
    ];
    let signers = [Signer::from(&signer_seeds)];

    CreateAccount {
        from: authority,
        to: pool_account,
        lamports: Rent::get()?.minimum_balance(G3mPoolState::LEN),
        space: G3mPoolState::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    // Transfer initial reserves from authority to vaults
    for i in 0..tc {
        let source = &accounts[4 + i];
        let vault = &accounts[4 + tc + i];

        Transfer {
            from: source,
            to: vault,
            authority: authority,
            amount: initial_reserves[i],
        }
        .invoke()?;
    }

    // Initialize pool state
    let data = pool_account.try_borrow_mut_data()?;
    let pool = unsafe { &mut *(data.as_ptr() as *mut G3mPoolState) };

    pool.discriminator = G3mPoolState::DISCRIMINATOR;
    pool.authority.copy_from_slice(authority.key().as_ref());
    pool.token_count = token_count;

    for i in 0..tc {
        let vault = &accounts[4 + tc + i];
        // Get mint from the source token account (offset 0, 32 bytes in SPL token layout)
        let source_data = accounts[4 + i].try_borrow_data()?;
        pool.token_mints[i].copy_from_slice(&source_data[0..32]);
        pool.token_vaults[i].copy_from_slice(vault.key().as_ref());
        pool.target_weights_bps[i] = weights_bps[i];
        pool.reserves[i] = initial_reserves[i];
    }

    // Zero out unused slots
    for i in tc..MAX_POOL_TOKENS {
        pool.token_mints[i] = [0u8; 32];
        pool.token_vaults[i] = [0u8; 32];
        pool.target_weights_bps[i] = 0;
        pool.reserves[i] = 0;
    }

    // Compute initial invariant
    let k = crate::math::compute_invariant(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
    )
    .ok_or(ProgramError::from(G3mError::Overflow))?;

    pool.set_invariant_k(k);
    pool.fee_rate_bps = fee_rate_bps;
    pool.drift_threshold_bps = drift_threshold_bps;
    pool.last_rebalance_slot = Clock::get()?.slot;
    pool.rebalance_cooldown = rebalance_cooldown;
    pool.paused = 0;
    pool.bump = bump;
    pool._padding = [0u8; 6];

    Ok(())
}

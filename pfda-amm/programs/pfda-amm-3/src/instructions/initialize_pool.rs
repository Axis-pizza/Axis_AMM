use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::InitializeAccount3;

use crate::error::Pfda3Error;
use crate::state::{load_mut, BatchQueue3, PoolState3};

/// Accounts:
/// 0: [signer, writable] payer
/// 1: [writable]          pool_state PDA
/// 2: [writable]          batch_queue PDA (batch_id=0)
/// 3: []                  token_mint_0 (SOL)
/// 4: []                  token_mint_1 (BONK)
/// 5: []                  token_mint_2 (WIF)
/// 6: [writable]          vault_0 (uninitialized SPL token account)
/// 7: [writable]          vault_1
/// 8: [writable]          vault_2
/// 9: []                  system_program
/// 10: []                 token_program
///
/// Data: [base_fee_bps: u16][window_slots: u64][w0: u32][w1: u32][w2: u32]
pub fn process_initialize_pool_3(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    base_fee_bps: u16,
    window_slots: u64,
    weights: [u32; 3],
) -> ProgramResult {
    if window_slots == 0 {
        return Err(Pfda3Error::InvalidWindowSlots.into());
    }

    let weight_sum: u64 = weights.iter().map(|&w| w as u64).sum();
    if weight_sum != 1_000_000 {
        return Err(Pfda3Error::InvalidWeight.into());
    }

    // Accounts: payer, pool, queue, mint0-2, vault0-2, treasury, system, token_program
    if accounts.len() < 12 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let payer = &accounts[0];
    let pool_ai = &accounts[1];
    let queue_ai = &accounts[2];
    let mint0 = &accounts[3];
    let mint1 = &accounts[4];
    let mint2 = &accounts[5];
    let vault0 = &accounts[6];
    let vault1 = &accounts[7];
    let vault2 = &accounts[8];
    let treasury_ai = &accounts[9];
    let _sys = &accounts[10];
    let _tok = &accounts[11];

    let mints = [mint0.key(), mint1.key(), mint2.key()];

    // Derive pool PDA: [b"pool3", mint0, mint1, mint2]
    let (expected_pool, pool_bump) = pubkey::find_program_address(
        &[b"pool3", mints[0], mints[1], mints[2]],
        program_id,
    );
    if pool_ai.key() != &expected_pool {
        return Err(ProgramError::InvalidSeeds);
    }

    // Check not initialized
    {
        let data = pool_ai.try_borrow_data()?;
        if data.len() >= 8 && data[..8] == PoolState3::DISCRIMINATOR {
            return Err(Pfda3Error::AlreadyInitialized.into());
        }
    }

    let clock = Clock::get()?;
    let rent = Rent::get()?;

    // Create pool account
    let pool_bump_seed = [pool_bump];
    let pool_signer = [
        Seed::from(b"pool3".as_ref()),
        Seed::from(mints[0].as_ref()),
        Seed::from(mints[1].as_ref()),
        Seed::from(mints[2].as_ref()),
        Seed::from(pool_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: payer,
        to: pool_ai,
        lamports: rent.minimum_balance(PoolState3::LEN),
        space: PoolState3::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&pool_signer)])?;

    // Initialize vault token accounts (pool PDA as owner)
    let vault_accounts = [vault0, vault1, vault2];
    let mint_accounts = [mint0, mint1, mint2];
    for i in 0..3 {
        InitializeAccount3 {
            account: vault_accounts[i],
            mint: mint_accounts[i],
            owner: &expected_pool,
        }
        .invoke()?;
    }

    // Write pool state
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;

        pool.discriminator = PoolState3::DISCRIMINATOR;
        for i in 0..3 {
            pool.token_mints[i] = *mints[i];
            pool.vaults[i] = *vault_accounts[i].key();
            pool.reserves[i] = 0;
            pool.weights[i] = weights[i];
        }
        pool.window_slots = window_slots;
        pool.current_batch_id = 0;
        pool.current_window_end = clock.slot + window_slots;
        pool.treasury = *treasury_ai.key();
        pool.authority = *payer.key();
        pool.base_fee_bps = base_fee_bps;
        pool.bump = pool_bump;
        pool.reentrancy_guard = 0;
        pool.paused = 0;
        pool._padding = [0; 3];
    }

    // Create batch queue for batch 0
    let batch_id_bytes = 0u64.to_le_bytes();
    let pool_key = *pool_ai.key();
    let (expected_queue, queue_bump) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if queue_ai.key() != &expected_queue {
        return Err(ProgramError::InvalidSeeds);
    }

    let queue_bump_seed = [queue_bump];
    let queue_signer = [
        Seed::from(b"queue3".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(batch_id_bytes.as_ref()),
        Seed::from(queue_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: payer,
        to: queue_ai,
        lamports: rent.minimum_balance(BatchQueue3::LEN),
        space: BatchQueue3::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&queue_signer)])?;

    {
        let mut data = queue_ai.try_borrow_mut_data()?;
        let queue = unsafe { load_mut::<BatchQueue3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        queue.discriminator = BatchQueue3::DISCRIMINATOR;
        queue.pool = pool_key;
        queue.batch_id = 0;
        queue.total_in = [0; 3];
        queue.window_end_slot = clock.slot + window_slots;
        queue.bump = queue_bump;
        queue._padding = [0; 7];
    }

    Ok(())
}

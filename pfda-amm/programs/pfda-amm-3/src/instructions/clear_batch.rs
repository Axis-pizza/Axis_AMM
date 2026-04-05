use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;

use crate::error::Pfda3Error;
use crate::state::{load, load_mut, BatchQueue3, ClearedBatchHistory3, PoolState3};

/// ClearBatch3 — O(1) clearing for a 3-token PFDA pool.
///
/// For 3 tokens with equal weights (33.3% each), the clearing prices
/// are derived from the reserves ratio. With Switchboard oracle feeds,
/// prices are bounded to ±5% of oracle-derived market prices.
///
/// Accounts:
/// 0: [signer, writable] cranker (searcher who won the Jito auction)
/// 1: [writable]          pool_state PDA
/// 2: [writable]          batch_queue PDA
/// 3: [writable]          history PDA (new)
/// 4: [writable]          new_queue PDA (batch_id+1)
/// 5: []                  system_program
/// 6: [] (optional)       oracle_feed_0 — Switchboard price feed for token 0
/// 7: [] (optional)       oracle_feed_1 — Switchboard price feed for token 1
/// 8: [] (optional)       oracle_feed_2 — Switchboard price feed for token 2
/// 9: [writable] (optional) treasury — receives bid payment from searcher
///
/// If bid_lamports > 0 and treasury account provided (account 9),
/// SOL is transferred from cranker to treasury as the searcher's payment
/// for exclusive clearing rights in this batch window.
pub fn process_clear_batch_3(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bid_lamports: u64,
) -> ProgramResult {
    let [cranker, pool_ai, queue_ai, history_ai, new_queue_ai, _sys, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !cranker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let current_slot = Clock::get()?.slot;

    // Load pool state
    let (batch_id, window_end, reserves, weights, window_slots, base_fee_bps, pool_key, treasury) = {
        let data = pool_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        if pool.reentrancy_guard != 0 {
            return Err(Pfda3Error::ReentrancyDetected.into());
        }
        if pool.paused != 0 {
            return Err(Pfda3Error::PoolPaused.into());
        }
        if current_slot < pool.current_window_end {
            return Err(Pfda3Error::BatchWindowNotEnded.into());
        }
        (
            pool.current_batch_id,
            pool.current_window_end,
            pool.reserves,
            pool.weights,
            pool.window_slots,
            pool.base_fee_bps,
            *pool_ai.key(),
            pool.treasury,
        )
    };

    // --- Jito bid payment ---
    // If bid_lamports > 0 and treasury account provided (account 9),
    // transfer SOL from cranker to treasury.
    if bid_lamports > 0 {
        if bid_lamports < crate::jito::MIN_BID_LAMPORTS {
            return Err(Pfda3Error::BidTooLow.into());
        }
        if accounts.len() > 9 {
            let treasury_ai = &accounts[9];
            // Validate treasury matches pool's configured treasury
            if treasury_ai.key().as_ref() != &treasury {
                return Err(Pfda3Error::TreasuryMismatch.into());
            }
            // Transfer SOL from cranker to treasury
            pinocchio_system::instructions::Transfer {
                from: cranker,
                to: treasury_ai,
                lamports: bid_lamports,
            }
            .invoke()?;

            // Compute revenue split for accounting (logged off-chain)
            let (_protocol_share, _lp_share) = crate::jito::compute_bid_split(
                bid_lamports,
                crate::jito::DEFAULT_ALPHA_BPS,
            );
        }
    }

    // Set reentrancy guard
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.reentrancy_guard = 1;
    }

    // Load batch queue
    let batch_id_bytes = batch_id.to_le_bytes();
    let (expected_queue, _) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if queue_ai.key() != &expected_queue {
        release_guard(pool_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let total_in = {
        let data = queue_ai.try_borrow_data()?;
        let queue = unsafe { load::<BatchQueue3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !queue.is_initialized() {
            release_guard(pool_ai)?;
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        queue.total_in
    };

    // --- Read oracle prices (if 3 oracle feeds provided: accounts 6, 7, 8) ---
    // Oracle prices are Q32.32 fixed-point (price per token in USD).
    // Max staleness: 100 slots (~40 seconds). Min 1 oracle sample.
    let oracle_prices: Option<[u64; 3]> = if accounts.len() > 8 {
        let mut prices = [0u64; 3];
        let mut all_ok = true;
        for i in 0..3 {
            let feed = &accounts[6 + i];
            match crate::oracle::read_switchboard_price(feed, current_slot, 100, 1) {
                Ok(p) => prices[i] = p,
                Err(_) => {
                    all_ok = false;
                    break;
                }
            }
        }
        if all_ok { Some(prices) } else { None }
    } else {
        None
    };

    // --- Compute clearing prices ---
    // Base: reserve-ratio derived prices (token i in numeraire = token 0).
    // With oracle: bound each clearing price to ±5% of oracle-derived price.
    let mut clearing_prices = [0u64; 3];

    let r0 = reserves[0].max(1) as u128;
    let w0 = weights[0].max(1) as u128;

    for i in 0..3 {
        let ri = reserves[i].max(1) as u128;
        let wi = weights[i].max(1) as u128;

        let reserve_price = if i == 0 {
            1u64 << 32 // 1.0 for the numeraire
        } else {
            ((r0 * wi * (1u128 << 32)) / (ri * w0)) as u64
        };

        // If oracle prices available, bound the clearing price to ±5% of oracle
        let effective_price = if let Some(ref oracle_px) = oracle_prices {
            if i == 0 {
                reserve_price // numeraire stays 1.0
            } else {
                // Oracle-derived relative price: price_i / price_0
                // Both are Q32.32, so ratio = (price_i << 32) / price_0
                let op_i = oracle_px[i] as u128;
                let op_0 = oracle_px[0].max(1) as u128;
                let oracle_rel = ((op_i << 32) / op_0) as u64;

                // Clamp reserve_price to within ±5% of oracle_rel
                let max_dev_bps: u64 = 500; // 5%
                let lower = oracle_rel.saturating_sub(oracle_rel * max_dev_bps / 10_000);
                let upper = oracle_rel.saturating_add(oracle_rel * max_dev_bps / 10_000);
                reserve_price.max(lower).min(upper)
            }
        } else {
            reserve_price
        };

        clearing_prices[i] = effective_price;
    }

    // Update reserves with batch inputs
    let mut total_out = [0u64; 3];
    let mut new_reserves = reserves;

    for i in 0..3 {
        if total_in[i] > 0 {
            new_reserves[i] = new_reserves[i].checked_add(total_in[i])
                .ok_or(Pfda3Error::Overflow)?;

            let value_in = (total_in[i] as u128) * (clearing_prices[i] as u128) >> 32;
            total_out[i] = value_in as u64;
        }
    }

    let rent = Rent::get()?;

    // Create history PDA
    let (expected_history, history_bump) = pubkey::find_program_address(
        &[b"history3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if history_ai.key() != &expected_history {
        release_guard(pool_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let history_bump_seed = [history_bump];
    let history_signer = [
        Seed::from(b"history3".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(batch_id_bytes.as_ref()),
        Seed::from(history_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: cranker,
        to: history_ai,
        lamports: rent.minimum_balance(ClearedBatchHistory3::LEN),
        space: ClearedBatchHistory3::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&history_signer)])?;

    {
        let mut data = history_ai.try_borrow_mut_data()?;
        let hist = unsafe { load_mut::<ClearedBatchHistory3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        hist.discriminator = ClearedBatchHistory3::DISCRIMINATOR;
        hist.pool = pool_key;
        hist.batch_id = batch_id;
        hist.clearing_prices = clearing_prices;
        hist.total_out = total_out;
        hist.total_in = total_in;
        hist.fee_bps = base_fee_bps;
        hist.is_cleared = true;
        hist.bump = history_bump;
        hist._padding = [0; 4];
    }

    // Create next batch queue
    let next_batch_id = batch_id.checked_add(1).ok_or(Pfda3Error::Overflow)?;
    let next_id_bytes = next_batch_id.to_le_bytes();
    let next_window_end = window_end + window_slots;

    let (expected_new_queue, new_queue_bump) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &next_id_bytes],
        program_id,
    );
    if new_queue_ai.key() != &expected_new_queue {
        release_guard(pool_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let new_queue_bump_seed = [new_queue_bump];
    let new_queue_signer = [
        Seed::from(b"queue3".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(next_id_bytes.as_ref()),
        Seed::from(new_queue_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: cranker,
        to: new_queue_ai,
        lamports: rent.minimum_balance(BatchQueue3::LEN),
        space: BatchQueue3::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&new_queue_signer)])?;

    {
        let mut data = new_queue_ai.try_borrow_mut_data()?;
        let queue = unsafe { load_mut::<BatchQueue3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        queue.discriminator = BatchQueue3::DISCRIMINATOR;
        queue.pool = pool_key;
        queue.batch_id = next_batch_id;
        queue.total_in = [0; 3];
        queue.window_end_slot = next_window_end;
        queue.bump = new_queue_bump;
        queue._padding = [0; 7];
    }

    // Update pool state and release reentrancy guard
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.reserves = new_reserves;
        pool.current_batch_id = next_batch_id;
        pool.current_window_end = next_window_end;
        pool.reentrancy_guard = 0;
    }

    Ok(())
}

fn release_guard(pool_ai: &AccountInfo) -> ProgramResult {
    let mut data = pool_ai.try_borrow_mut_data()?;
    let pool = unsafe { load_mut::<PoolState3>(&mut data) }
        .ok_or(ProgramError::InvalidAccountData)?;
    pool.reentrancy_guard = 0;
    Ok(())
}

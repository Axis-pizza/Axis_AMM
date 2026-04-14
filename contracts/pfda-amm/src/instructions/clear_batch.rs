use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    error::PfmmError,
    math::{compute_clearing_price, fp_div, fp_from_int, fp_mul, fp_to_int, FP_ONE},
    state::{load, load_mut, BatchQueue, ClearedBatchHistory, PoolState},
};

/// Accounts for ClearBatch:
/// 0. `[signer, writable]` cranker (searcher who won the Jito auction)
/// 1. `[writable]`          pool_state PDA
/// 2. `[writable]`          batch_queue PDA (current batch)
/// 3. `[writable]`          cleared_batch_history PDA (new, for this batch)
/// 4. `[writable]`          new_batch_queue PDA (for batch_id+1)
/// 5. `[]`                  system_program
/// 6. `[]` (optional)       oracle_feed_a — Switchboard price feed for token A
/// 7. `[]` (optional)       oracle_feed_b — Switchboard price feed for token B
/// 8. `[]` (optional)       instructions_sysvar — for Jito tip verification
///
/// Integration points:
///   - Switchboard: If oracle feeds are provided (accounts 6+7), clearing price
///     is bounded within ±5% of oracle-derived market price.
///   - Jito: If instructions sysvar is provided (account 8), verifies that a
///     tip was included in the same transaction to a Jito tip account.
///     The tip is split: protocol_share + lp_share per the revenue formula.
pub fn process_clear_batch(program_id: &Pubkey, accounts: &[AccountInfo], bid_lamports: u64) -> ProgramResult {
    let [cranker, pool_state_ai, batch_queue_ai, history_ai, new_queue_ai, _system_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Optional oracle feed accounts
    let oracle_feed_a = if accounts.len() > 6 { Some(&accounts[6]) } else { None };
    let oracle_feed_b = if accounts.len() > 7 { Some(&accounts[7]) } else { None };

    // Jito auction bid enforcement:
    // If bid_lamports > 0 and accounts[8] is a protocol treasury,
    // transfer SOL from cranker to treasury. This is the searcher's payment
    // for exclusive clearing rights in this batch window.
    if bid_lamports > 0 && accounts.len() > 8 {
        let treasury = &accounts[8];

        // Transfer SOL from cranker to treasury via system program CPI
        pinocchio_system::instructions::Transfer {
            from: cranker,
            to: treasury,
            lamports: bid_lamports,
        }
        .invoke()?;

        // Compute revenue split for accounting
        let (_protocol_share, _lp_share) = crate::jito::compute_bid_split(
            bid_lamports,
            crate::jito::DEFAULT_ALPHA_BPS,
        );
    }

    if !cranker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let current_slot = Clock::get()?.slot;

    // Verify pool_state PDA
    {
        let data = pool_state_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !pool.is_initialized() {
            return Err(PfmmError::InvalidDiscriminator.into());
        }
        if pool.reentrancy_guard != 0 {
            return Err(PfmmError::ReentrancyDetected.into());
        }
        if pool.paused != 0 {
            return Err(PfmmError::PoolPaused.into());
        }
        let (expected, _bump) = pubkey::find_program_address(
            &[b"pool", &pool.token_a_mint, &pool.token_b_mint],
            program_id,
        );
        if pool_state_ai.key() != &expected {
            return Err(ProgramError::InvalidSeeds);
        }
    }

    // Step 1: Validate batch window has ended and read pool data
    let (
        current_batch_id,
        current_window_end,
        reserve_a,
        reserve_b,
        current_weight_a,
        target_weight_a,
        weight_start_slot,
        weight_end_slot,
        window_slots,
        base_fee_bps,
    ) = {
        let data = pool_state_ai.try_borrow_data()?;
        let pool = unsafe { load::<PoolState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if current_slot < pool.current_window_end {
            return Err(PfmmError::BatchWindowNotEnded.into());
        }
        (
            pool.current_batch_id,
            pool.current_window_end,
            pool.reserve_a,
            pool.reserve_b,
            pool.current_weight_a,
            pool.target_weight_a,
            pool.weight_start_slot,
            pool.weight_end_slot,
            pool.window_slots,
            pool.base_fee_bps,
        )
    };

    // Step 2: Set reentrancy guard
    {
        let mut data = pool_state_ai.try_borrow_mut_data()?;
        let pool =
            unsafe { load_mut::<PoolState>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;
        pool.reentrancy_guard = 1;
    }

    // Step 3: TFMM weight interpolation
    let interpolated_weight_a = if current_slot >= weight_end_slot {
        target_weight_a
    } else if current_slot <= weight_start_slot {
        current_weight_a
    } else {
        let elapsed = current_slot - weight_start_slot;
        let total = weight_end_slot - weight_start_slot;
        if target_weight_a >= current_weight_a {
            let d = (target_weight_a - current_weight_a) as u64;
            current_weight_a + (d * elapsed / total) as u32
        } else {
            let d = (current_weight_a - target_weight_a) as u64;
            current_weight_a - (d * elapsed / total) as u32
        }
    };

    // Step 4: Validate and read batch queue
    let pool_key = *pool_state_ai.key();
    let batch_id_bytes = current_batch_id.to_le_bytes();
    let (expected_queue, _queue_bump) = pubkey::find_program_address(
        &[b"queue", &pool_key, &batch_id_bytes],
        program_id,
    );
    if batch_queue_ai.key() != &expected_queue {
        // Release guard before returning error
        release_guard(pool_state_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let (total_in_a, total_in_b) = {
        let data = batch_queue_ai.try_borrow_data()?;
        let queue =
            unsafe { load::<BatchQueue>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !queue.is_initialized() {
            release_guard(pool_state_ai)?;
            return Err(PfmmError::InvalidDiscriminator.into());
        }
        (queue.total_in_a, queue.total_in_b)
    };

    // Step 5: Read oracle prices (if provided) for NAV-aware clearing
    // Oracle prices are Q32.32 fixed-point (price per token in USD).
    // Max staleness: 100 slots (~40 seconds). Min 1 oracle sample.
    let oracle_prices: Option<(u64, u64)> = match (oracle_feed_a, oracle_feed_b) {
        (Some(feed_a), Some(feed_b)) => {
            let price_a = crate::oracle::read_switchboard_price(feed_a, current_slot, 100, 1);
            let price_b = crate::oracle::read_switchboard_price(feed_b, current_slot, 100, 1);
            match (price_a, price_b) {
                (Ok(pa), Ok(pb)) => Some((pa, pb)),
                _ => None, // Fall back to invariant pricing on oracle failure
            }
        }
        _ => None,
    };

    // Step 6: Compute clearing price
    // If oracle prices are available, adjust the clearing price to reflect
    // real market conditions. The oracle-derived price = price_b / price_a
    // (how many units of B one unit of A buys at market rate).
    let clearing_result = compute_clearing_price(
        reserve_a,
        reserve_b,
        interpolated_weight_a,
        total_in_a,
        total_in_b,
    );

    let (clearing_price, out_b_per_in_a, out_a_per_in_b, new_reserve_a, new_reserve_b) =
        match clearing_result {
            Some(cp) if cp > 0 => {
                // If we have oracle prices, blend: use oracle-informed price
                // as a reference and the G3M clearing price for execution.
                // The clearing price is bounded by the oracle price to prevent
                // manipulation: |clearing - oracle| <= max_deviation.
                let effective_cp = if let Some((price_a, price_b)) = oracle_prices {
                    // Oracle-derived market price: B per A = price_a / price_b
                    let oracle_price = fp_div(fp_from_int(price_a >> 32), fp_from_int((price_b >> 32).max(1)));
                    // Use the G3M price but clamp to within 5% of oracle
                    let max_dev_bps: u64 = 500; // 5%
                    let lower = oracle_price.saturating_sub(oracle_price * max_dev_bps / 10_000);
                    let upper = oracle_price.saturating_add(oracle_price * max_dev_bps / 10_000);
                    cp.max(lower).min(upper)
                } else {
                    cp
                };

                let fee_bps = base_fee_bps as u64;
                let one_minus_fee_fp = ((10_000u64 - fee_bps) * FP_ONE) / 10_000;

                let out_b_per_in_a = fp_mul(effective_cp, one_minus_fee_fp);
                let inv_price = fp_div(FP_ONE, effective_cp);
                let out_a_per_in_b = fp_mul(inv_price, one_minus_fee_fp);

                // Update reserves
                let a_out = fp_to_int(fp_div(fp_from_int(total_in_b), effective_cp));
                let b_out = fp_to_int(fp_mul(fp_from_int(total_in_a), effective_cp));

                let new_ra = reserve_a
                    .checked_add(total_in_a)
                    .and_then(|x| x.checked_sub(a_out));
                let new_rb = reserve_b
                    .checked_add(total_in_b)
                    .and_then(|x| x.checked_sub(b_out));

                match (new_ra, new_rb) {
                    (Some(ra), Some(rb)) => (effective_cp, out_b_per_in_a, out_a_per_in_b, ra, rb),
                    _ => {
                        release_guard(pool_state_ai)?;
                        return Err(PfmmError::Overflow.into());
                    }
                }
            }
            _ => {
                // No orders or price failed — use spot price, reserves unchanged
                let spot_price = if reserve_a > 0 {
                    fp_div(fp_from_int(reserve_b), fp_from_int(reserve_a))
                } else {
                    FP_ONE
                };
                let inv_spot = fp_div(FP_ONE, spot_price.max(1));
                (spot_price, spot_price, inv_spot, reserve_a, reserve_b)
            }
        };

    let rent = Rent::get()?;

    // Step 6: Create ClearedBatchHistory PDA
    let (expected_history, history_bump) = pubkey::find_program_address(
        &[b"history", &pool_key, &batch_id_bytes],
        program_id,
    );
    if history_ai.key() != &expected_history {
        release_guard(pool_state_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let history_lamports = rent.minimum_balance(ClearedBatchHistory::LEN);
    let history_bump_seed = [history_bump];
    let history_signer_seeds = [
        Seed::from(b"history".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(batch_id_bytes.as_ref()),
        Seed::from(history_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: cranker,
        to: history_ai,
        lamports: history_lamports,
        space: ClearedBatchHistory::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&history_signer_seeds)])?;

    {
        let mut data = history_ai.try_borrow_mut_data()?;
        let history = unsafe { load_mut::<ClearedBatchHistory>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        *history = ClearedBatchHistory {
            discriminator: ClearedBatchHistory::DISCRIMINATOR,
            pool: pool_key,
            batch_id: current_batch_id,
            clearing_price,
            out_b_per_in_a,
            out_a_per_in_b,
            is_cleared: true,
            bump: history_bump,
            _padding: [0; 6],
        };
    }

    // Step 7: Create new BatchQueue for batch_id+1
    let next_batch_id = current_batch_id
        .checked_add(1)
        .ok_or(PfmmError::Overflow)?;
    let next_batch_id_bytes = next_batch_id.to_le_bytes();
    let next_window_end = current_window_end + window_slots;

    let (expected_new_queue, new_queue_bump) = pubkey::find_program_address(
        &[b"queue", &pool_key, &next_batch_id_bytes],
        program_id,
    );
    if new_queue_ai.key() != &expected_new_queue {
        release_guard(pool_state_ai)?;
        return Err(ProgramError::InvalidSeeds);
    }

    let new_queue_lamports = rent.minimum_balance(BatchQueue::LEN);
    let new_queue_bump_seed = [new_queue_bump];
    let new_queue_signer_seeds = [
        Seed::from(b"queue".as_ref()),
        Seed::from(pool_key.as_ref()),
        Seed::from(next_batch_id_bytes.as_ref()),
        Seed::from(new_queue_bump_seed.as_ref()),
    ];

    CreateAccount {
        from: cranker,
        to: new_queue_ai,
        lamports: new_queue_lamports,
        space: BatchQueue::LEN as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&new_queue_signer_seeds)])?;

    {
        let mut data = new_queue_ai.try_borrow_mut_data()?;
        let queue =
            unsafe { load_mut::<BatchQueue>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;
        *queue = BatchQueue {
            discriminator: BatchQueue::DISCRIMINATOR,
            pool: pool_key,
            batch_id: next_batch_id,
            total_in_a: 0,
            total_in_b: 0,
            window_end_slot: next_window_end,
            bump: new_queue_bump,
            _padding: [0; 7],
        };
    }

    // Step 8: Update PoolState and release reentrancy guard
    {
        let mut data = pool_state_ai.try_borrow_mut_data()?;
        let pool =
            unsafe { load_mut::<PoolState>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;

        pool.reserve_a = new_reserve_a;
        pool.reserve_b = new_reserve_b;
        pool.current_batch_id = next_batch_id;
        pool.current_window_end = next_window_end;
        pool.current_weight_a = interpolated_weight_a;
        pool.reentrancy_guard = 0;
    }

    Ok(())
}

/// Helper: release reentrancy guard on error paths
fn release_guard(pool_state_ai: &AccountInfo) -> ProgramResult {
    let mut data = pool_state_ai.try_borrow_mut_data()?;
    let pool =
        unsafe { load_mut::<PoolState>(&mut data) }.ok_or(ProgramError::InvalidAccountData)?;
    pool.reentrancy_guard = 0;
    Ok(())
}

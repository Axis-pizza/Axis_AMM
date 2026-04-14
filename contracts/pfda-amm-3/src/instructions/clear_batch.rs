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

    // --- Jito bid payment (before reentrancy guard — safe to fail freely) ---
    if bid_lamports > 0 {
        if bid_lamports < crate::jito::MIN_BID_LAMPORTS {
            return Err(Pfda3Error::BidTooLow.into());
        }
        // Enforce bid cap: bid must not exceed alpha% of estimated batch fees
        {
            let data = queue_ai.try_borrow_data()?;
            let queue = unsafe { load::<BatchQueue3>(&data) }
                .ok_or(ProgramError::InvalidAccountData)?;
            let total_volume: u128 = queue.total_in.iter()
                .fold(0u128, |acc, &x| acc.saturating_add(x as u128));
            let total_fees = total_volume
                .saturating_mul(base_fee_bps as u128)
                / 10_000;
            let max_bid = total_fees
                .saturating_mul(crate::jito::DEFAULT_ALPHA_BPS as u128)
                / 10_000;
            if (bid_lamports as u128) > max_bid {
                return Err(Pfda3Error::BidExcessive.into());
            }
        }
        if accounts.len() <= 9 {
            return Err(Pfda3Error::BidWithoutTreasury.into());
        }
        let treasury_ai = &accounts[9];
        if treasury_ai.key().as_ref() != &treasury {
            return Err(Pfda3Error::TreasuryMismatch.into());
        }
        pinocchio_system::instructions::Transfer {
            from: cranker,
            to: treasury_ai,
            lamports: bid_lamports,
        }
        .invoke()?;

        let (_ps, _ls) = crate::jito::compute_bid_split(
            bid_lamports,
            crate::jito::DEFAULT_ALPHA_BPS,
        );
    }

    // Set reentrancy guard
    {
        let mut data = pool_ai.try_borrow_mut_data()?;
        let pool = unsafe { load_mut::<PoolState3>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        pool.reentrancy_guard = 1;
    }

    // === All operations after this point must release the guard on error ===
    // Delegate to inner function; always release guard regardless of outcome.
    let result = clear_batch_inner(
        program_id, accounts, cranker, pool_ai, queue_ai, history_ai, new_queue_ai,
        current_slot, batch_id, window_end, reserves, weights, window_slots,
        base_fee_bps, pool_key, bid_lamports,
    );

    if result.is_err() {
        release_guard(pool_ai)?;
    }

    result
}

/// Inner clearing logic — all errors propagated with `?` are safe because
/// the caller guarantees `release_guard` on any `Err`.
#[inline(never)]
fn clear_batch_inner(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    cranker: &AccountInfo,
    pool_ai: &AccountInfo,
    queue_ai: &AccountInfo,
    history_ai: &AccountInfo,
    new_queue_ai: &AccountInfo,
    current_slot: u64,
    batch_id: u64,
    window_end: u64,
    reserves: [u64; 3],
    weights: [u32; 3],
    window_slots: u64,
    base_fee_bps: u16,
    pool_key: Pubkey,
    bid_lamports: u64,
) -> ProgramResult {
    // Load batch queue
    let batch_id_bytes = batch_id.to_le_bytes();
    let (expected_queue, _) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if queue_ai.key() != &expected_queue {
        return Err(ProgramError::InvalidSeeds);
    }

    let total_in = {
        let data = queue_ai.try_borrow_data()?;
        let queue = unsafe { load::<BatchQueue3>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !queue.is_initialized() {
            return Err(Pfda3Error::InvalidDiscriminator.into());
        }
        queue.total_in
    };

    // --- Read oracle prices (if 3 oracle feeds provided: accounts 6, 7, 8) ---
    let oracle_prices: Option<[u64; 3]> = if accounts.len() > 8 {
        let mut prices = [0u64; 3];
        for i in 0..3 {
            let feed = &accounts[6 + i];
            match crate::oracle::read_switchboard_price(feed, current_slot, 100, 1) {
                Ok(p) => prices[i] = p,
                Err(_) => {
                    return Err(Pfda3Error::OracleStale.into());
                }
            }
        }
        Some(prices)
    } else {
        None
    };

    // --- Compute clearing prices ---
    let mut clearing_prices = [0u64; 3];

    let r0 = reserves[0].max(1) as u128;
    let w0 = weights[0].max(1) as u128;

    for i in 0..3 {
        let ri = reserves[i].max(1) as u128;
        let wi = weights[i].max(1) as u128;

        let reserve_price = if i == 0 {
            1u64 << 32
        } else {
            ((r0 * wi * (1u128 << 32)) / (ri * w0)) as u64
        };

        let effective_price = if let Some(ref oracle_px) = oracle_prices {
            if i == 0 {
                reserve_price
            } else {
                let op_i = oracle_px[i] as u128;
                let op_0 = oracle_px[0].max(1) as u128;
                let oracle_rel = ((op_i << 32) / op_0) as u64;

                let max_dev_bps: u64 = 500;
                let lower = oracle_rel.saturating_sub(oracle_rel * max_dev_bps / 10_000);
                let upper = oracle_rel.saturating_add(oracle_rel * max_dev_bps / 10_000);
                reserve_price.max(lower).min(upper)
            }
        } else {
            reserve_price
        };

        clearing_prices[i] = effective_price;
    }

    // === Pre-clearing invariant snapshot ===
    // Use checked arithmetic — overflow is an error, not a silent pass.
    let pre_product: u128 = (reserves[0].max(1) as u128)
        .checked_mul(reserves[1].max(1) as u128)
        .and_then(|x| x.checked_mul(reserves[2].max(1) as u128))
        .ok_or(ProgramError::from(Pfda3Error::Overflow))?;

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

    // === Reserve adequacy check ===
    let total_value_in: u128 = {
        let mut v: u128 = 0;
        for i in 0..3 {
            v = v.checked_add(
                (total_in[i] as u128)
                    .checked_mul(clearing_prices[i] as u128)
                    .ok_or(Pfda3Error::Overflow)?
                    >> 32
            ).ok_or(Pfda3Error::Overflow)?;
        }
        v
    };

    if total_value_in > 0 {
        let fee_factor = 10_000u128.checked_sub(base_fee_bps as u128)
            .ok_or(Pfda3Error::Overflow)?;
        let max_claim_value = total_value_in
            .checked_mul(fee_factor)
            .ok_or(Pfda3Error::Overflow)?
            / 10_000;

        for j in 0..3 {
            let price_j = clearing_prices[j] as u128;
            if price_j == 0 {
                continue;
            }
            let max_outflow_j = (max_claim_value << 32)
                .checked_div(price_j)
                .ok_or(Pfda3Error::Overflow)?;
            if (new_reserves[j] as u128) < max_outflow_j {
                return Err(Pfda3Error::ReserveInsufficient.into());
            }
        }
    }

    // === Bid-to-volume validation ===
    if bid_lamports > 0 && total_value_in > 0 {
        // Safe truncation: cap at u64::MAX for ratio check
        let volume_u64 = if total_value_in > u64::MAX as u128 {
            u64::MAX
        } else {
            total_value_in as u64
        };
        crate::jito::validate_bid_against_volume(bid_lamports, volume_u64)?;
    }

    // === Post-clearing 3D invariant check ===
    let post_product: u128 = (new_reserves[0].max(1) as u128)
        .checked_mul(new_reserves[1].max(1) as u128)
        .and_then(|x| x.checked_mul(new_reserves[2].max(1) as u128))
        .ok_or(ProgramError::from(Pfda3Error::Overflow))?;

    if post_product < pre_product {
        return Err(Pfda3Error::InvariantViolation.into());
    }

    let rent = Rent::get()?;

    // Create history PDA
    let (expected_history, history_bump) = pubkey::find_program_address(
        &[b"history3", &pool_key, &batch_id_bytes],
        program_id,
    );
    if history_ai.key() != &expected_history {
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
    let next_window_end = window_end.checked_add(window_slots)
        .ok_or(Pfda3Error::Overflow)?;

    let (expected_new_queue, new_queue_bump) = pubkey::find_program_address(
        &[b"queue3", &pool_key, &next_id_bytes],
        program_id,
    );
    if new_queue_ai.key() != &expected_new_queue {
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

    // === Emit clearing metrics via return_data for A/B test analysis ===
    // Layout (57 bytes):
    //   [0..8]:   clearing_price_0 (u64 LE, Q32.32)
    //   [8..16]:  clearing_price_1
    //   [16..24]: clearing_price_2
    //   [24..32]: total_value_in_lo (u64 LE, low 8 bytes of numeraire units)
    //   [32..40]: bid_lamports (u64 LE)
    //   [40..48]: batch_id (u64 LE)
    //   [48..56]: slot (u64 LE)
    //   [56]:     oracle_used (u8, 0 or 1)
    let mut return_buf = [0u8; 57];
    return_buf[0..8].copy_from_slice(&clearing_prices[0].to_le_bytes());
    return_buf[8..16].copy_from_slice(&clearing_prices[1].to_le_bytes());
    return_buf[16..24].copy_from_slice(&clearing_prices[2].to_le_bytes());
    return_buf[24..32].copy_from_slice(&(total_value_in as u64).to_le_bytes());
    return_buf[32..40].copy_from_slice(&bid_lamports.to_le_bytes());
    return_buf[40..48].copy_from_slice(&batch_id.to_le_bytes());
    return_buf[48..56].copy_from_slice(&current_slot.to_le_bytes());
    return_buf[56] = if oracle_prices.is_some() { 1 } else { 0 };
    pinocchio::program::set_return_data(&return_buf);

    Ok(())
}

fn release_guard(pool_ai: &AccountInfo) -> ProgramResult {
    let mut data = pool_ai.try_borrow_mut_data()?;
    let pool = unsafe { load_mut::<PoolState3>(&mut data) }
        .ok_or(ProgramError::InvalidAccountData)?;
    pool.reentrancy_guard = 0;
    Ok(())
}

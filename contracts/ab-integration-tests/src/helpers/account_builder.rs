use solana_address::Address;

/// Build G3mPoolState raw bytes (455 bytes, repr(C)).
/// Layout: see contracts/axis-g3m/src/state/pool_state.rs
#[allow(clippy::too_many_arguments)]
pub fn build_g3m_pool_state(
    authority: &Address,
    token_count: u8,
    mints: &[Address],
    vaults: &[Address],
    weights_bps: &[u16],
    reserves: &[u64],
    fee_rate_bps: u16,
    drift_threshold_bps: u16,
    rebalance_cooldown: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 455];
    let tc = token_count as usize;

    d[0..8].copy_from_slice(b"g3mpool\0"); // discriminator
    d[8..40].copy_from_slice(authority.as_ref()); // authority
    d[40] = token_count; // token_count

    for i in 0..tc {
        let off = 41 + i * 32;
        d[off..off + 32].copy_from_slice(mints[i].as_ref()); // token_mints
    }
    for i in 0..tc {
        let off = 201 + i * 32;
        d[off..off + 32].copy_from_slice(vaults[i].as_ref()); // token_vaults
    }
    for i in 0..tc {
        let off = 361 + i * 2;
        d[off..off + 2].copy_from_slice(&weights_bps[i].to_le_bytes()); // target_weights_bps
    }
    for i in 0..tc {
        let off = 371 + i * 8;
        d[off..off + 8].copy_from_slice(&reserves[i].to_le_bytes()); // reserves
    }

    // invariant_k: simplified product for test setup
    // k = ∏ r_i for equal weights (good enough for seeding)
    let k: u128 = reserves.iter().fold(1u128 << 32, |acc, &r| {
        if r == 0 {
            return 0;
        }
        (acc * ((r as u128) << 32)) >> 32
    });
    d[411..419].copy_from_slice(&(k as u64).to_le_bytes()); // invariant_k_lo
    d[419..427].copy_from_slice(&((k >> 64) as u64).to_le_bytes()); // invariant_k_hi

    d[427..429].copy_from_slice(&fee_rate_bps.to_le_bytes()); // fee_rate_bps
    d[429..431].copy_from_slice(&drift_threshold_bps.to_le_bytes()); // drift_threshold_bps
                                                                     // last_rebalance_slot = 0 at offset 431 (already zeroed)
    d[439..447].copy_from_slice(&rebalance_cooldown.to_le_bytes()); // rebalance_cooldown
    d[447] = 0; // paused
    d[448] = bump;
    d
}

/// Build PoolState3 raw bytes for pfda-amm-3 (336 bytes, repr(C)).
///
/// Verified offsets from print_sizes test:
///   token_mints:        8
///   vaults:           104
///   reserves:         200
///   weights:          224
///   window_slots:     240 (offset 236 with 4-byte padding before u64)
///   current_batch_id: 248
///   current_window_end:256
///   treasury:         264
///   authority:        296
///   base_fee_bps:     328
///   bump:             330
///   reentrancy_guard: 331
///   paused:           332
///   _padding:         333..336
#[allow(clippy::too_many_arguments)]
pub fn build_pfda3_pool_state(
    mints: &[Address; 3],
    vaults: &[Address; 3],
    reserves: &[u64; 3],
    weights: &[u32; 3],
    window_slots: u64,
    current_batch_id: u64,
    current_window_end: u64,
    treasury: &Address,
    authority: &Address,
    base_fee_bps: u16,
    bump: u8,
) -> Vec<u8> {
    let size = 336;
    let mut d = vec![0u8; size];

    d[0..8].copy_from_slice(b"pool3st\0"); // 0: discriminator
    for i in 0..3 {
        let o = 8 + i * 32;
        d[o..o + 32].copy_from_slice(mints[i].as_ref());
    } // 8: mints
    for i in 0..3 {
        let o = 104 + i * 32;
        d[o..o + 32].copy_from_slice(vaults[i].as_ref());
    } // 104: vaults
    for i in 0..3 {
        let o = 200 + i * 8;
        d[o..o + 8].copy_from_slice(&reserves[i].to_le_bytes());
    } // 200: reserves
    for i in 0..3 {
        let o = 224 + i * 4;
        d[o..o + 4].copy_from_slice(&weights[i].to_le_bytes());
    } // 224: weights
      // 236: 4 bytes padding (repr(C) alignment for u64)
    d[240..248].copy_from_slice(&window_slots.to_le_bytes()); // 240
    d[248..256].copy_from_slice(&current_batch_id.to_le_bytes()); // 248
    d[256..264].copy_from_slice(&current_window_end.to_le_bytes()); // 256
    d[264..296].copy_from_slice(treasury.as_ref()); // 264
    d[296..328].copy_from_slice(authority.as_ref()); // 296
    d[328..330].copy_from_slice(&base_fee_bps.to_le_bytes()); // 328
    d[330] = bump; // 330
    d[331] = 0; // reentrancy_guard                                  // 331
    d[332] = 0; // paused                                            // 332
                // 333..336: padding (zeroed)
    d
}

/// Build PoolState raw bytes for pfda-amm (the 2-token legacy program).
/// Layout (240 bytes, repr(C)) per contracts/pfda-amm/src/state/pool_state.rs:
///   0:    discriminator "poolstat" (8)
///   8:    token_a_mint (32)
///  40:    token_b_mint (32)
///  72:    vault_a (32)
/// 104:    vault_b (32)
/// 136:    reserve_a (u64)
/// 144:    reserve_b (u64)
/// 152:    current_weight_a (u32)
/// 156:    target_weight_a (u32)
/// 160:    weight_start_slot (u64)
/// 168:    weight_end_slot (u64)
/// 176:    window_slots (u64)
/// 184:    current_batch_id (u64)
/// 192:    current_window_end (u64)
/// 200:    base_fee_bps (u16)
/// 202:    fee_discount_bps (u16)
/// 204:    authority (32)
/// 236:    bump (u8)
/// 237:    reentrancy_guard (u8)
/// 238:    paused (u8)
/// 239:    _padding (u8)
#[allow(clippy::too_many_arguments)]
pub fn build_pfda_pool_state(
    token_a_mint: &Address,
    token_b_mint: &Address,
    vault_a: &Address,
    vault_b: &Address,
    reserve_a: u64,
    reserve_b: u64,
    weight_a: u32,
    window_slots: u64,
    current_batch_id: u64,
    current_window_end: u64,
    base_fee_bps: u16,
    authority: &Address,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 240];
    d[0..8].copy_from_slice(b"poolstat");
    d[8..40].copy_from_slice(token_a_mint.as_ref());
    d[40..72].copy_from_slice(token_b_mint.as_ref());
    d[72..104].copy_from_slice(vault_a.as_ref());
    d[104..136].copy_from_slice(vault_b.as_ref());
    d[136..144].copy_from_slice(&reserve_a.to_le_bytes());
    d[144..152].copy_from_slice(&reserve_b.to_le_bytes());
    d[152..156].copy_from_slice(&weight_a.to_le_bytes());
    d[156..160].copy_from_slice(&weight_a.to_le_bytes()); // target == current
    // weight_start_slot + weight_end_slot left zero
    d[176..184].copy_from_slice(&window_slots.to_le_bytes());
    d[184..192].copy_from_slice(&current_batch_id.to_le_bytes());
    d[192..200].copy_from_slice(&current_window_end.to_le_bytes());
    d[200..202].copy_from_slice(&base_fee_bps.to_le_bytes());
    // fee_discount_bps left zero
    d[204..236].copy_from_slice(authority.as_ref());
    d[236] = bump;
    // reentrancy_guard / paused / pad zero
    d
}

/// Build BatchQueue raw bytes for pfda-amm (80 bytes).
pub fn build_batch_queue(
    pool: &Address,
    batch_id: u64,
    total_in_a: u64,
    total_in_b: u64,
    window_end_slot: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 80];
    d[0..8].copy_from_slice(b"batchque");
    d[8..40].copy_from_slice(pool.as_ref());
    d[40..48].copy_from_slice(&batch_id.to_le_bytes());
    d[48..56].copy_from_slice(&total_in_a.to_le_bytes());
    d[56..64].copy_from_slice(&total_in_b.to_le_bytes());
    d[64..72].copy_from_slice(&window_end_slot.to_le_bytes());
    d[72] = bump;
    d
}

/// Build ClearedBatchHistory raw bytes for pfda-amm (80 bytes).
pub fn build_cleared_batch_history(
    pool: &Address,
    batch_id: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 80];
    d[0..8].copy_from_slice(b"clrdhist");
    d[8..40].copy_from_slice(pool.as_ref());
    d[40..48].copy_from_slice(&batch_id.to_le_bytes());
    // clearing_price / out_b_per_in_a / out_a_per_in_b left zero —
    // CloseBatchHistory doesn't read them.
    d[72] = 1; // is_cleared
    d[73] = bump;
    d
}

/// Build UserOrderTicket raw bytes for pfda-amm (112 bytes).
pub fn build_user_order_ticket(
    owner: &Address,
    pool: &Address,
    batch_id: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 112];
    d[0..8].copy_from_slice(b"usrorder");
    d[8..40].copy_from_slice(owner.as_ref());
    d[40..72].copy_from_slice(pool.as_ref());
    d[72..80].copy_from_slice(&batch_id.to_le_bytes());
    // amount_in_a / amount_in_b / min_amount_out left zero
    d[104] = 0; // is_claimed = false
    d[105] = bump;
    d
}

/// Build BatchQueue3 raw bytes (88 bytes).
pub fn build_batch_queue_3(
    pool: &Address,
    batch_id: u64,
    total_in: &[u64; 3],
    window_end_slot: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 88];
    let mut off = 0;
    d[off..off + 8].copy_from_slice(b"batch3q\0");
    off += 8;
    d[off..off + 32].copy_from_slice(pool.as_ref());
    off += 32;
    d[off..off + 8].copy_from_slice(&batch_id.to_le_bytes());
    off += 8;
    for i in 0..3 {
        d[off..off + 8].copy_from_slice(&total_in[i].to_le_bytes());
        off += 8;
    }
    d[off..off + 8].copy_from_slice(&window_end_slot.to_le_bytes());
    off += 8;
    d[off] = bump;
    d
}

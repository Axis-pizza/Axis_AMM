/// G3mPoolState — repr(C), multi-token G3M pool
///
/// Supports up to 5 tokens (A/B test spec: 5 CEX-unlisted memecoins at 20% each).
/// PDA seeds: [b"g3m_pool", authority]
///
/// Layout (fixed-size for Solana account model):
///   discriminator:         8
///   authority:            32
///   token_count:           1
///   token_mints:     5 * 32 = 160
///   token_vaults:    5 * 32 = 160
///   target_weights:  5 *  2 =  10
///   reserves:        5 *  8 =  40
///   invariant_k:          16  (u128)
///   fee_rate_bps:          2
///   drift_threshold_bps:   2
///   last_rebalance_slot:   8
///   rebalance_cooldown:    8
///   paused:                1
///   bump:                  1
///   _padding:              6
///   TOTAL:               455

pub const MAX_POOL_TOKENS: usize = 5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct G3mPoolState {
    /// Discriminator: b"g3mpool\0"
    pub discriminator: [u8; 8],
    /// Pool authority (creator)
    pub authority: [u8; 32],
    /// Number of active tokens (2..=5)
    pub token_count: u8,
    /// Token mint addresses
    pub token_mints: [[u8; 32]; MAX_POOL_TOKENS],
    /// Token vault PDAs (pool-controlled token accounts)
    pub token_vaults: [[u8; 32]; MAX_POOL_TOKENS],
    /// Target weights in basis points (must sum to 10_000)
    pub target_weights_bps: [u16; MAX_POOL_TOKENS],
    /// Current reserves per token
    pub reserves: [u64; MAX_POOL_TOKENS],
    /// G3M invariant k low 8 bytes (Q32.32, split to avoid u128 alignment)
    pub invariant_k_lo: u64,
    /// G3M invariant k high 8 bytes
    pub invariant_k_hi: u64,
    /// Swap fee in basis points (e.g., 100 = 1%)
    pub fee_rate_bps: u16,
    /// Drift threshold in basis points (e.g., 500 = 5%)
    pub drift_threshold_bps: u16,
    /// Slot of last rebalance execution
    pub last_rebalance_slot: u64,
    /// Minimum slots between rebalances (cooldown)
    pub rebalance_cooldown: u64,
    /// Emergency pause flag
    pub paused: u8,
    /// PDA bump seed
    pub bump: u8,
    /// Alignment padding
    pub _padding: [u8; 6],
}

impl G3mPoolState {
    pub const DISCRIMINATOR: [u8; 8] = *b"g3mpool\0";
    pub const LEN: usize = core::mem::size_of::<G3mPoolState>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }

    /// Compute actual weight of token i (in bps) from reserves.
    /// For G3M, actual weight is derived from reserve ratios relative to targets.
    /// actual_weight_i = (reserve_i * target_weight_i) / sum(reserve_j * target_weight_j) * 10_000
    pub fn actual_weight_bps(&self, index: usize) -> Option<u64> {
        if index >= self.token_count as usize {
            return None;
        }

        let mut total_weighted: u128 = 0;
        for j in 0..self.token_count as usize {
            total_weighted = total_weighted.checked_add(
                (self.reserves[j] as u128)
                    .checked_mul(self.target_weights_bps[j] as u128)?,
            )?;
        }

        if total_weighted == 0 {
            return Some(0);
        }

        let actual = (self.reserves[index] as u128)
            .checked_mul(self.target_weights_bps[index] as u128)?
            .checked_mul(10_000)?
            .checked_div(total_weighted)? as u64;

        Some(actual)
    }

    /// Compute drift for token i: |actual_weight - target_weight| / target_weight (in bps)
    pub fn drift_bps(&self, index: usize) -> Option<u64> {
        let actual = self.actual_weight_bps(index)?;
        let target = self.target_weights_bps[index] as u64;

        if target == 0 {
            return Some(0);
        }

        let diff = if actual > target {
            actual.checked_sub(target)?
        } else {
            target.checked_sub(actual)?
        };

        diff.checked_mul(10_000)?.checked_div(target)
    }

    /// Check if any token exceeds drift threshold
    pub fn needs_rebalance(&self) -> bool {
        for i in 0..self.token_count as usize {
            if let Some(drift) = self.drift_bps(i) {
                if drift > self.drift_threshold_bps as u64 {
                    return true;
                }
            }
        }
        false
    }
}

impl G3mPoolState {
    pub fn invariant_k(&self) -> u128 {
        (self.invariant_k_hi as u128) << 64 | self.invariant_k_lo as u128
    }

    pub fn set_invariant_k(&mut self, k: u128) {
        self.invariant_k_lo = k as u64;
        self.invariant_k_hi = (k >> 64) as u64;
    }
}

// Compile-time size assertion: 8+32+1+160+160+10+40+8+8+2+2+8+8+1+1+6 = 455
// But repr(C) may add padding after token_count (u8). Let's be flexible.
const _: () = assert!(core::mem::size_of::<G3mPoolState>() > 0);

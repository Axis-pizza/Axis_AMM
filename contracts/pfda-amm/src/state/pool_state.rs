/// PoolState - 244 bytes, repr(C)
///
/// PDA seeds: [b"pool", token_a_mint, token_b_mint]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PoolState {
    /// Discriminator: b"poolstat"
    pub discriminator: [u8; 8],
    /// Token A mint address
    pub token_a_mint: [u8; 32],
    /// Token B mint address
    pub token_b_mint: [u8; 32],
    /// Pool-controlled token A vault
    pub vault_a: [u8; 32],
    /// Pool-controlled token B vault
    pub vault_b: [u8; 32],
    /// Current reserve of token A
    pub reserve_a: u64,
    /// Current reserve of token B
    pub reserve_b: u64,
    /// Current weight of token A in micro-units (divide by 1_000_000 for fraction)
    pub current_weight_a: u32,
    /// Target weight of token A for TFMM weight transition
    pub target_weight_a: u32,
    /// Slot at which weight transition begins
    pub weight_start_slot: u64,
    /// Slot at which weight transition ends
    pub weight_end_slot: u64,
    /// Number of slots per batch window
    pub window_slots: u64,
    /// Current batch ID being accumulated
    pub current_batch_id: u64,
    /// Slot at which the current batch window ends
    pub current_window_end: u64,
    /// Base fee in basis points
    pub base_fee_bps: u16,
    /// Fee discount for searchers in basis points
    pub fee_discount_bps: u16,
    /// Pool authority (creator, can pause/update weights)
    pub authority: [u8; 32],
    /// PDA bump seed
    pub bump: u8,
    /// Reentrancy guard: 0 = open, 1 = locked
    pub reentrancy_guard: u8,
    /// Paused flag (1 = paused, 0 = active)
    pub paused: u8,
    /// Alignment padding
    pub _padding: [u8; 1],
}

impl PoolState {
    pub const DISCRIMINATOR: [u8; 8] = *b"poolstat";
    pub const LEN: usize = core::mem::size_of::<PoolState>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }

    /// Interpolate current weight_a based on current slot.
    /// Returns weight_a in micro-units (0..=1_000_000).
    pub fn interpolated_weight_a(&self, current_slot: u64) -> u32 {
        if current_slot >= self.weight_end_slot {
            return self.target_weight_a;
        }
        if current_slot <= self.weight_start_slot {
            return self.current_weight_a;
        }
        let elapsed = current_slot - self.weight_start_slot;
        let total = self.weight_end_slot - self.weight_start_slot;
        let delta = if self.target_weight_a >= self.current_weight_a {
            let d = (self.target_weight_a - self.current_weight_a) as u128;
            ((d * elapsed as u128) / total as u128) as u32
        } else {
            let d = (self.current_weight_a - self.target_weight_a) as u128;
            let sub = ((d * elapsed as u128) / total as u128) as u32;
            return self.current_weight_a.saturating_sub(sub);
        };
        self.current_weight_a + delta
    }
}

// Compile-time size assertion (actual layout = 240 bytes)
const _: () = assert!(core::mem::size_of::<PoolState>() == 240);

#[cfg(test)]
mod tests {
    use super::*;

    fn pool_with_weight_transition(
        current_weight_a: u32,
        target_weight_a: u32,
        start_slot: u64,
        end_slot: u64,
    ) -> PoolState {
        let mut ps = unsafe { core::mem::zeroed::<PoolState>() };
        ps.discriminator = PoolState::DISCRIMINATOR;
        ps.current_weight_a = current_weight_a;
        ps.target_weight_a = target_weight_a;
        ps.weight_start_slot = start_slot;
        ps.weight_end_slot = end_slot;
        ps
    }

    #[test]
    fn interpolated_weight_large_delta_no_overflow() {
        let start = 0u64;
        let end = start + u32::MAX as u64 + 1;
        let current = 1_000;
        let target = 1_000_000;
        let elapsed_slots = u32::MAX as u64;

        let ps = pool_with_weight_transition(current, target, start, end);
        let result = ps.interpolated_weight_a(start + elapsed_slots);

        let delta = (target - current) as u128;
        let expected = current + ((delta * elapsed_slots as u128) / (end - start) as u128) as u32;
        assert_eq!(result, expected);
    }

    #[test]
    fn interpolated_weight_at_boundaries() {
        let ps = pool_with_weight_transition(200_000, 800_000, 100, 200);
        assert_eq!(ps.interpolated_weight_a(50), 200_000);
        assert_eq!(ps.interpolated_weight_a(100), 200_000);
        assert_eq!(ps.interpolated_weight_a(200), 800_000);
        assert_eq!(ps.interpolated_weight_a(999), 800_000);
        assert_eq!(ps.interpolated_weight_a(150), 500_000);
    }

    #[test]
    fn interpolated_weight_decreasing() {
        let ps = pool_with_weight_transition(800_000, 200_000, 0, 100);
        assert_eq!(ps.interpolated_weight_a(0), 800_000);
        assert_eq!(ps.interpolated_weight_a(50), 500_000);
        assert_eq!(ps.interpolated_weight_a(100), 200_000);
    }
}

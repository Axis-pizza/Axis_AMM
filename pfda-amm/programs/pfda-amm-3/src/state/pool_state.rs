/// PoolState3 — 3-token PFDA pool for ETF A (SOL/BONK/WIF)
///
/// PDA seeds: [b"pool3", token_mints[0], token_mints[1], token_mints[2]]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PoolState3 {
    pub discriminator: [u8; 8],
    /// 3 token mint addresses
    pub token_mints: [[u8; 32]; 3],
    /// 3 vault addresses (pool-PDA-owned token accounts)
    pub vaults: [[u8; 32]; 3],
    /// Current reserves
    pub reserves: [u64; 3],
    /// Target weights in micro-units (sum to 1_000_000). 333_333 each for equal weight.
    pub weights: [u32; 3],
    /// Batch window length in slots
    pub window_slots: u64,
    /// Current batch ID
    pub current_batch_id: u64,
    /// Current batch window end slot
    pub current_window_end: u64,
    /// Protocol treasury (receives auction bids + protocol fee share)
    pub treasury: [u8; 32],
    /// Pool authority (creator, can pause/update)
    pub authority: [u8; 32],
    /// Base fee in basis points
    pub base_fee_bps: u16,
    /// PDA bump
    pub bump: u8,
    /// Reentrancy guard
    pub reentrancy_guard: u8,
    /// Paused flag (1 = paused, 0 = active)
    pub paused: u8,
    /// Padding
    pub _padding: [u8; 3],
}

impl PoolState3 {
    pub const DISCRIMINATOR: [u8; 8] = *b"pool3st\0";
    pub const LEN: usize = core::mem::size_of::<PoolState3>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

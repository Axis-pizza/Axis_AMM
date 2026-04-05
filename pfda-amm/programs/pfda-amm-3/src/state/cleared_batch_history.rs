/// ClearedBatchHistory3 — 3-token clearing record
///
/// PDA seeds: [b"history3", pool_key, batch_id.to_le_bytes()]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClearedBatchHistory3 {
    pub discriminator: [u8; 8],
    pub pool: [u8; 32],
    pub batch_id: u64,
    /// Clearing prices: price[i] = value of token i in numeraire (Q32.32)
    pub clearing_prices: [u64; 3],
    /// Total output per token after clearing
    pub total_out: [u64; 3],
    /// Total input per token
    pub total_in: [u64; 3],
    /// Fee rate in bps (copied from pool at clearing time)
    pub fee_bps: u16,
    pub is_cleared: bool,
    pub bump: u8,
    pub _padding: [u8; 4],
}

impl ClearedBatchHistory3 {
    pub const DISCRIMINATOR: [u8; 8] = *b"clrd3h\0\0";
    pub const LEN: usize = core::mem::size_of::<ClearedBatchHistory3>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

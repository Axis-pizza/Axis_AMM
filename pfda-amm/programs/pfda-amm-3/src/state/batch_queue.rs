/// BatchQueue3 — 3-token batch queue. O(1) aggregation.
///
/// PDA seeds: [b"queue3", pool_key, batch_id.to_le_bytes()]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BatchQueue3 {
    pub discriminator: [u8; 8],
    pub pool: [u8; 32],
    pub batch_id: u64,
    /// Accumulated input per token (scalar aggregates, O(1))
    pub total_in: [u64; 3],
    pub window_end_slot: u64,
    pub bump: u8,
    pub _padding: [u8; 7],
}

impl BatchQueue3 {
    pub const DISCRIMINATOR: [u8; 8] = *b"batch3q\0";
    pub const LEN: usize = core::mem::size_of::<BatchQueue3>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

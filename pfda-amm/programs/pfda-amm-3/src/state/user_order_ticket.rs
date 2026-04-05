/// UserOrderTicket3 — 3-token order intent
///
/// PDA seeds: [b"ticket3", pool_key, user_key, batch_id.to_le_bytes()]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UserOrderTicket3 {
    pub discriminator: [u8; 8],
    pub owner: [u8; 32],
    pub pool: [u8; 32],
    pub batch_id: u64,
    /// Amount of each token deposited (only one should be > 0)
    pub amounts_in: [u64; 3],
    /// Index of the desired output token
    pub out_token_idx: u8,
    /// Minimum output amount (slippage protection)
    pub min_amount_out: u64,
    pub is_claimed: bool,
    pub bump: u8,
    pub _padding: [u8; 5],
}

impl UserOrderTicket3 {
    pub const DISCRIMINATOR: [u8; 8] = *b"usrord3\0";
    pub const LEN: usize = core::mem::size_of::<UserOrderTicket3>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

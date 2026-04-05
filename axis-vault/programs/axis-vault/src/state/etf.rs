/// EtfState — on-chain ETF vault.
///
/// Manages a basket of up to 5 SPL tokens with target weights.
/// Users deposit basket tokens → receive ETF mint tokens.
/// Users burn ETF tokens → receive proportional basket tokens back.
///
/// PDA seeds: [b"etf", authority, name_bytes]
///
/// The ETF mint is a separate SPL token mint where the EtfState PDA is the mint authority.
pub const MAX_BASKET_TOKENS: usize = 5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EtfState {
    pub discriminator: [u8; 8],
    /// Authority (creator) who can update weights or pause
    pub authority: [u8; 32],
    /// ETF token mint address (SPL mint where this PDA is authority)
    pub etf_mint: [u8; 32],
    /// Number of tokens in the basket
    pub token_count: u8,
    /// Token mint addresses in the basket
    pub token_mints: [[u8; 32]; MAX_BASKET_TOKENS],
    /// Token vault addresses (PDA-owned)
    pub token_vaults: [[u8; 32]; MAX_BASKET_TOKENS],
    /// Target weights in basis points (sum to 10_000)
    pub weights_bps: [u16; MAX_BASKET_TOKENS],
    /// Total ETF token supply (tracked for NAV calculation)
    pub total_supply: u64,
    /// Treasury (receives protocol fees)
    pub treasury: [u8; 32],
    /// Paused flag
    pub paused: u8,
    /// PDA bump
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 6],
}

impl EtfState {
    pub const DISCRIMINATOR: [u8; 8] = *b"etfstate";
    pub const LEN: usize = core::mem::size_of::<EtfState>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

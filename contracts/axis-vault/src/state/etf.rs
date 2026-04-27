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

/// Max on-chain display name for the ETF (UTF-8, zero-padded).
pub const MAX_ETF_NAME_LEN: usize = 32;

/// Max on-chain ticker (ASCII uppercase letters/digits, zero-padded).
pub const MAX_ETF_TICKER_LEN: usize = 16;

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
    /// Protocol fee on deposit/withdraw in basis points (e.g. 30 = 0.3%)
    pub fee_bps: u16,
    /// Paused flag
    pub paused: u8,
    /// PDA bump
    pub bump: u8,
    /// Human-readable display name (UTF-8, zero-padded to MAX_ETF_NAME_LEN).
    /// Same bytes used as the `name` PDA seed — stored here so clients
    /// (explorers, third-party UIs) can render the ETF without depending
    /// on any off-chain database.
    pub name: [u8; MAX_ETF_NAME_LEN],
    /// Short ticker symbol (ASCII uppercase + digits, zero-padded).
    /// Convention matches traditional finance (e.g. "AXBTC"). Wallets
    /// can use this for compact display alongside the ETF mint.
    pub ticker: [u8; MAX_ETF_TICKER_LEN],
    /// Slot at which `CreateEtf` ran. Captured from `Clock::get()?.slot`.
    /// Gives clients a tamper-proof creation timestamp for provenance
    /// without relying on explorer indexes.
    pub created_at_slot: u64,
    /// Maximum allowed `fee_bps`. SetFee rejects any new fee above this
    /// ceiling. Hard-set at CreateEtf time from `MAX_FEE_BPS_CEILING`
    /// in constants.rs. Locks the worst-case fee an authority can set,
    /// even after a compromised authority key. Cannot be raised.
    pub max_fee_bps: u16,
    /// Two-byte alignment pad before tvl_cap. Reserved for future
    /// flag bytes (e.g. compliance flag, kyc-required bit).
    pub _pad: [u8; 2],
    /// TVL cap: maximum `total_supply` allowed. Deposits that would
    /// push total_supply above this revert with `TvlCapExceeded`.
    /// Zero = no cap (uncapped). Can only be raised by the authority,
    /// never lowered (lowering would brick deposits for any pool
    /// currently above the lower cap, with no in-protocol path to
    /// drain). Initialized to zero (uncapped) by CreateEtf; the
    /// authority sets it via SetCap once they've decided the closed-
    /// beta ramp curve.
    pub tvl_cap: u64,
    /// Tail padding. Future metadata additions (e.g. delegate keys,
    /// migration markers) slot in here without another discriminator
    /// bump as long as alignment is respected.
    pub _padding: [u8; 4],
}

impl EtfState {
    /// Discriminator bumped from `etfstat2` → `etfstat3` as part of the
    /// pre-mainnet hardening: SetFee with `max_fee_bps` ceiling and
    /// SetCap with `tvl_cap`. Old v2 accounts fail `is_initialized()`
    /// and must be closed/re-created rather than migrated in place —
    /// devnet pools created before this bump are pre-launch and
    /// expected to be discarded.
    pub const DISCRIMINATOR: [u8; 8] = *b"etfstat3";
    pub const LEN: usize = core::mem::size_of::<EtfState>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

//! RebalanceState — sidecar PDA carrying rebalance-window accounting and
//! the pending weight proposal for one ETF.
//!
//! Why a sidecar and not new EtfState fields: EtfState accounts are
//! allocated at exactly `EtfState::LEN` by CreateEtf and the program has
//! no realloc path, so any field appended to EtfState would make every
//! pre-upgrade account fail the `load::<EtfState>` size check and brick
//! live ETFs. A lazily-created sidecar keyed off the etf_state address
//! adds state without touching the v1 layout: existing ETFs keep working
//! untouched and pay rent for rebalance state only when their authority
//! first uses Rebalance or ProposeWeights.
//!
//! PDA seeds: [b"rebal", etf_state_pubkey]

use crate::state::etf::MAX_BASKET_TOKENS;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RebalanceState {
    pub discriminator: [u8; 8],
    /// The EtfState PDA this sidecar belongs to. Checked on every load
    /// so a sidecar created for one ETF can never be replayed against
    /// another (the PDA derivation already binds it, but the stored
    /// back-pointer makes the check independent of seed recomputation).
    pub etf_state: [u8; 32],
    /// PDA bump for [b"rebal", etf_state].
    pub bump: u8,
    /// Alignment pad up to the first u64 field.
    pub _pad: [u8; 7],
    /// Slot at which the current turnover window opened. Zero means
    /// "no window yet" — the next Rebalance always opens a fresh one.
    pub window_start_slot: u64,
    /// Per-vault balances snapshotted when the window opened. The
    /// turnover cap is computed against these, not live balances, so
    /// intra-window deposits can't be used to inflate the sell budget.
    pub window_snapshot: [u64; MAX_BASKET_TOKENS],
    /// Per-vault amounts actually consumed by Rebalance legs inside the
    /// current window.
    pub window_sold: [u64; MAX_BASKET_TOKENS],
    /// Pending weight proposal (zero-padded past token_count).
    pub proposed_weights: [u16; MAX_BASKET_TOKENS],
    /// Alignment pad before proposal_eta_slot.
    pub _pad2: [u8; 6],
    /// Slot at which the pending proposal becomes applicable. Zero
    /// means no pending proposal.
    pub proposal_eta_slot: u64,
    /// Reserved for future fields (e.g. per-ETF turnover overrides).
    pub _reserved: [u8; 32],
}

impl RebalanceState {
    pub const DISCRIMINATOR: [u8; 8] = *b"rebal__1";
    pub const LEN: usize = core::mem::size_of::<RebalanceState>();

    pub fn is_initialized(&self) -> bool {
        self.discriminator == Self::DISCRIMINATOR
    }
}

// Lock the wire layout. The TS decoder (frontend `decodeRebalanceState`)
// and the LiteSVM test reader both hardcode these byte offsets; a field
// or padding edit that shifts them would silently corrupt every external
// read. Any such edit must update those offsets and these asserts in
// lockstep.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(RebalanceState, discriminator) == 0);
    assert!(offset_of!(RebalanceState, etf_state) == 8);
    assert!(offset_of!(RebalanceState, bump) == 40);
    assert!(offset_of!(RebalanceState, window_start_slot) == 48);
    assert!(offset_of!(RebalanceState, window_snapshot) == 56);
    assert!(offset_of!(RebalanceState, window_sold) == 96);
    assert!(offset_of!(RebalanceState, proposed_weights) == 136);
    assert!(offset_of!(RebalanceState, proposal_eta_slot) == 152);
    assert!(core::mem::size_of::<RebalanceState>() == 192);
};

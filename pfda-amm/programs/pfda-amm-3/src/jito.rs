/// Jito auction integration for 3-token PFDA batch clearing.
///
/// Ported from pfda-amm (2-token) jito.rs.
///
/// Flow:
///   1. Off-chain: Searchers observe batch windows ending
///   2. Off-chain: Searchers construct Jito bundles containing:
///      a) SOL transfer to protocol treasury (the "bid")
///      b) ClearBatch instruction (with bid_lamports parameter)
///      c) Jito tip transaction (for block engine priority)
///   3. Off-chain: Jito Block Engine selects highest tipper's bundle
///   4. On-chain: ClearBatch validates bid and transfers SOL to treasury

/// Revenue distribution from searcher bid.
///
///   protocol_share = α * bid (α defaults to 50%)
///   lp_share = (1-α) * bid
pub fn compute_bid_split(bid_lamports: u64, alpha_bps: u16) -> (u64, u64) {
    let protocol_share = (bid_lamports as u128)
        .saturating_mul(alpha_bps as u128)
        / 10_000;
    let lp_share = bid_lamports.saturating_sub(protocol_share as u64);
    (protocol_share as u64, lp_share)
}

/// Minimum bid in lamports (anti-spam). 0.001 SOL = 1_000_000 lamports.
pub const MIN_BID_LAMPORTS: u64 = 1_000_000;

/// Default alpha (protocol share) in basis points. 5000 = 50%.
pub const DEFAULT_ALPHA_BPS: u16 = 5000;

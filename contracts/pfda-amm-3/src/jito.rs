/// Jito auction integration for 3-token PFDA batch clearing.
///
/// Flow:
///   1. Off-chain: Searchers observe batch windows ending
///   2. Off-chain: Searchers construct Jito bundles containing:
///      a) SOL transfer to protocol treasury (the "bid")
///      b) ClearBatch instruction (with bid_lamports parameter)
///      c) Jito tip transaction (for block engine priority)
///   3. Off-chain: Jito Block Engine selects highest tipper's bundle
///   4. On-chain: ClearBatch validates bid and transfers SOL to treasury
///
/// Security improvements over v1:
///   - Bid-to-volume ratio validation (bid must be reasonable relative to batch value)
///   - Jito tip account validation (known mainnet tip accounts)
///   - Revenue split tracked on-chain via return_data

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

use crate::error::Pfda3Error;

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

/// Maximum bid-to-volume ratio in bps (10% of batch volume).
/// Prevents unreasonably large bids that could indicate manipulation.
const MAX_BID_VOLUME_RATIO_BPS: u64 = 1000;

/// Known Jito tip accounts (mainnet). Used for optional validation when
/// the tip account is provided. These are the 8 canonical Jito tip accounts.
pub const JITO_TIP_ACCOUNTS: [[u8; 32]; 8] = [
    // 96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5
    [0x7a, 0x63, 0x3f, 0xe2, 0xb3, 0x97, 0x17, 0x2f,
     0xde, 0x71, 0x63, 0x31, 0x22, 0xf9, 0x5e, 0x24,
     0x9d, 0x6f, 0x7a, 0x64, 0x4e, 0x3c, 0x91, 0x8b,
     0x64, 0xf2, 0xa1, 0xb5, 0xe7, 0x9c, 0x51, 0x08],
    // HFqU5x63VTqvQss8hp11i4bPUBi8XE5NyN3pUPm47GhA
    [0xf0, 0x66, 0x02, 0x5b, 0x66, 0x0f, 0x39, 0x08,
     0x18, 0x37, 0x69, 0x05, 0xe4, 0x67, 0x81, 0x7e,
     0x69, 0x3d, 0x8e, 0x8f, 0x89, 0x69, 0x26, 0x54,
     0x1a, 0x12, 0x5e, 0x59, 0x47, 0x0c, 0x39, 0x73],
    // Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY
    [0xad, 0xac, 0x37, 0x65, 0x91, 0xb8, 0x85, 0x6e,
     0x8e, 0x16, 0x0e, 0x75, 0x0e, 0x75, 0xf7, 0x6a,
     0x74, 0x52, 0x2d, 0x48, 0xce, 0xe3, 0xe3, 0x52,
     0x5c, 0xf6, 0xfc, 0xc8, 0x0e, 0x83, 0xb5, 0x05],
    // ADaUMid9yfUC67HFrMR3DMpcP7Y6suUAZndaLxB1dqb1
    [0x00, 0x8c, 0x97, 0x5c, 0x3e, 0x3c, 0x3f, 0x88,
     0x0b, 0x15, 0x7a, 0x06, 0xd4, 0x7c, 0x3f, 0x46,
     0x54, 0x43, 0x41, 0x1c, 0xae, 0xf0, 0x44, 0x4e,
     0xf5, 0x95, 0xad, 0x75, 0xf3, 0x74, 0x3a, 0xd9],
    // ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt
    [0x00, 0x8d, 0x39, 0x8b, 0x4f, 0x37, 0xb7, 0x76,
     0xd0, 0xce, 0x85, 0xe7, 0x15, 0x20, 0xcc, 0xdc,
     0x96, 0x3e, 0x15, 0x35, 0x35, 0xe5, 0x04, 0x1b,
     0xca, 0xfe, 0x5d, 0xc7, 0xfe, 0x40, 0xce, 0x8e],
    // DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh
    [0xbe, 0x95, 0x92, 0x0f, 0x02, 0x32, 0xde, 0x98,
     0x84, 0x7d, 0x0f, 0x8f, 0x8a, 0x1c, 0x51, 0xd9,
     0xf2, 0x39, 0x33, 0x6c, 0x54, 0xf3, 0x52, 0x68,
     0x07, 0x4d, 0x31, 0xfa, 0xd2, 0x4c, 0x1b, 0x45],
    // 3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT
    [0x22, 0xb7, 0x9f, 0x83, 0xa2, 0xa3, 0x5e, 0xa6,
     0x17, 0x72, 0x3e, 0x2f, 0x48, 0xce, 0xde, 0xb5,
     0xa1, 0x21, 0x82, 0xdd, 0x37, 0xd4, 0x26, 0xb2,
     0x0f, 0x4c, 0x8d, 0x67, 0x7d, 0x53, 0x24, 0x1a],
    // DttWaMuVvTiDuNEhpcRyfMeHtBgsXQGqs6uo3LWEtxXp
    [0xbf, 0xf9, 0xb0, 0x9e, 0xa5, 0x57, 0x5f, 0xea,
     0x62, 0x69, 0xb6, 0x44, 0x85, 0x2d, 0xa8, 0x60,
     0xb5, 0x7b, 0x4e, 0x89, 0x93, 0x62, 0x85, 0x44,
     0x62, 0x63, 0xfb, 0xba, 0x69, 0xc4, 0x05, 0x29],
];

/// Validate that a bid is reasonable relative to the batch volume.
///
/// A bid that exceeds MAX_BID_VOLUME_RATIO_BPS of the batch value is rejected.
/// This prevents manipulation where an attacker places an absurd bid to force
/// a specific clearing outcome, and ensures the searcher's cost is proportional
/// to the value they can extract.
///
/// `total_value_in` is the total batch deposit value in numeraire units.
/// Returns Ok(()) if the bid is reasonable, or Err if excessive.
pub fn validate_bid_against_volume(
    bid_lamports: u64,
    total_value_in: u64,
) -> Result<(), ProgramError> {
    if total_value_in == 0 {
        // No volume — any non-zero bid is fine (just clears an empty batch)
        return Ok(());
    }

    // bid / total_value_in * 10_000 <= MAX_BID_VOLUME_RATIO_BPS
    let ratio_bps = (bid_lamports as u128)
        .saturating_mul(10_000)
        / (total_value_in as u128).max(1);

    if ratio_bps > MAX_BID_VOLUME_RATIO_BPS as u128 {
        return Err(Pfda3Error::BidExcessive.into());
    }

    Ok(())
}

/// Verify an account is one of the known Jito tip accounts.
/// Returns true if the account key matches any of the 8 canonical tip addresses.
pub fn is_known_jito_tip_account(account: &AccountInfo) -> bool {
    let key_bytes = account.key().as_ref();
    for tip_account in JITO_TIP_ACCOUNTS.iter() {
        if key_bytes == tip_account {
            return true;
        }
    }
    false
}

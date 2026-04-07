/// Switchboard on-demand oracle price feed reader for 3-token PFDA.
///
/// Ported from pfda-amm (2-token) oracle.rs. Reads raw bytes from Switchboard
/// PullFeedAccountData accounts without importing the Switchboard crate.
///
/// Account layout (from switchboard-on-demand source):
///   - result.mean: i128 at offset 1272 (scaled by 10^18)
///   - result.num_samples: u8 at offset 1336
///   - result.slot: u64 at offset 1344

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

use crate::error::Pfda3Error;

/// Offset of CurrentResult.value (i128) within PullFeedAccountData
const RESULT_VALUE_OFFSET: usize = 1272;

/// Offset of CurrentResult.slot (u64)
const RESULT_SLOT_OFFSET: usize = 1344;

/// Offset of CurrentResult.num_samples (u8)
const RESULT_NUM_SAMPLES_OFFSET: usize = 1336;

/// Minimum account size for a valid Switchboard feed
const MIN_FEED_ACCOUNT_SIZE: usize = 1360;

/// Switchboard prices are scaled by 10^18
const SWITCHBOARD_PRECISION: u128 = 1_000_000_000_000_000_000;

/// Switchboard On-Demand program ID: SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv
const SWITCHBOARD_ON_DEMAND_PID: [u8; 32] = [
    0x06, 0x73, 0xbd, 0x46, 0xf2, 0xe4, 0x7e, 0x04,
    0xf1, 0x2b, 0xd9, 0x2f, 0xb7, 0x31, 0x96, 0x8e,
    0xcd, 0x9d, 0x97, 0x57, 0xc2, 0x74, 0xda, 0x87,
    0x47, 0x6f, 0x46, 0x5c, 0x04, 0x0c, 0x65, 0x73,
];

/// Read a Switchboard on-demand price feed and return the price as Q32.32 fixed-point.
///
/// Validates:
///   - Account data is large enough
///   - Price is not stale (within max_stale_slots)
///   - At least min_samples oracle responses
///   - Price is positive
///
/// Returns: price as Q32.32 (u64), or error
pub fn read_switchboard_price(
    feed_account: &AccountInfo,
    current_slot: u64,
    max_stale_slots: u64,
    min_samples: u8,
) -> Result<u64, ProgramError> {
    // Verify the feed account is owned by the Switchboard On-Demand program
    if feed_account.owner() != &SWITCHBOARD_ON_DEMAND_PID {
        return Err(Pfda3Error::OracleOwnerMismatch.into());
    }

    let data = feed_account.try_borrow_data()?;

    if data.len() < MIN_FEED_ACCOUNT_SIZE {
        return Err(Pfda3Error::OracleInvalid.into());
    }

    let price_i128 = i128::from_le_bytes(
        data[RESULT_VALUE_OFFSET..RESULT_VALUE_OFFSET + 16]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );

    if price_i128 <= 0 {
        return Err(Pfda3Error::OraclePriceNegative.into());
    }

    let result_slot = u64::from_le_bytes(
        data[RESULT_SLOT_OFFSET..RESULT_SLOT_OFFSET + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );

    if current_slot > result_slot && (current_slot - result_slot) > max_stale_slots {
        return Err(Pfda3Error::OracleStale.into());
    }

    let num_samples = data[RESULT_NUM_SAMPLES_OFFSET];
    if num_samples < min_samples {
        return Err(Pfda3Error::OracleInsufficientSamples.into());
    }

    // Convert from Switchboard precision (10^18) to Q32.32
    let price_u128 = price_i128 as u128;
    let fp_price = price_u128
        .checked_mul(1u128 << 32)
        .and_then(|x| x.checked_div(SWITCHBOARD_PRECISION))
        .ok_or(ProgramError::from(Pfda3Error::Overflow))?;

    if fp_price > u64::MAX as u128 {
        return Err(Pfda3Error::Overflow.into());
    }

    Ok(fp_price as u64)
}

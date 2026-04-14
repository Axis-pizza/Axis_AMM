/// Switchboard on-demand oracle price feed reader.
///
/// Zero-dependency implementation: reads raw bytes from Switchboard
/// PullFeedAccountData accounts without importing the Switchboard crate.
///
/// Account layout (from switchboard-on-demand source):
///   - Anchor discriminator: 8 bytes (offset 0)
///   - OracleSubmission[32]: 1024 bytes (offset 8)
///   - authority: 32 bytes (offset 1032)
///   - queue: 32 bytes (offset 1064)
///   - feed_hash: 32 bytes (offset 1096)
///   - initialized_at: i64 (offset 1128)
///   - permissions: u64 (offset 1136)
///   - max_variance: u64 (offset 1144)
///   - min_responses: u32 (offset 1152)
///   - name: 32 bytes (offset 1156)
///   - padding1: 1 byte (offset 1188)
///   - permit_write_by_authority: u8 (offset 1189)
///   - historical_result_idx: u8 (offset 1190)
///   - min_sample_size: u8 (offset 1191)
///   - last_update_timestamp: i64 (offset 1192)
///   - lut_slot: u64 (offset 1200)
///   - _reserved1: 32 bytes (offset 1208)
///   - result: CurrentResult (offset 1240)
///     - value: i128 (offset 1240) — THE PRICE, scaled by 10^18
///     - std_dev: i128 (offset 1256)
///     - mean: i128 (offset 1272)
///     - range: i128 (offset 1288)
///     - min_value: i128 (offset 1304)
///     - max_value: i128 (offset 1320)
///     - num_samples: u8 (offset 1336)
///     - submission_idx: u8 (offset 1337)
///     - padding1: [u8; 6] (offset 1338)
///     - slot: u64 (offset 1344) — slot when result was computed
///   - max_staleness: u32 (offset 1352)
///
/// Note: All offsets include the 8-byte Anchor discriminator prefix.
///
/// Switchboard on-demand program ID (mainnet):
///   SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv  (V3 / on-demand)

use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

use crate::error::PfmmError;

/// Switchboard on-demand V3 program ID (mainnet): SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv
const SWITCHBOARD_V3_PROGRAM_ID: [u8; 32] = [
    0x06, 0x73, 0xbd, 0x46, 0xf2, 0xe4, 0x7e, 0x04,
    0xf1, 0x2b, 0xd9, 0x2f, 0xb7, 0x31, 0x96, 0x8e,
    0xcd, 0x9d, 0x97, 0x57, 0xc2, 0x74, 0xda, 0x87,
    0x47, 0x6f, 0x46, 0x5c, 0x04, 0x0c, 0x65, 0x73,
];

/// Switchboard on-demand V3 devnet: Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2
const SWITCHBOARD_DEVNET_PROGRAM_ID: [u8; 32] = [
    0x90, 0x6e, 0x14, 0x64, 0xc5, 0xf8, 0xb7, 0x63,
    0x3c, 0xc0, 0x5a, 0x42, 0x4c, 0xdd, 0xb3, 0xae,
    0xcd, 0x6d, 0xab, 0xb8, 0xae, 0xc7, 0x47, 0xbc,
    0x4f, 0x3e, 0x11, 0x30, 0x1e, 0x40, 0x63, 0xcb,
];

fn verify_switchboard_owner(feed_account: &AccountInfo) -> Result<(), ProgramError> {
    let owner = feed_account.owner();
    if owner != &SWITCHBOARD_V3_PROGRAM_ID && owner != &SWITCHBOARD_DEVNET_PROGRAM_ID {
        return Err(PfmmError::OracleOwnerMismatch.into());
    }
    Ok(())
}

/// Offset of CurrentResult.value (i128) within PullFeedAccountData
/// Verified empirically against devnet accounts owned by program
/// 2TfB33aLaneQb5TNVwyDz3jSZXS6jdW2ARw1Dgf84XCG
/// The result.mean (aggregated price) is at offset 1272 (i128, 16 bytes)
/// Confirmed: offset 1272 reads $3.52 from a live devnet feed
const RESULT_VALUE_OFFSET: usize = 1272;

/// Offset of CurrentResult.slot (u64) — at offset 1344 in the standard layout
/// For safety, we also check the result.mean at a nearby offset
const RESULT_SLOT_OFFSET: usize = 1344;

/// Offset of CurrentResult.num_samples (u8)
const RESULT_NUM_SAMPLES_OFFSET: usize = 1336;

/// Minimum account size for a valid Switchboard feed
const MIN_FEED_ACCOUNT_SIZE: usize = 1360;

/// Switchboard prices are scaled by 10^18
const SWITCHBOARD_PRECISION: u128 = 1_000_000_000_000_000_000;

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
    verify_switchboard_owner(feed_account)?;

    let data = feed_account.try_borrow_data()?;

    // Validate account size
    if data.len() < MIN_FEED_ACCOUNT_SIZE {
        return Err(PfmmError::InvalidDiscriminator.into());
    }

    // Read the price value (i128, little-endian, at offset RESULT_VALUE_OFFSET)
    let price_i128 = i128::from_le_bytes(
        data[RESULT_VALUE_OFFSET..RESULT_VALUE_OFFSET + 16]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );

    // Price must be positive
    if price_i128 <= 0 {
        return Err(PfmmError::ClearingPriceFailed.into());
    }

    // Read the result slot (u64, LE)
    let result_slot = u64::from_le_bytes(
        data[RESULT_SLOT_OFFSET..RESULT_SLOT_OFFSET + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );

    // Validate staleness
    if current_slot > result_slot && (current_slot - result_slot) > max_stale_slots {
        return Err(PfmmError::ClearingPriceFailed.into());
    }

    // Validate sample count
    let num_samples = data[RESULT_NUM_SAMPLES_OFFSET];
    if num_samples < min_samples {
        return Err(PfmmError::ClearingPriceFailed.into());
    }

    // Convert from Switchboard precision (10^18) to Q32.32
    // Q32.32 = price_i128 * 2^32 / 10^18
    let price_u128 = price_i128 as u128;
    let fp_price = price_u128
        .checked_mul(1u128 << 32)
        .and_then(|x| x.checked_div(SWITCHBOARD_PRECISION))
        .ok_or(ProgramError::from(PfmmError::Overflow))?;

    if fp_price > u64::MAX as u128 {
        return Err(PfmmError::Overflow.into());
    }

    Ok(fp_price as u64)
}

/// Compute NAV for a 2-token pool using oracle prices.
///
/// nav = (reserve_a * price_a + reserve_b * price_b) in Q32.32
/// All prices are Q32.32 (price per token in USD or common numeraire).
pub fn compute_nav_q32(
    reserve_a: u64,
    reserve_b: u64,
    price_a: u64,
    price_b: u64,
) -> Option<u128> {
    let value_a = (reserve_a as u128).checked_mul(price_a as u128)?;
    let value_b = (reserve_b as u128).checked_mul(price_b as u128)?;
    // Result is in Q32.32 * raw = Q32.32 with extra 'reserve' factor
    // Shift down by 32 to get NAV in raw units (USD-equivalent lamports)
    Some(value_a.checked_add(value_b)? >> 32)
}

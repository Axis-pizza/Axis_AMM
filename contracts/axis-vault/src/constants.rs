//! Shared constants for axis-vault.

/// SPL Token Program ID, byte-encoded so we can compare owners without a
/// string conversion. Kept here so Deposit and Withdraw reference one source.
pub const TOKEN_PROGRAM_ID: [u8; 32] = [
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93,
    0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91,
    0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
];

/// Maximum allowed divergence (in basis points) between the highest and
/// lowest per-vault mint candidates computed during Deposit. Larger gaps
/// imply the vault is out of ratio with the basket's target weights — the
/// deposit could over- or under-mint relative to any single token. We
/// reject early with `NavDeviationExceeded` rather than minting at a stale
/// composition. 300 bps = 3 %.
pub const MAX_NAV_DEVIATION_BPS: u64 = 300;

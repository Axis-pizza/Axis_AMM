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

/// Minimum base amount accepted on the first deposit into a fresh ETF.
/// Closes the cheap-attacker leg of the inflation / donation attack:
/// without this, an attacker could seed with `amount = 1`, then donate
/// huge quantities of basket tokens directly into the vault ATAs to
/// push every proportional-mint candidate to zero for the next
/// legitimate depositor (they would revert on `ZeroDeposit`, bricking
/// the pool). 1_000_000 = 1 token at 6 decimals.
pub const MIN_FIRST_DEPOSIT: u64 = 1_000_000;

/// Virtual liquidity lock added to `etf.total_supply` on the first
/// deposit but never minted to any holder. Combined with
/// `MIN_FIRST_DEPOSIT` this keeps `vault_balance / total_supply`
/// bounded below for the life of the ETF so that vault donations can
/// never round proportional math to zero. Mirrors Uniswap V2's
/// `MINIMUM_LIQUIDITY = 1_000`. Because nobody holds these tokens,
/// they can never be withdrawn — a tiny amount of each basket token
/// is permanently stranded in the vaults, which is the intended cost.
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Protocol treasury multisig address — the single destination for
/// protocol fee revenue once ops (#38) finalizes the Squads V4 setup.
///
/// This is a placeholder until @muse0509 confirms the signer list and
/// the multisig is deployed on devnet → mainnet. `SweepTreasury`
/// already works against whatever pubkey the ETF was created with (see
/// `EtfState.treasury`); the CreateEtf gate — reject
/// `treasury != PROTOCOL_TREASURY` — stays deferred until this
/// constant points at a real multisig so tests can still spin up
/// ad-hoc treasuries during the transition.
///
/// TODO(ops #38): replace zeros with the deployed Squads vault key.
pub const PROTOCOL_TREASURY: [u8; 32] = [0u8; 32];

pub fn protocol_treasury_is_active() -> bool {
    PROTOCOL_TREASURY.iter().any(|&b| b != 0)
}

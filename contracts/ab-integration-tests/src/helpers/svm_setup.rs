use litesvm::LiteSVM;
use solana_address::Address;
use solana_clock::Clock;

pub const AXIS_G3M_ID: &str = "65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi";
pub const PFDA_AMM_3_ID: &str = "DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf";
pub const PFDA_AMM_ID: &str = "CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk";
pub const AXIS_VAULT_ID: &str = "DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy";
pub const JUPITER_V6_ID: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const MPL_TOKEN_METADATA_ID: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

pub const AXIS_G3M_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../axis-g3m/target/deploy/axis_g3m.so"
);
pub const PFDA_AMM_3_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pfda-amm-3/target/deploy/pfda_amm_3.so"
);
pub const PFDA_AMM_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pfda-amm/target/deploy/pfda_amm.so"
);
pub const AXIS_VAULT_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../axis-vault/target/deploy/axis_vault.so"
);
pub const JUPITER_V6_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../axis-g3m/fixtures/jupiter_v6.so"
);

/// Drain-only Jupiter substitute used by axis-vault WithdrawSol bound
/// tests. See `contracts/ab-integration-tests/mock-jupiter/` for the
/// program source. Loaded into LiteSVM at the canonical
/// `JUPITER_V6_ID` so axis-vault's `JUPITER_PROGRAM_ID` check passes.
pub const MOCK_JUPITER_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/mock_jupiter.so"
);

/// Metaplex Token Metadata Program (mainnet/devnet) — required by
/// axis-vault v1.1 CreateEtf. Tests that drive a real CreateEtf flow
/// must load this into LiteSVM at `MPL_TOKEN_METADATA_ID`.
///
/// Dump into the fixtures dir before running the suite:
/// ```sh
/// solana program dump -u mainnet-beta \
///   metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s \
///   contracts/ab-integration-tests/fixtures/mpl_token_metadata.so
/// ```
/// CI does this in `ci/job-local-benchmark.sh` /
/// `ci/e2e-local-prepare.sh` so devs and CI share one source.
pub const MPL_TOKEN_METADATA_SO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/mpl_token_metadata.so"
);

#[macro_export]
macro_rules! require_fixture {
    ($path:expr) => {
        if !std::path::Path::new($path).exists() {
            eprintln!("SKIP: fixture not found — {}", $path);
            return;
        }
    };
}

pub fn axis_g3m_id() -> Address {
    AXIS_G3M_ID.parse().unwrap()
}
pub fn pfda3_id() -> Address {
    PFDA_AMM_3_ID.parse().unwrap()
}
pub fn pfda_amm_id() -> Address {
    PFDA_AMM_ID.parse().unwrap()
}
pub fn axis_vault_id() -> Address {
    AXIS_VAULT_ID.parse().unwrap()
}
pub fn jupiter_id() -> Address {
    JUPITER_V6_ID.parse().unwrap()
}
pub fn mpl_token_metadata_id() -> Address {
    MPL_TOKEN_METADATA_ID.parse().unwrap()
}

/// Derive the Metaplex Token Metadata PDA for a given mint:
/// `[b"metadata", METAPLEX_PROGRAM_ID, mint]`. Used by axis-vault v1.1
/// CreateEtf — tests that build the ix by hand must include this PDA
/// in the account list.
pub fn metadata_pda_for(mint: &Address) -> Address {
    let mpl = mpl_token_metadata_id();
    Address::find_program_address(
        &[b"metadata", mpl.as_ref(), mint.as_ref()],
        &mpl,
    )
    .0
}

/// Create a LiteSVM with both AMM programs loaded. Returns None if fixtures missing.
pub fn create_dual_program_svm() -> Option<LiteSVM> {
    if !std::path::Path::new(AXIS_G3M_SO).exists() || !std::path::Path::new(PFDA_AMM_3_SO).exists()
    {
        return None;
    }

    let mut svm = LiteSVM::new();
    svm.add_program_from_file(axis_g3m_id(), AXIS_G3M_SO).ok()?;
    svm.add_program_from_file(pfda3_id(), PFDA_AMM_3_SO).ok()?;

    if std::path::Path::new(JUPITER_V6_SO).exists() {
        let _ = svm.add_program_from_file(jupiter_id(), JUPITER_V6_SO);
    }

    Some(svm)
}

/// Warp SVM clock to a target slot.
pub fn warp_to_slot(svm: &mut LiteSVM, target_slot: u64) {
    let clock = Clock {
        slot: target_slot,
        epoch_start_timestamp: 0,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: target_slot as i64,
    };
    svm.set_sysvar(&clock);
}

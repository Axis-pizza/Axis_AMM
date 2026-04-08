use litesvm::LiteSVM;
use solana_address::Address;
use solana_clock::Clock;

pub const AXIS_G3M_ID: &str = "65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi";
pub const PFDA_AMM_3_ID: &str = "DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf";
pub const JUPITER_V6_ID: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";

pub const AXIS_G3M_SO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../axis-g3m/target/deploy/axis_g3m.so");
pub const PFDA_AMM_3_SO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../pfda-amm-3/target/deploy/pfda_amm_3.so");
pub const JUPITER_V6_SO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../axis-g3m/fixtures/jupiter_v6.so");

#[macro_export]
macro_rules! require_fixture {
    ($path:expr) => {
        if !std::path::Path::new($path).exists() {
            eprintln!("SKIP: fixture not found — {}", $path);
            return;
        }
    };
}

pub fn axis_g3m_id() -> Address { AXIS_G3M_ID.parse().unwrap() }
pub fn pfda3_id() -> Address { PFDA_AMM_3_ID.parse().unwrap() }
pub fn jupiter_id() -> Address { JUPITER_V6_ID.parse().unwrap() }

/// Create a LiteSVM with both AMM programs loaded. Returns None if fixtures missing.
pub fn create_dual_program_svm() -> Option<LiteSVM> {
    if !std::path::Path::new(AXIS_G3M_SO).exists()
        || !std::path::Path::new(PFDA_AMM_3_SO).exists()
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

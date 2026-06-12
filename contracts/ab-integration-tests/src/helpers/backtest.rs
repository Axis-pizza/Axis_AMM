use serde::Deserialize;

// ─── PfdaBackend ─────────────────────────────────────────────────────────────

/// Build a synthetic Switchboard PullFeedAccountData blob.
/// Offsets verified against contracts/pfda-amm-3/src/oracle.rs:
///   [0..8]       PULL_FEED_DISCRIMINATOR
///   [1272..1288] CurrentResult.value (i128 LE, scaled 1e18)
///   [1336]       CurrentResult.num_samples (u8)
///   [1344..1352] CurrentResult.slot (u64 LE)
///   total size   1360 bytes
fn build_switchboard_feed(price_q_1e18: i128, slot: u64, samples: u8) -> Vec<u8> {
    const SIZE: usize = 1360;
    let mut d = vec![0u8; SIZE];
    d[0..8].copy_from_slice(&[0xc4, 0x1b, 0x6c, 0xc4, 0x0a, 0xd7, 0xdb, 0x28]);
    d[1272..1288].copy_from_slice(&price_q_1e18.to_le_bytes());
    d[1336] = samples;
    d[1344..1352].copy_from_slice(&slot.to_le_bytes());
    d
}

/// Switchboard On-Demand V3 mainnet program ID bytes.
const SWITCHBOARD_V3: [u8; 32] = [
    0x06, 0x73, 0xbd, 0x46, 0xf2, 0xe4, 0x7e, 0x04,
    0xf1, 0x2b, 0xd9, 0x2f, 0xb7, 0x31, 0x96, 0x8e,
    0xcd, 0x9d, 0x97, 0x57, 0xc2, 0x74, 0xda, 0x87,
    0x47, 0x6f, 0x46, 0x5c, 0x04, 0x0c, 0x65, 0x73,
];

#[derive(Debug, Clone, Copy)]
pub struct PfdaSwapResult {
    pub out_amount: u64,
    pub clearing_price_q32: u64,
    pub cu: u64,
}

/// Execute one in->out swap through a real pfda-3 batch (swap_request,
/// clear_batch WITH oracle feeds, claim) against a freshly seeded pool
/// with the given reserves and oracle feed prices (1e18-scaled).
/// Returns the realized out-token amount the user receives at claim.
/// Returns None only if the .so fixture is missing.
pub fn pfda_execute_swap(
    reserves: [u64; 3],
    feed_prices_1e18: [i128; 3],
    weights: [u32; 3],
    base_fee_bps: u16,
    in_idx: usize,
    out_idx: usize,
    amount_in: u64,
) -> Option<PfdaSwapResult> {
    use crate::helpers::account_builder::{build_batch_queue_3, build_pfda3_pool_state};
    use crate::helpers::svm_setup::{pfda3_id, warp_to_slot, PFDA_AMM_3_SO};
    use crate::helpers::token_factory::{
        create_mint, create_token_account, read_token_amount, system_program_id, token_program_id,
    };
    use litesvm::LiteSVM;
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{account_meta::AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_native_token::LAMPORTS_PER_SOL;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    if !std::path::Path::new(PFDA_AMM_3_SO).exists() {
        return None;
    }

    let pid = pfda3_id();
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(pid, PFDA_AMM_3_SO).ok()?;

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 100 * LAMPORTS_PER_SOL).ok()?;

    // Create 3 mints
    let mints: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for &m in &mints {
        create_mint(&mut svm, m, &user.pubkey(), 6);
    }

    // Derive pool PDA
    let (pool, pool_bump) = Address::find_program_address(
        &[b"pool3", mints[0].as_ref(), mints[1].as_ref(), mints[2].as_ref()],
        &pid,
    );

    // Create vaults seeded with reserves
    let vaults: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, reserves[i]);
    }

    // Create user token accounts with generous supply
    let user_tokens: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        // Fund generously so amount_in is always covered
        let balance = amount_in.max(10_000_000_000);
        create_token_account(&mut svm, user_tokens[i], &mints[i], &user.pubkey(), balance);
    }

    // Seed pool state — window_end=100, current_batch_id=0, current slot < 100
    let current_window_end = 100u64;
    let (queue0, q0_bump) = Address::find_program_address(
        &[b"queue3", pool.as_ref(), &0u64.to_le_bytes()],
        &pid,
    );

    // Use a distinct treasury (user as authority)
    let treasury = Address::new_unique();
    let pool_data = build_pfda3_pool_state(
        &mints,
        &vaults,
        &reserves,
        &weights,
        10,                  // window_slots
        0,                   // current_batch_id
        current_window_end,
        &treasury,
        &user.pubkey(),      // authority = user so they can also crank
        base_fee_bps,
        pool_bump,
    );
    svm.set_account(
        pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pool_data,
            owner: pid,
            executable: false,
            rent_epoch: 0,
        },
    )
    .ok()?;

    let queue_data = build_batch_queue_3(&pool, 0, &[0; 3], current_window_end, q0_bump);
    svm.set_account(
        queue0,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: queue_data,
            owner: pid,
            executable: false,
            rent_epoch: 0,
        },
    )
    .ok()?;

    // Helper closure to send a transaction and return (cu, logs_on_error)
    let send_tx = |svm: &mut LiteSVM, ix: Instruction, signer: &Keypair| -> Result<u64, String> {
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&signer.pubkey()),
            &[signer],
            svm.latest_blockhash(),
        );
        match svm.send_transaction(tx) {
            Ok(meta) => Ok(meta.compute_units_consumed),
            Err(e) => {
                let mut msg = format!("err={:?}", e.err);
                for log in &e.meta.logs {
                    msg.push_str(&format!("\n  {}", log));
                }
                Err(msg)
            }
        }
    };

    // --- Step 1: swap_request at slot < 100 ---
    let (ticket, _) = Address::find_program_address(
        &[b"ticket3", pool.as_ref(), user.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pid,
    );

    let mut sr_data = vec![1u8]; // disc
    sr_data.push(in_idx as u8);
    sr_data.extend_from_slice(&amount_in.to_le_bytes());
    sr_data.push(out_idx as u8);
    sr_data.extend_from_slice(&0u64.to_le_bytes()); // min_out
    let swap_request_ix = Instruction {
        program_id: pid,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new(queue0, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(user_tokens[in_idx], false),
            AccountMeta::new(vaults[in_idx], false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: sr_data,
    };

    send_tx(&mut svm, swap_request_ix, &user)
        .map_err(|e| { eprintln!("[pfda_execute_swap] swap_request failed: {e}"); e })
        .ok()?;

    // --- Step 2: warp to slot 200, build oracle feeds, clear_batch ---
    warp_to_slot(&mut svm, 200);

    let feed_addrs: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        let feed_data = build_switchboard_feed(feed_prices_1e18[i], 200, 2);
        svm.set_account(
            feed_addrs[i],
            Account {
                lamports: LAMPORTS_PER_SOL,
                data: feed_data,
                owner: Address::from(SWITCHBOARD_V3),
                executable: false,
                rent_epoch: 0,
            },
        )
        .ok()?;
    }

    let (history0, _) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &0u64.to_le_bytes()],
        &pid,
    );
    let (queue1, _) = Address::find_program_address(
        &[b"queue3", pool.as_ref(), &1u64.to_le_bytes()],
        &pid,
    );

    // clear_batch with feeds at accounts 6/7/8
    let mut cb_data = vec![2u8]; // disc
    cb_data.extend_from_slice(&0u64.to_le_bytes()); // bid_lamports = 0
    let clear_batch_ix = Instruction {
        program_id: pid,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new(pool, false),
            AccountMeta::new(queue0, false),
            AccountMeta::new(history0, false),
            AccountMeta::new(queue1, false),
            AccountMeta::new_readonly(system_program_id(), false),
            // oracle feeds at 6, 7, 8
            AccountMeta::new_readonly(feed_addrs[0], false),
            AccountMeta::new_readonly(feed_addrs[1], false),
            AccountMeta::new_readonly(feed_addrs[2], false),
        ],
        data: cb_data,
    };

    let clear_cu = send_tx(&mut svm, clear_batch_ix, &user)
        .map_err(|e| { eprintln!("[pfda_execute_swap] clear_batch failed: {e}"); e })
        .ok()?;

    // Read clearing_price_q32 for out_idx from history
    let clearing_price_q32 = {
        if let Some(acc) = svm.get_account(&history0) {
            // ClearedBatchHistory3 layout (176 bytes):
            //   [0..8]   discriminator
            //   [8..40]  pool
            //   [40..48] batch_id
            //   [48..72] clearing_prices: [u64; 3]   (3 * 8 = 24 bytes)
            //   ...
            if acc.data.len() >= 72 {
                let off = 48 + out_idx * 8;
                u64::from_le_bytes(acc.data[off..off + 8].try_into().unwrap_or([0; 8]))
            } else {
                0
            }
        } else {
            0
        }
    };

    // --- Step 3: claim ---
    let before = read_token_amount(&svm, &user_tokens[out_idx]);

    let claim_ix = Instruction {
        program_id: pid,
        accounts: vec![
            AccountMeta::new_readonly(user.pubkey(), true),
            AccountMeta::new(pool, false),
            AccountMeta::new_readonly(history0, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(vaults[0], false),
            AccountMeta::new(vaults[1], false),
            AccountMeta::new(vaults[2], false),
            AccountMeta::new(user_tokens[0], false),
            AccountMeta::new(user_tokens[1], false),
            AccountMeta::new(user_tokens[2], false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: vec![3u8],
    };

    let claim_cu = send_tx(&mut svm, claim_ix, &user)
        .map_err(|e| { eprintln!("[pfda_execute_swap] claim failed: {e}"); e })
        .ok()?;

    let after = read_token_amount(&svm, &user_tokens[out_idx]);
    let out_amount = after.saturating_sub(before);

    Some(PfdaSwapResult {
        out_amount,
        clearing_price_q32,
        cu: clear_cu.max(claim_cu),
    })
}

// ─── Price row / calibration types ───────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct PriceRow { pub day: u32, pub sol_usd: f64, pub bonk_usd: f64, pub wif_usd: f64 }

pub fn load_prices(path: &str) -> Vec<PriceRow> {
    let txt = std::fs::read_to_string(path).expect("prices.csv");
    txt.lines().skip(1).filter(|l| !l.trim().is_empty()).map(|l| {
        let c: Vec<&str> = l.split(',').collect();
        PriceRow { day: c[0].parse().unwrap(), sol_usd: c[1].parse().unwrap(),
                   bonk_usd: c[2].parse().unwrap(), wif_usd: c[3].parse().unwrap() }
    }).collect()
}

#[derive(Debug, Deserialize)]
pub struct CalSample { pub in_amount: u64, pub out_amount: u64 }
#[derive(Debug, Deserialize)]
pub struct CalPair { pub r#in: String, pub out: String, pub mid_price_out_per_in: f64, pub samples: Vec<CalSample> }
#[derive(Debug, Deserialize)]
pub struct Calibration { pub pairs: Vec<CalPair> }

pub fn load_calibration(path: &str) -> Calibration {
    serde_json::from_str(&std::fs::read_to_string(path).expect("calibration")).expect("calibration json")
}

/// External-liquidity CPMM model with input-side depth L (input base units).
#[derive(Debug, Clone, Copy)]
pub struct JupModel { pub depth_l: f64, pub mid_price: f64 }

impl JupModel {
    /// out for input dx at mid price p with depth L.
    pub fn quote_out(&self, dx: f64) -> f64 { self.mid_price * dx * self.depth_l / (self.depth_l + dx) }
    /// effective slippage fraction for input dx.
    pub fn slippage_frac(&self, dx: f64) -> f64 { dx / (self.depth_l + dx) }
}

/// Solve L from one calibration sample. None when no positive impact.
pub fn calibrate_l(dx: f64, out: f64, mid: f64) -> Option<f64> {
    let denom = mid * dx - out;
    if denom <= 0.0 { None } else { Some(out * dx / denom) }
}

/// Average L across a pair's samples (mid = pair.mid_price_out_per_in).
pub fn calibrate_pair(p: &CalPair) -> JupModel {
    let ls: Vec<f64> = p.samples.iter()
        .filter_map(|s| calibrate_l(s.in_amount as f64, s.out_amount as f64, p.mid_price_out_per_in))
        .collect();
    let l = if ls.is_empty() { f64::INFINITY } else { ls.iter().sum::<f64>() / ls.len() as f64 };
    JupModel { depth_l: l, mid_price: p.mid_price_out_per_in }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    const PRICES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/prices.csv");
    const CAL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/jup_calibration.json");
    #[test]
    fn prices_load_and_are_ordered() {
        let rows = load_prices(PRICES);
        assert!(rows.len() >= 30);
        assert_eq!(rows[0].day, 0);
        assert!(rows.iter().all(|r| r.sol_usd > 0.0 && r.bonk_usd > 0.0 && r.wif_usd > 0.0));
    }
    #[test]
    fn calibration_loads() {
        let cal = load_calibration(CAL);
        assert!(!cal.pairs.is_empty());
        assert!(cal.pairs.iter().all(|p| !p.samples.is_empty()));
    }
}

#[cfg(test)]
mod jup_model_tests {
    use super::*;
    #[test]
    fn calibrated_model_reproduces_its_samples_within_tolerance() {
        let truth = JupModel { depth_l: 50_000_000_000.0, mid_price: 5_000_000.0 };
        let sizes = [1_000_000_000.0, 10_000_000_000.0, 100_000_000_000.0];
        for dx in sizes {
            let out = truth.quote_out(dx);
            let l = calibrate_l(dx, out, truth.mid_price).unwrap();
            assert!((l - truth.depth_l).abs() / truth.depth_l < 1e-6, "L recovered: {l}");
        }
    }
    #[test]
    fn larger_trade_has_more_slippage() {
        let m = JupModel { depth_l: 1_000.0, mid_price: 1.0 };
        assert!(m.slippage_frac(100.0) < m.slippage_frac(500.0));
    }
    #[test]
    fn calibrate_from_fixture() {
        let cal = load_calibration(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/jup_calibration.json"));
        let m = calibrate_pair(&cal.pairs[0]);
        assert!(m.depth_l.is_finite() && m.depth_l > 0.0);
    }
}

#[cfg(test)]
mod pfda_backend_tests {
    use super::*;
    #[test]
    fn pfda_swap_returns_positive_out() {
        if !std::path::Path::new(crate::helpers::svm_setup::PFDA_AMM_3_SO).exists() { return; }
        let r = pfda_execute_swap(
            [1_000_000_000; 3],
            [1_000_000_000_000_000_000, 1_000_000_000_000_000_000, 1_000_000_000_000_000_000],
            [333_333, 333_333, 333_334], 30, 0, 1, 1_000_000,
        ).unwrap();
        println!("pfda_swap out_amount={} clearing_price_q32={} cu={}", r.out_amount, r.clearing_price_q32, r.cu);
        assert!(r.out_amount > 0, "expected positive realized out, got {}", r.out_amount);
    }
}

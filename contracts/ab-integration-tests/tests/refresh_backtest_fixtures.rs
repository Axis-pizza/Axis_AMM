//! Offline fixture refresher for backtest stage-2.
//!
//! This test is **always ignored** (`#[ignore]`).  It is a no-op unless the
//! caller provides `MAINNET_RPC_URL` in the environment.  When an RPC URL is
//! present it fetches a small grid of SOL→BONK Jupiter routes and serialises
//! each one as JSON under `fixtures/backtest/jup_routes/`.
//!
//! # Usage
//! ```sh
//! MAINNET_RPC_URL=https://api.mainnet-beta.solana.com \
//!   cargo test --test refresh_backtest_fixtures -- --ignored --nocapture
//! ```
//!
//! Without `MAINNET_RPC_URL` the test exits immediately with an informational
//! message — it never contacts the network and never fails the CI suite.

use ab_integration_tests::helpers::mainnet_fork::{fetch_jupiter_route, JupiterRoute};
use serde_json::{json, Value};

// ─── Known mainnet mint addresses ────────────────────────────────────────────

/// Wrapped SOL mint (So11111111111111111111111111111111111111112).
const WSOL_MINT_B58: &str = "So11111111111111111111111111111111111111112";
/// BONK mint (DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263).
const BONK_MINT_B58: &str = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";

// Trade sizes in lamports / native SOL units (9 decimals).
// 0.01 SOL, 0.1 SOL, 1 SOL — small enough to avoid rate-limit issues on
// the public Jupiter API while giving three distinct depth calibration points.
const SIZES: &[u64] = &[
    10_000_000,  // 0.01 SOL
    100_000_000, // 0.10 SOL
    1_000_000_000, // 1.00 SOL
];

// ─── Serialisation helpers ────────────────────────────────────────────────────

fn parse_address_b58(s: &str) -> solana_address::Address {
    let bytes = bs58::decode(s).into_vec().expect("invalid base58");
    let arr: [u8; 32] = bytes.try_into().expect("not 32 bytes");
    solana_address::Address::from(arr)
}

fn route_to_json(label: &str, route: &JupiterRoute) -> Value {
    use base64::Engine;
    let swap_data_b64 = base64::engine::general_purpose::STANDARD.encode(&route.swap_data);
    let accounts: Vec<Value> = route
        .accounts
        .iter()
        .map(|a| {
            json!({
                "pubkey":      bs58::encode(a.pubkey.as_ref()).into_string(),
                "is_signer":   a.is_signer,
                "is_writable": a.is_writable,
            })
        })
        .collect();
    let alts: Vec<Value> = route
        .address_lookup_tables
        .iter()
        .map(|a| Value::String(bs58::encode(a.as_ref()).into_string()))
        .collect();

    json!({
        "label":      label,
        "in_mint":    bs58::encode(route.accounts.first().map(|a| a.pubkey.as_ref()).unwrap_or(&[0u8;32])).into_string(),
        "out_mint":   bs58::encode(route.accounts.last().map(|a| a.pubkey.as_ref()).unwrap_or(&[0u8;32])).into_string(),
        "in_amount":  route.in_amount,
        "out_amount": route.out_amount,
        "swap_data":  swap_data_b64,
        "accounts":   accounts,
        "address_lookup_tables": alts,
    })
}

// ─── Test ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn refresh_backtest_fixtures() {
    // ── Early exit without network ────────────────────────────────────────
    let rpc_url = match std::env::var("MAINNET_RPC_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!(
                "[refresh_backtest_fixtures] MAINNET_RPC_URL not set — skipping. \
                 Set it to a mainnet RPC endpoint and re-run with --ignored to populate fixtures."
            );
            return;
        }
    };

    // ── Output directory ──────────────────────────────────────────────────
    let out_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/backtest/jup_routes"
    );
    std::fs::create_dir_all(out_dir)
        .expect("could not create fixtures/backtest/jup_routes");

    let wsol = parse_address_b58(WSOL_MINT_B58);
    let bonk = parse_address_b58(BONK_MINT_B58);

    // Dummy user pubkey — we only need the route shape, not a real account.
    // The actual signer in the replayed CPI will be the pool PDA.
    let dummy_user = solana_address::Address::new_unique();

    eprintln!("[refresh_backtest_fixtures] RPC={}", &rpc_url[..rpc_url.len().min(60)]);
    eprintln!(
        "[refresh_backtest_fixtures] Fetching {} SOL→BONK routes …",
        SIZES.len()
    );

    let mut written = 0usize;

    for (i, &size) in SIZES.iter().enumerate() {
        let label = format!(
            "SOL->BONK {:.4} SOL",
            size as f64 / 1_000_000_000.0
        );
        eprintln!("  route_{i}: {} …", label);

        match fetch_jupiter_route(&wsol, &bonk, size, 50, &dummy_user) {
            Ok(route) => {
                let json_val = route_to_json(&label, &route);
                let json_str = serde_json::to_string_pretty(&json_val)
                    .expect("json serialisation failed");
                let path = format!("{}/route_{}.json", out_dir, i);
                std::fs::write(&path, &json_str)
                    .unwrap_or_else(|e| panic!("write {} failed: {}", path, e));
                eprintln!(
                    "  → written {} (in={} out={})",
                    path, route.in_amount, route.out_amount
                );
                written += 1;
            }
            Err(e) => {
                eprintln!("  warn: route_{i} fetch failed: {e}");
            }
        }
    }

    eprintln!(
        "[refresh_backtest_fixtures] Done — {}/{} routes written to {}",
        written,
        SIZES.len(),
        out_dir
    );
    assert!(written > 0, "all Jupiter route fetches failed — check RPC URL and network");
}

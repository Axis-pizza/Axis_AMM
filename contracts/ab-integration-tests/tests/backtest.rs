use ab_integration_tests::helpers::{backtest::*, svm_setup::*};
use ab_integration_tests::require_fixture;

#[test]
fn backtest_writes_report() {
    require_fixture!(PFDA_AMM_3_SO);
    let prices = load_prices(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/prices.csv"));
    let cal = load_calibration(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/jup_calibration.json"));
    let jm = calibrate_pair(&cal.pairs[0]);
    let summary = run_rebalance_backtest(&prices, [jm; 3], [333_333, 333_333, 333_334], 1_000_000.0);
    let mut md = render_report(&summary);
    eprintln!("backtest report stage-1: jup_total_cost_bps={:.2} pfda_total_cost_bps={:.2} jup_avg_te={:.2} pfda_avg_te={:.2}",
        summary.jup_total_cost_bps, summary.pfda_total_cost_bps, summary.jup_avg_te_bps, summary.pfda_avg_te_bps);

    // ── MEV section: run sandwich probe for a couple of victim sizes ────────
    // Use a shallow depth (depth_l=$5M) to ensure the $200k victim produces
    // meaningful jup extraction (victim/depth ≈ 4% → ~2 bps extracted).
    // Reserves and weights matching the pool fixture used in pfda_backend_tests.
    let mev_jm = JupModel { depth_l: 5_000_000.0, mid_price: 1.0 };
    let mev_reserves = [1_000_000_000u64; 3];
    let mev_weights = [333_333u32, 333_333, 333_334];
    let mev_results: Vec<MevResult> = [50_000.0f64, 200_000.0f64]
        .iter()
        .map(|&v| mev_probe(&mev_jm, mev_reserves, mev_weights, 30, 0, 1, v))
        .collect();

    for r in &mev_results {
        eprintln!(
            "MEV probe: victim=${:.0}  jup_extracted_bps={:.4}  pfda_extracted_bps={:.4}",
            r.victim_size_usd, r.jup_extracted_bps, r.pfda_extracted_bps
        );
    }

    md.push('\n');
    md.push_str(&render_mev(&mev_results));

    // ── Stage-2: recorded-route execution quality ──────────────────────────
    // Skips cleanly when the fixtures dir is empty (CI).  Prints a skip line
    // to stderr so the grep "[backtest] stage-2 skipped" assertion passes.
    let stage2_results = stage2_exec_quality(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/backtest/jup_routes"
    ));
    md.push('\n');
    md.push_str(&render_stage2(&stage2_results));

    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/backtest-report.md");
    std::fs::write(out, &md).unwrap();
    eprintln!("backtest report -> {out}");
    assert!(summary.steps.len() >= 30);
}

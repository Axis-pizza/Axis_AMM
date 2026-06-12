use ab_integration_tests::helpers::{backtest::*, svm_setup::*};
use ab_integration_tests::require_fixture;

#[test]
fn backtest_writes_report() {
    require_fixture!(PFDA_AMM_3_SO);
    let prices = load_prices(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/prices.csv"));
    let cal = load_calibration(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/backtest/jup_calibration.json"));
    let jm = calibrate_pair(&cal.pairs[0]);
    let summary = run_rebalance_backtest(&prices, [jm; 3], [333_333, 333_333, 333_334], 1_000_000.0);
    let md = render_report(&summary);
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/backtest-report.md");
    std::fs::write(out, &md).unwrap();
    eprintln!("backtest report -> {out}");
    eprintln!("jup_total_cost_bps={:.2} pfda_total_cost_bps={:.2} jup_avg_te={:.2} pfda_avg_te={:.2}",
        summary.jup_total_cost_bps, summary.pfda_total_cost_bps, summary.jup_avg_te_bps, summary.pfda_avg_te_bps);
    assert!(summary.steps.len() >= 30);
}

/// Metrics collected from a G3M (ETF B) test run.
#[derive(Debug, Default)]
pub struct G3mMetrics {
    pub init_cu: u64,
    pub swap_cu: u64,
    pub check_drift_cu: u64,
    pub rebalance_cu: u64,
    pub pre_k: u128,
    pub post_k: u128,
    pub total_slots: u64,
}

/// Metrics collected from a PFDA-3 (ETF A) test run.
#[derive(Debug, Default)]
pub struct Pfda3Metrics {
    pub init_cu: u64,
    pub add_liq_cu: u64,
    pub swap_request_cu: u64,
    pub clear_batch_cu: u64,
    pub claim_cu: u64,
    pub clearing_prices: [u64; 3],
    pub total_value_in: u64,
    pub batch_window_slots: u64,
    pub total_slots: u64,
}

/// A/B comparison result.
pub struct ABComparison {
    pub g3m: G3mMetrics,
    pub pfda3: Pfda3Metrics,
}

impl ABComparison {
    pub fn print_report(&self) {
        let g = &self.g3m;
        let p = &self.pfda3;

        println!();
        println!("╔═══════════════════════════════════════════════════════════════╗");
        println!("║                    A/B Test Comparison                        ║");
        println!("╠═══════════════════════════════════════════════════════════════╣");
        println!("║  Metric                │  ETF A (PFDA-3)  │  ETF B (G3M)     ║");
        println!("╠════════════════════════╪══════════════════╪══════════════════╣");
        println!("║  Init CU              │  {:>14}  │  {:>14}  ║", p.init_cu, g.init_cu);
        println!("║  Swap/SwapRequest CU  │  {:>14}  │  {:>14}  ║", p.swap_request_cu, g.swap_cu);
        println!("║  Clear/Rebalance CU   │  {:>14}  │  {:>14}  ║", p.clear_batch_cu, g.rebalance_cu);
        println!("║  Claim CU             │  {:>14}  │  {:>14}  ║", p.claim_cu, "N/A");
        println!("║  Total CU             │  {:>14}  │  {:>14}  ║",
            p.init_cu + p.add_liq_cu + p.swap_request_cu + p.clear_batch_cu + p.claim_cu,
            g.init_cu + g.swap_cu + g.check_drift_cu + g.rebalance_cu);
        println!("╠════════════════════════╪══════════════════╪══════════════════╣");

        let g_inv = if g.pre_k > 0 {
            ((g.post_k as i128 - g.pre_k as i128) * 10_000 / g.pre_k as i128) as i64
        } else { 0 };
        println!("║  Invariant Δ (bps)    │  {:>14}  │  {:>14}  ║", "—", g_inv);
        println!("║  Execution slots      │  {:>14}  │  {:>14}  ║", p.total_slots, g.total_slots);
        println!("╚═══════════════════════════════════════════════════════════════╝");
    }
}

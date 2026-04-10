use serde::Serialize;

/// Metrics collected from a G3M (ETF B) test run.
#[derive(Debug, Default, Clone, Serialize)]
pub struct G3mMetrics {
    pub init_cu: u64,
    pub swap_cu: u64,
    pub drift_swap_cu: u64,
    pub check_drift_cu: u64,
    pub rebalance_cu: u64,
    pub pre_k: u128,
    pub post_k: u128,
    pub pre_reserves: Vec<u64>,
    pub post_reserves: Vec<u64>,
    pub total_slots: u64,
    pub tokens_received: u64,
    pub effective_price: f64,
    pub slippage_bps: f64,
    pub price_improvement_bps: f64,
    pub post_trade_drift_bps: f64,
    pub invariant_delta_bps: f64,
    pub rebalance_frequency: u64,
    pub rebalance_effectiveness_bps: f64,
    pub fee_revenue: u64,
    pub treasury_delta: i64,
    pub net_cost_lamports: f64,
    pub success: bool,
    pub retries: u64,
    pub timeouts: u64,
    pub critical_invariant_violation: bool,
    pub slot_to_finality: u64,
    pub cold_start_cu: u64,
    pub steady_state_cu: u64,
    pub total_cu: u64,
}

/// Metrics collected from a PFDA-3 (ETF A) test run.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Pfda3Metrics {
    pub init_cu: u64,
    pub add_liq_cu: u64,
    pub swap_request_cu: u64,
    pub clear_batch_cu: u64,
    pub claim_cu: u64,
    pub clearing_prices: [u64; 3],
    pub total_value_in: u64,
    pub tokens_received: u64,
    pub batch_window_slots: u64,
    pub total_slots: u64,
    pub effective_price: f64,
    pub slippage_bps: f64,
    pub price_improvement_bps: f64,
    pub post_trade_drift_bps: f64,
    pub invariant_delta_bps: f64,
    pub rebalance_frequency: u64,
    pub rebalance_effectiveness_bps: f64,
    pub fee_revenue: u64,
    pub treasury_delta: i64,
    pub net_cost_lamports: f64,
    pub success: bool,
    pub retries: u64,
    pub timeouts: u64,
    pub critical_invariant_violation: bool,
    pub slot_to_finality: u64,
    pub cold_start_cu: u64,
    pub steady_state_cu: u64,
    pub total_cu: u64,
}

/// Full A/B comparison report.
#[derive(Debug, Serialize)]
pub struct ABReport {
    pub generated_at: String,
    pub environment: String,
    pub scenarios: Vec<ABScenario>,
}

/// A single A/B test scenario (e.g. "balanced pool", "imbalanced pool", "large swap").
#[derive(Debug, Serialize)]
pub struct ABScenario {
    pub name: String,
    pub description: String,
    pub swap_amount: u64,
    pub initial_reserves: Vec<u64>,
    pub g3m: G3mMetrics,
    pub pfda3: Pfda3Metrics,
}

impl ABReport {
    pub fn new(environment: &str) -> Self {
        ABReport {
            generated_at: chrono_lite_now(),
            environment: environment.to_string(),
            scenarios: Vec::new(),
        }
    }

    pub fn add_scenario(&mut self, s: ABScenario) {
        self.scenarios.push(s);
    }

    /// Export as JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Export as Markdown string.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Axis A/B Test Report\n\n");
        md.push_str(&format!("- Generated: {}\n", self.generated_at));
        md.push_str(&format!("- Environment: {}\n\n", self.environment));

        for (i, s) in self.scenarios.iter().enumerate() {
            md.push_str(&format!("## Scenario {}: {}\n\n", i + 1, s.name));
            md.push_str(&format!("{}\n\n", s.description));
            md.push_str(&format!("- Swap amount: {}\n", s.swap_amount));
            md.push_str(&format!("- Initial reserves: {:?}\n\n", s.initial_reserves));

            let g = &s.g3m;
            let p = &s.pfda3;
            let g_total =
                g.init_cu + g.swap_cu + g.drift_swap_cu + g.check_drift_cu + g.rebalance_cu;
            let p_total =
                p.init_cu + p.add_liq_cu + p.swap_request_cu + p.clear_batch_cu + p.claim_cu;

            md.push_str("| Metric | ETF A (PFDA-3) | ETF B (G3M) |\n");
            md.push_str("|--------|---------------:|------------:|\n");
            md.push_str(&format!("| Init CU | {} | {} |\n", p.init_cu, g.init_cu));
            md.push_str(&format!(
                "| Swap/Request CU | {} | {} |\n",
                p.swap_request_cu, g.swap_cu
            ));
            if g.drift_swap_cu > 0 {
                md.push_str(&format!(
                    "| Drift-Trigger Swap CU | N/A | {} |\n",
                    g.drift_swap_cu
                ));
            }
            md.push_str(&format!(
                "| Clear/Rebalance CU | {} | {} |\n",
                p.clear_batch_cu, g.rebalance_cu
            ));
            md.push_str(&format!("| Claim CU | {} | N/A |\n", p.claim_cu));
            md.push_str(&format!(
                "| **Total CU** | **{}** | **{}** |\n",
                p_total, g_total
            ));
            md.push_str(&format!(
                "| Tokens received | {} | {} |\n",
                p.tokens_received, g.tokens_received
            ));
            md.push_str(&format!(
                "| Execution slots | {} | {} |\n",
                p.total_slots, g.total_slots
            ));

            if g.pre_k > 0 {
                let delta =
                    ((g.post_k as i128 - g.pre_k as i128) * 10_000 / g.pre_k as i128) as i64;
                md.push_str(&format!("| Invariant delta (bps) | — | {} |\n", delta));
            }
            md.push_str("\n");
        }

        // Summary
        if self.scenarios.len() > 1 {
            md.push_str("## Summary\n\n");
            let avg_g: u64 = self
                .scenarios
                .iter()
                .map(|s| {
                    s.g3m.init_cu
                        + s.g3m.swap_cu
                        + s.g3m.drift_swap_cu
                        + s.g3m.check_drift_cu
                        + s.g3m.rebalance_cu
                })
                .sum::<u64>()
                / self.scenarios.len() as u64;
            let avg_p: u64 = self
                .scenarios
                .iter()
                .map(|s| {
                    s.pfda3.init_cu
                        + s.pfda3.add_liq_cu
                        + s.pfda3.swap_request_cu
                        + s.pfda3.clear_batch_cu
                        + s.pfda3.claim_cu
                })
                .sum::<u64>()
                / self.scenarios.len() as u64;
            md.push_str(&format!(
                "- Average total CU: ETF A = {}, ETF B = {}\n",
                avg_p, avg_g
            ));
            md.push_str(&format!(
                "- CU efficiency: ETF B uses {:.0}% of ETF A's compute\n",
                avg_g as f64 / avg_p.max(1) as f64 * 100.0
            ));
        }

        md
    }

    /// Print table to stdout.
    pub fn print_table(&self) {
        for s in &self.scenarios {
            let g = &s.g3m;
            let p = &s.pfda3;

            println!();
            println!("━━━ {} ━━━", s.name);
            println!("╔════════════════════════╤══════════════════╤══════════════════╗");
            println!("║  Metric                │  ETF A (PFDA-3)  │  ETF B (G3M)     ║");
            println!("╠════════════════════════╪══════════════════╪══════════════════╣");
            println!(
                "║  Init CU              │  {:>14}  │  {:>14}  ║",
                p.init_cu, g.init_cu
            );
            println!(
                "║  Swap/SwapRequest CU  │  {:>14}  │  {:>14}  ║",
                p.swap_request_cu, g.swap_cu
            );
            println!(
                "║  Clear/Rebalance CU   │  {:>14}  │  {:>14}  ║",
                p.clear_batch_cu, g.rebalance_cu
            );
            println!(
                "║  Claim CU             │  {:>14}  │  {:>14}  ║",
                p.claim_cu, "N/A"
            );
            println!(
                "║  Total CU             │  {:>14}  │  {:>14}  ║",
                p.init_cu + p.add_liq_cu + p.swap_request_cu + p.clear_batch_cu + p.claim_cu,
                g.init_cu + g.swap_cu + g.drift_swap_cu + g.check_drift_cu + g.rebalance_cu
            );
            println!("╠════════════════════════╪══════════════════╪══════════════════╣");
            println!(
                "║  Tokens received      │  {:>14}  │  {:>14}  ║",
                p.tokens_received, g.tokens_received
            );
            println!(
                "║  Execution slots      │  {:>14}  │  {:>14}  ║",
                p.total_slots, g.total_slots
            );
            println!("╚════════════════════════╧══════════════════╧══════════════════╝");
        }
    }
}

// ─── PR Validation report models ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default)]
pub struct StatsSummary {
    pub n: usize,
    pub p50: f64,
    pub p95: f64,
    pub mean: f64,
    pub std: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProtocolAggregate {
    pub success_rate_pct: f64,
    pub failure_rate_pct: f64,
    pub retry_rate_pct: f64,
    pub timeout_rate_pct: f64,
    pub critical_invariant_violations: u64,
    pub cold_start_cu: StatsSummary,
    pub steady_state_cu: StatsSummary,
    pub total_cu: StatsSummary,
    pub slot_to_finality: StatsSummary,
    pub tokens_out: StatsSummary,
    pub effective_price: StatsSummary,
    pub slippage_bps: StatsSummary,
    pub price_improvement_bps: StatsSummary,
    pub post_trade_drift_bps: StatsSummary,
    pub invariant_delta_bps: StatsSummary,
    pub rebalance_frequency: StatsSummary,
    pub rebalance_effectiveness_bps: StatsSummary,
    pub fee_revenue: StatsSummary,
    pub treasury_delta: StatsSummary,
    pub net_cost_lamports: StatsSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScenarioRunRecord {
    pub run_index: usize,
    pub seed: String,
    pub token_sample: Vec<String>,
    pub comparison_tokens: Vec<String>,
    pub comparable_for_gate: bool,
    pub pfda3: Pfda3Metrics,
    pub g3m: G3mMetrics,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MetricSignificance {
    pub metric: String,
    pub n_baseline: usize,
    pub n_candidate: usize,
    pub effect_mean_delta: f64,
    pub bootstrap_ci95: Option<[f64; 2]>,
    pub mann_whitney_p: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScenarioValidationSummary {
    pub id: String,
    pub description: String,
    pub scenario_seed: String,
    pub repeats: usize,
    pub attempts: usize,
    pub token_sample: Vec<String>,
    pub comparison_tokens: Vec<String>,
    pub swap_ratio_bps: u16,
    pub comparable_for_gate: bool,
    pub comparable_runs: usize,
    pub pfda3: ProtocolAggregate,
    pub g3m: ProtocolAggregate,
    pub significance: Vec<MetricSignificance>,
    pub runs: Vec<ScenarioRunRecord>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GateCheck {
    pub gate: String,
    pub pass: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GateResult {
    pub baseline: String,
    pub candidate: String,
    pub all_pass: bool,
    pub checks: Vec<GateCheck>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct EnvironmentValidationReport {
    pub name: String,
    pub status: String,
    pub notes: Vec<String>,
    pub scenarios: Vec<ScenarioValidationSummary>,
    pub gate: Option<GateResult>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct FairnessRules {
    pub token_universe_candidates: Vec<String>,
    pub initial_liquidity_rule: String,
    pub fee_rule: String,
    pub swap_rule: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PRValidationReport {
    pub generated_at: String,
    pub run_id: String,
    pub base_seed: String,
    pub repeats_per_scenario: usize,
    pub fairness: FairnessRules,
    pub environments: Vec<EnvironmentValidationReport>,
}

impl PRValidationReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Axis A/B PR Validation Report\n\n");
        md.push_str(&format!("- Generated At: {}\n", self.generated_at));
        md.push_str(&format!("- Run ID: {}\n", self.run_id));
        md.push_str(&format!("- Base Seed: {}\n", self.base_seed));
        md.push_str(&format!(
            "- Repeats/Scenario: {}\n\n",
            self.repeats_per_scenario
        ));

        md.push_str("## Fairness Rules\n\n");
        md.push_str(&format!(
            "- Token universe candidates: {:?}\n",
            self.fairness.token_universe_candidates
        ));
        md.push_str(&format!(
            "- Initial liquidity: {}\n",
            self.fairness.initial_liquidity_rule
        ));
        md.push_str(&format!("- Fee rule: {}\n", self.fairness.fee_rule));
        md.push_str(&format!("- Swap rule: {}\n", self.fairness.swap_rule));
        for note in &self.fairness.notes {
            md.push_str(&format!("- Note: {}\n", note));
        }
        md.push('\n');

        for env in &self.environments {
            md.push_str(&format!("## Environment: {}\n\n", env.name));
            md.push_str(&format!("- Status: {}\n", env.status));
            for note in &env.notes {
                md.push_str(&format!("- Note: {}\n", note));
            }
            md.push('\n');

            if env.status != "completed" {
                continue;
            }

            if let Some(gate) = &env.gate {
                md.push_str("### Multi-Metric Gate\n\n");
                md.push_str(&format!("- Baseline: {}\n", gate.baseline));
                md.push_str(&format!("- Candidate: {}\n", gate.candidate));
                md.push_str(&format!(
                    "- Gate Result: **{}**\n\n",
                    if gate.all_pass { "PASS" } else { "FAIL" }
                ));
                md.push_str("| Gate | Pass | Detail |\n");
                md.push_str("|---|---|---|\n");
                for check in &gate.checks {
                    md.push_str(&format!(
                        "| {} | {} | {} |\n",
                        check.gate,
                        if check.pass { "YES" } else { "NO" },
                        check.detail
                    ));
                }
                md.push('\n');
            }

            for scenario in &env.scenarios {
                md.push_str(&format!("### Scenario: {}\n\n", scenario.id));
                md.push_str(&format!("- Description: {}\n", scenario.description));
                md.push_str(&format!("- Scenario seed: {}\n", scenario.scenario_seed));
                md.push_str(&format!("- Token sample: {:?}\n", scenario.token_sample));
                md.push_str(&format!(
                    "- Comparison tokens: {:?}\n",
                    scenario.comparison_tokens
                ));
                md.push_str(&format!(
                    "- Comparable for gate: {}\n",
                    scenario.comparable_for_gate
                ));
                md.push_str(&format!("- Target repeats: {}\n", scenario.repeats));
                md.push_str(&format!("- Attempts: {}\n", scenario.attempts));
                md.push_str(&format!(
                    "- Comparable runs: {}\n\n",
                    scenario.comparable_runs
                ));

                md.push_str("| Metric | ETF A (PFDA-3) | ETF B (G3M) |\n");
                md.push_str("|---|---:|---:|\n");
                md.push_str(&format!(
                    "| Total CU p50/p95 | {:.2} / {:.2} | {:.2} / {:.2} |\n",
                    scenario.pfda3.total_cu.p50,
                    scenario.pfda3.total_cu.p95,
                    scenario.g3m.total_cu.p50,
                    scenario.g3m.total_cu.p95
                ));
                md.push_str(&format!(
                    "| Slippage bps p50/p95 | {:.2} / {:.2} | {:.2} / {:.2} |\n",
                    scenario.pfda3.slippage_bps.p50,
                    scenario.pfda3.slippage_bps.p95,
                    scenario.g3m.slippage_bps.p50,
                    scenario.g3m.slippage_bps.p95
                ));
                md.push_str(&format!(
                    "| Slots-to-finality p50/p95 | {:.2} / {:.2} | {:.2} / {:.2} |\n",
                    scenario.pfda3.slot_to_finality.p50,
                    scenario.pfda3.slot_to_finality.p95,
                    scenario.g3m.slot_to_finality.p50,
                    scenario.g3m.slot_to_finality.p95
                ));
                md.push_str(&format!(
                    "| Success rate | {:.2}% | {:.2}% |\n",
                    scenario.pfda3.success_rate_pct, scenario.g3m.success_rate_pct
                ));
                md.push('\n');

                if !scenario.significance.is_empty() {
                    md.push_str("Significance checks:\n\n");
                    md.push_str("| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |\n");
                    md.push_str("|---|---:|---|---:|\n");
                    for s in &scenario.significance {
                        let ci = match s.bootstrap_ci95 {
                            Some([l, h]) => format!("[{:.4}, {:.4}]", l, h),
                            None => "N/A".to_string(),
                        };
                        let p = s
                            .mann_whitney_p
                            .map(|v| format!("{:.6}", v))
                            .unwrap_or_else(|| "N/A".to_string());
                        md.push_str(&format!(
                            "| {} | {:.4} | {} | {} |\n",
                            s.metric, s.effect_mean_delta, ci, p
                        ));
                    }
                    md.push('\n');
                }
            }
        }

        md
    }
}

pub fn summarize(samples: &[f64]) -> StatsSummary {
    if samples.is_empty() {
        return StatsSummary::default();
    }

    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let variance = if n > 1 {
        sorted
            .iter()
            .map(|v| {
                let d = *v - mean;
                d * d
            })
            .sum::<f64>()
            / (n - 1) as f64
    } else {
        0.0
    };

    StatsSummary {
        n,
        p50: percentile_sorted(&sorted, 0.50),
        p95: percentile_sorted(&sorted, 0.95),
        mean,
        std: variance.sqrt(),
        min: *sorted.first().unwrap_or(&0.0),
        max: *sorted.last().unwrap_or(&0.0),
    }
}

pub fn bootstrap_mean_diff_ci(
    baseline: &[f64],
    candidate: &[f64],
    iters: usize,
    seed: u64,
) -> Option<[f64; 2]> {
    if baseline.is_empty() || candidate.is_empty() || iters == 0 {
        return None;
    }

    let mut rng = SplitMix64::new(seed);
    let mut diffs = Vec::with_capacity(iters);

    for _ in 0..iters {
        let b_mean = bootstrap_mean(baseline, &mut rng);
        let c_mean = bootstrap_mean(candidate, &mut rng);
        diffs.push(c_mean - b_mean);
    }

    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some([
        percentile_sorted(&diffs, 0.025),
        percentile_sorted(&diffs, 0.975),
    ])
}

pub fn mann_whitney_u_pvalue(sample_a: &[f64], sample_b: &[f64]) -> Option<f64> {
    if sample_a.is_empty() || sample_b.is_empty() {
        return None;
    }

    let n1 = sample_a.len() as f64;
    let n2 = sample_b.len() as f64;
    let n_total = n1 + n2;

    let mut combined: Vec<(f64, bool)> = Vec::with_capacity(sample_a.len() + sample_b.len());
    combined.extend(sample_a.iter().map(|v| (*v, true)));
    combined.extend(sample_b.iter().map(|v| (*v, false)));
    combined.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut rank_sum_a = 0.0;
    let mut i = 0usize;
    let mut tie_correction_sum = 0.0;

    while i < combined.len() {
        let mut j = i + 1;
        while j < combined.len() && (combined[j].0 - combined[i].0).abs() <= f64::EPSILON {
            j += 1;
        }

        let rank_start = i as f64 + 1.0;
        let rank_end = j as f64;
        let avg_rank = (rank_start + rank_end) / 2.0;

        for k in i..j {
            if combined[k].1 {
                rank_sum_a += avg_rank;
            }
        }

        let t = (j - i) as f64;
        tie_correction_sum += t * t * t - t;
        i = j;
    }

    let u1 = rank_sum_a - n1 * (n1 + 1.0) / 2.0;
    let u2 = n1 * n2 - u1;
    let u = u1.min(u2);

    let mu = n1 * n2 / 2.0;
    let tie_correction = if n_total > 1.0 {
        1.0 - tie_correction_sum / (n_total * n_total * n_total - n_total)
    } else {
        1.0
    };
    let sigma_sq = (n1 * n2 * (n_total + 1.0) / 12.0) * tie_correction.max(0.0);
    if sigma_sq <= 0.0 {
        return Some(1.0);
    }

    let sigma = sigma_sq.sqrt();
    let correction = if u > mu { -0.5 } else { 0.5 };
    let z = (u - mu + correction) / sigma;
    let p = 2.0 * (1.0 - normal_cdf(z.abs()));
    Some(p.clamp(0.0, 1.0))
}

pub fn metric_significance(
    metric: &str,
    baseline: &[f64],
    candidate: &[f64],
    seed: u64,
) -> MetricSignificance {
    let baseline_mean = if baseline.is_empty() {
        0.0
    } else {
        baseline.iter().sum::<f64>() / baseline.len() as f64
    };
    let candidate_mean = if candidate.is_empty() {
        0.0
    } else {
        candidate.iter().sum::<f64>() / candidate.len() as f64
    };

    MetricSignificance {
        metric: metric.to_string(),
        n_baseline: baseline.len(),
        n_candidate: candidate.len(),
        effect_mean_delta: candidate_mean - baseline_mean,
        bootstrap_ci95: bootstrap_mean_diff_ci(baseline, candidate, 2000, seed),
        mann_whitney_p: mann_whitney_u_pvalue(baseline, candidate),
    }
}

fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let qq = q.clamp(0.0, 1.0);
    let pos = qq * (sorted.len().saturating_sub(1) as f64);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let w = pos - lo as f64;
        sorted[lo] * (1.0 - w) + sorted[hi] * w
    }
}

fn bootstrap_mean(samples: &[f64], rng: &mut SplitMix64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let mut sum = 0.0;
    for _ in 0..samples.len() {
        let idx = (rng.next_u64() as usize) % samples.len();
        sum += samples[idx];
    }
    sum / samples.len() as f64
}

#[derive(Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    // Abramowitz and Stegun 7.1.26
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();

    let t = 1.0 / (1.0 + 0.327_591_1 * ax);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-ax * ax).exp());

    sign * y
}

/// Lightweight timestamp (no chrono dependency).
fn chrono_lite_now() -> String {
    // Use a fixed format for reproducibility in CI.
    // In real usage this would use std::time::SystemTime.
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s-since-epoch", d.as_secs())
}

// Legacy compat
pub type ABComparison = ABReport;

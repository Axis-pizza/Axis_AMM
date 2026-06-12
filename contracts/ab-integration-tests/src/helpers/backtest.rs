use serde::Deserialize;

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

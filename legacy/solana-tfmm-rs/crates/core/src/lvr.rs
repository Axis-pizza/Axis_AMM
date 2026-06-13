use crate::types::{LvrInput2, LvrResult, PricePoint, RvrCostConfig};
use ndarray::Array1;
use thiserror::Error;

const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 60.0 * 60.0;

#[derive(Debug, Error)]
pub enum LvrError {
    #[error("price series must contain at least 2 points")]
    TooFewPoints,
    #[error("price must be positive")]
    NonPositivePrice,
    #[error("dt_seconds must be > 0")]
    InvalidDt,
    #[error("weights must be in [0,1] and sum to 1 (within tolerance)")]
    InvalidWeights,
}

/// log-return series を計算
pub fn log_returns(points: &[PricePoint]) -> Result<Vec<f64>, LvrError> {
    if points.len() < 2 {
        return Err(LvrError::TooFewPoints);
    }

    let mut out = Vec::with_capacity(points.len() - 1);
    for w in points.windows(2) {
        let p0 = w[0].price;
        let p1 = w[1].price;
        if p0 <= 0.0 || p1 <= 0.0 {
            return Err(LvrError::NonPositivePrice);
        }
        out.push((p1 / p0).ln());
    }
    Ok(out)
}

/// 不偏分散ではなく、ここでは母分散ベース（平均からの二乗平均）
pub fn variance(xs: &[f64]) -> Result<f64, LvrError> {
    if xs.is_empty() {
        return Err(LvrError::TooFewPoints);
    }
    let arr = Array1::from_vec(xs.to_vec());
    let mean = arr.sum() / arr.len() as f64;
    let var = arr.mapv(|x| {
        let d = x - mean;
        d * d
    });
    Ok(var.sum() / arr.len() as f64)
}

/// サンプル間隔 dt_seconds の実現分散から年率分散へ変換
pub fn annualize_variance(var_per_step: f64, dt_seconds: f64) -> Result<f64, LvrError> {
    if dt_seconds <= 0.0 {
        return Err(LvrError::InvalidDt);
    }
    Ok(var_per_step * (SECONDS_PER_YEAR / dt_seconds))
}

/// PricePoint系列から年率分散を直接計算
pub fn realized_annual_variance(points: &[PricePoint], dt_seconds: f64) -> Result<f64, LvrError> {
    let rets = log_returns(points)?;
    let var_step = variance(&rets)?;
    annualize_variance(var_step, dt_seconds)
}

/// 2資産LVR（簡略）
///
/// 研究用途の最初の近似として:
///   LVR_step ≈ 0.5 * w_x * w_y * sigma^2_annual * (dt / year) * TVL
///
/// sigma^2_annual は年率分散
pub fn compute_lvr_2asset(input: LvrInput2) -> Result<LvrResult, LvrError> {
    if input.dt_seconds <= 0.0 {
        return Err(LvrError::InvalidDt);
    }

    let sum_w = input.weight_x + input.weight_y;
    let weights_ok = input.weight_x >= 0.0
        && input.weight_y >= 0.0
        && (sum_w - 1.0).abs() < 1e-9;

    if !weights_ok {
        return Err(LvrError::InvalidWeights);
    }

    let dt_years = input.dt_seconds / SECONDS_PER_YEAR;

    let lvr_step_usd =
        0.5 * input.weight_x * input.weight_y * input.variance_annual * dt_years * input.tvl_usd;

    let steps_per_year = SECONDS_PER_YEAR / input.dt_seconds;
    let lvr_annual_usd = lvr_step_usd * steps_per_year;
    let lvr_annual_ratio = if input.tvl_usd > 0.0 {
        lvr_annual_usd / input.tvl_usd
    } else {
        0.0
    };

    Ok(LvrResult {
        lvr_step_usd,
        lvr_annual_usd,
        lvr_annual_ratio,
    })
}

/// RVR（ここでは現実的なCEXリバランスコスト近似）
///
/// 返り値は「実行コストUSD」
/// （論文のRVRそのものの定義に完全一致ではなく、比較用ベンチマーク成分）
pub fn compute_rvr_cost_usd(trade_size_usd: f64, cfg: RvrCostConfig) -> f64 {
    let total_bps = cfg.taker_fee_bps + cfg.slippage_bps;
    trade_size_usd * total_bps / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(ts: i64, p: f64) -> PricePoint {
        PricePoint { ts_unix: ts, price: p }
    }

    #[test]
    fn log_returns_basic() {
        let points = vec![pp(0, 100.0), pp(1, 110.0)];
        let r = log_returns(&points).unwrap();
        assert_eq!(r.len(), 1);
        assert!((r[0] - (1.1_f64).ln()).abs() < 1e-12);
    }

    #[test]
    fn variance_zero_for_constant_returns() {
        let xs = vec![0.1, 0.1, 0.1];
        let v = variance(&xs).unwrap();
        assert!(v.abs() < 1e-12);
    }

    #[test]
    fn annualize_variance_scales_up() {
        let v = annualize_variance(1e-8, 60.0).unwrap();
        assert!(v > 0.0);
    }

    #[test]
    fn compute_lvr_2asset_positive() {
        let out = compute_lvr_2asset(LvrInput2 {
            weight_x: 0.5,
            weight_y: 0.5,
            variance_annual: 0.64, // sigma=80%
            tvl_usd: 100_000.0,
            dt_seconds: 60.0,
        })
        .unwrap();

        assert!(out.lvr_step_usd > 0.0);
        assert!(out.lvr_annual_usd > 0.0);
        assert!(out.lvr_annual_ratio > 0.0);
    }

    #[test]
    fn lvr_annual_ratio_matches_formula() {
        let out = compute_lvr_2asset(LvrInput2 {
            weight_x: 0.5,
            weight_y: 0.5,
            variance_annual: 0.64,
            tvl_usd: 100_000.0,
            dt_seconds: 1.0,
        })
        .unwrap();

        // 年率比は dtに依存せず 0.5 * wx * wy * sigma2 = 0.5 * 0.25 * 0.64 = 0.08
        assert!((out.lvr_annual_ratio - 0.08).abs() < 1e-9);
    }

    #[test]
    fn rvr_cost_basic() {
        let cost = compute_rvr_cost_usd(
            10_000.0,
            RvrCostConfig {
                taker_fee_bps: 10.0,
                slippage_bps: 3.0,
            },
        );
        assert!((cost - 13.0).abs() < 1e-12);
    }

    #[test]
    fn realized_annual_variance_runs() {
        let points = vec![
            pp(0, 100.0),
            pp(60, 101.0),
            pp(120, 99.5),
            pp(180, 100.2),
            pp(240, 100.8),
        ];
        let v = realized_annual_variance(&points, 60.0).unwrap();
        assert!(v >= 0.0);
    }
}
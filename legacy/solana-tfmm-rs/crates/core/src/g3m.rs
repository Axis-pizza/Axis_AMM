use crate::types::{DriftMetrics, PoolSnapshot2};

/// 2資産G3M/CPMM系の研究用ユーティリティ
pub struct G3m2;

impl G3m2 {
    /// プールのスポット価格 (y per x)
    ///
    /// 厳密なG3Mのスポット価格は重みを含む形になるが、
    /// ここではまず観測用として reserve ratio ベースを提供。
    pub fn spot_price_reserve_ratio(reserve_x: f64, reserve_y: f64) -> f64 {
        reserve_y / reserve_x
    }

    /// 重み付きG3Mの理論スポット価格（2資産）
    /// p = (w_x / w_y) * (R_y / R_x) を y per x として表現
    pub fn spot_price_weighted(
        reserve_x: f64,
        reserve_y: f64,
        w_x: f64,
        w_y: f64,
    ) -> f64 {
        (w_x / w_y) * (reserve_y / reserve_x)
    }

    /// 実際の価値配分 θ を計算（2資産）
    /// external_price_y_per_x = y per x
    ///
    /// x資産の価値を y建てに換算:
    /// value_x_in_y = reserve_x * external_price_y_per_x
    /// value_y_in_y = reserve_y
    pub fn actual_weights_from_external_price(
        reserve_x: f64,
        reserve_y: f64,
        external_price_y_per_x: f64,
    ) -> (f64, f64) {
        let value_x = reserve_x * external_price_y_per_x;
        let value_y = reserve_y;
        let total = value_x + value_y;

        let theta_x = value_x / total;
        let theta_y = value_y / total;
        (theta_x, theta_y)
    }

    /// Σ|θ_i - w_i| を計算（2資産）
    pub fn drift_metrics(
        reserve_x: f64,
        reserve_y: f64,
        target_w_x: f64,
        target_w_y: f64,
        external_price_y_per_x: f64,
    ) -> DriftMetrics {
        let (theta_x, theta_y) =
            Self::actual_weights_from_external_price(reserve_x, reserve_y, external_price_y_per_x);

        let drift = (theta_x - target_w_x).abs() + (theta_y - target_w_y).abs();

        DriftMetrics {
            abs_weight_drift_sum: drift,
            actual_w_x: theta_x,
            actual_w_y: theta_y,
        }
    }

    pub fn drift_metrics_from_snapshot(
        snap: &PoolSnapshot2,
        external_price_y_per_x: f64,
    ) -> DriftMetrics {
        Self::drift_metrics(
            snap.reserve_x,
            snap.reserve_y,
            snap.target_w_x,
            snap.target_w_y,
            external_price_y_per_x,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_ratio_price_basic() {
        let p = G3m2::spot_price_reserve_ratio(10.0, 20.0);
        assert!((p - 2.0).abs() < 1e-12);
    }

    #[test]
    fn weighted_spot_price_reduces_to_ratio_when_equal_weights() {
        let p = G3m2::spot_price_weighted(10.0, 20.0, 0.5, 0.5);
        assert!((p - 2.0).abs() < 1e-12);
    }

    #[test]
    fn actual_weights_sum_to_one() {
        let (wx, wy) = G3m2::actual_weights_from_external_price(10.0, 20.0, 2.0);
        assert!(((wx + wy) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn drift_zero_when_actual_matches_target() {
        // reserve_x=10, reserve_y=20, external_price=2 => x価値=20, y価値=20 => 50/50
        let d = G3m2::drift_metrics(10.0, 20.0, 0.5, 0.5, 2.0);
        assert!(d.abs_weight_drift_sum < 1e-12);
    }

    #[test]
    fn drift_positive_when_target_differs() {
        let d = G3m2::drift_metrics(10.0, 20.0, 0.7, 0.3, 2.0);
        assert!(d.abs_weight_drift_sum > 0.0);
    }
}
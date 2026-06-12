use serde::{Deserialize, Serialize};

/// 単一時点の価格データ（例: SOL/USD）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PricePoint {
    pub ts_unix: i64,
    pub price: f64,
}

/// 2資産プールの状態スナップショット（研究用最小）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PoolSnapshot2 {
    pub ts_unix: i64,
    pub reserve_x: f64,
    pub reserve_y: f64,
    /// ターゲットウェイト（x側）
    pub target_w_x: f64,
    /// ターゲットウェイト（y側）
    pub target_w_y: f64,
}

/// 2資産の動的ウェイトスケジュール（線形補間）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeightSchedule2 {
    pub start_ts_unix: i64,
    pub end_ts_unix: i64,
    pub start_w_x: f64,
    pub end_w_x: f64,
}

impl WeightSchedule2 {
    pub fn weights_at(&self, ts_unix: i64) -> (f64, f64) {
        if ts_unix <= self.start_ts_unix {
            let wx = self.start_w_x;
            return (wx, 1.0 - wx);
        }
        if ts_unix >= self.end_ts_unix {
            let wx = self.end_w_x;
            return (wx, 1.0 - wx);
        }

        let total = (self.end_ts_unix - self.start_ts_unix) as f64;
        let elapsed = (ts_unix - self.start_ts_unix) as f64;
        let t = elapsed / total;

        let wx = self.start_w_x + (self.end_w_x - self.start_w_x) * t;
        (wx, 1.0 - wx)
    }
}

/// LVR計算の入力（2資産簡略）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LvrInput2 {
    pub weight_x: f64,
    pub weight_y: f64,
    /// 年率分散（log-return variance annualized）
    pub variance_annual: f64,
    /// 評価対象のTVL（USD）
    pub tvl_usd: f64,
    /// 時間刻み（秒）
    pub dt_seconds: f64,
}

/// LVRの計算結果
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LvrResult {
    /// 1ステップあたりのLVR（USD）
    pub lvr_step_usd: f64,
    /// 年率換算LVR（USD/年）
    pub lvr_annual_usd: f64,
    /// TVL比（年率, 例 0.08 = 8%）
    pub lvr_annual_ratio: f64,
}

/// RVRコストの設定（CEX実行コスト近似）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RvrCostConfig {
    /// taker fee（bps）
    pub taker_fee_bps: f64,
    /// スプレッド/スリッページ（bps）
    pub slippage_bps: f64,
}

/// 裁定判定の入力（2資産）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArbCheckInput {
    /// プール価格 (y per x)
    pub pool_price: f64,
    /// 外部市場価格 (y per x)
    pub external_price: f64,
    /// 取引サイズ（x建て）
    pub trade_size_x: f64,
    /// 取引コスト（USD）
    pub total_cost_usd: f64,
    /// xの外部価格（USD換算用）
    pub x_price_usd: f64,
}

/// 裁定判定の結果
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArbCheckResult {
    pub gross_profit_usd: f64,
    pub net_profit_usd: f64,
    pub profitable: bool,
}

/// TFMMでよく見る配分ドリフト（2資産では簡略可）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DriftMetrics {
    /// Σ|θ_i - w_i| （2資産）
    pub abs_weight_drift_sum: f64,
    /// 実際の価値配分（x側）
    pub actual_w_x: f64,
    pub actual_w_y: f64,
}
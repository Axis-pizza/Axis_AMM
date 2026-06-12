use crate::types::{ArbCheckInput, ArbCheckResult};

/// 研究用の簡易裁定判定ロジック
pub struct ArbEngine;

impl ArbEngine {
    /// gross profit を概算（USD）
    ///
    /// 直感的近似:
    ///   edge = |pool - ext| / ext
    ///   notional_usd = trade_size_x * x_price_usd
    ///   gross ≈ edge * notional_usd
    pub fn estimate_gross_profit_usd(input: ArbCheckInput) -> f64 {
        let edge = ((input.pool_price - input.external_price) / input.external_price).abs();
        let notional_usd = input.trade_size_x * input.x_price_usd;
        edge * notional_usd
    }

    pub fn check_profitability(input: ArbCheckInput) -> ArbCheckResult {
        let gross = Self::estimate_gross_profit_usd(input);
        let net = gross - input.total_cost_usd;

        ArbCheckResult {
            gross_profit_usd: gross,
            net_profit_usd: net,
            profitable: net > 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profitability_positive_when_edge_big_enough() {
        let input = ArbCheckInput {
            pool_price: 101.0,
            external_price: 100.0,
            trade_size_x: 1.0,
            total_cost_usd: 0.2,
            x_price_usd: 100.0,
        };

        let out = ArbEngine::check_profitability(input);
        assert!(out.gross_profit_usd > 0.0);
        assert!(out.profitable);
    }

    #[test]
    fn profitability_negative_when_cost_too_high() {
        let input = ArbCheckInput {
            pool_price: 100.2,
            external_price: 100.0,
            trade_size_x: 1.0,
            total_cost_usd: 1.0,
            x_price_usd: 100.0,
        };

        let out = ArbEngine::check_profitability(input);
        assert!(!out.profitable);
        assert!(out.net_profit_usd < 0.0);
    }
}
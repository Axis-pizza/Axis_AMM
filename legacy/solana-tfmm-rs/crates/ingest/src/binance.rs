use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Binance Spot aggTrade（最小必要フィールド）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceAggTrade {
    #[serde(rename = "a")]
    pub agg_trade_id: u64,
    #[serde(rename = "p")]
    pub price: String, // stringで来る
    #[serde(rename = "q")]
    pub qty: String,   // stringで来る
    #[serde(rename = "f")]
    pub first_trade_id: u64,
    #[serde(rename = "l")]
    pub last_trade_id: u64,
    #[serde(rename = "T")]
    pub timestamp_ms: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
    #[serde(rename = "M")]
    pub is_best_match: bool,
}

impl BinanceAggTrade {
    pub fn price_f64(&self) -> Result<f64> {
        self.price
            .parse::<f64>()
            .with_context(|| format!("failed to parse Binance price: {}", self.price))
    }

    pub fn qty_f64(&self) -> Result<f64> {
        self.qty
            .parse::<f64>()
            .with_context(|| format!("failed to parse Binance qty: {}", self.qty))
    }
}

#[derive(Debug, Clone)]
pub struct BinanceClient {
    http: Client,
    base_url: String,
}

impl BinanceClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .context("failed to build Binance reqwest client")?;

        Ok(Self {
            http,
            base_url: "https://api.binance.com".to_string(),
        })
    }

    /// aggTrades を時間範囲で取得（最大1000件）
    ///
    /// symbol例: "SOLUSDC"
    pub fn get_agg_trades(
        &self,
        symbol: &str,
        start_time_ms: i64,
        end_time_ms: i64,
        limit: usize,
    ) -> Result<Vec<BinanceAggTrade>> {
        let limit = limit.clamp(1, 1000);

        let url = format!(
            "{}/api/v3/aggTrades?symbol={}&startTime={}&endTime={}&limit={}",
            self.base_url, symbol, start_time_ms, end_time_ms, limit
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("Binance aggTrades request failed: {url}"))?;

        let status = resp.status();
        let text = resp.text().context("failed to read Binance response body")?;

        if !status.is_success() {
            return Err(anyhow!(
                "Binance API error: status={} body={}",
                status,
                truncate(&text, 400)
            ));
        }

        let trades: Vec<BinanceAggTrade> =
            serde_json::from_str(&text).context("failed to parse Binance aggTrades JSON")?;

        Ok(trades)
    }

    /// 指定レンジを複数回叩いて連結（1000件超え対策）
    ///
    /// 安全重視で最大ページ数を制限。
    pub fn get_agg_trades_range_paged(
        &self,
        symbol: &str,
        start_time_ms: i64,
        end_time_ms: i64,
        per_page_limit: usize,
        max_pages: usize,
    ) -> Result<Vec<BinanceAggTrade>> {
        if end_time_ms < start_time_ms {
            return Err(anyhow!("end_time_ms < start_time_ms"));
        }

        let mut all = Vec::<BinanceAggTrade>::new();
        let mut current_start = start_time_ms;
        let mut pages = 0usize;

        while current_start <= end_time_ms && pages < max_pages {
            pages += 1;

            let batch = self.get_agg_trades(symbol, current_start, end_time_ms, per_page_limit)?;
            if batch.is_empty() {
                break;
            }

            let last_ts = batch.last().map(|t| t.timestamp_ms).unwrap_or(current_start);

            // 重複を避けつつ追加
            if all.is_empty() {
                all.extend(batch);
            } else {
                let last_seen_id = all.last().map(|t| t.agg_trade_id);
                for t in batch {
                    if Some(t.agg_trade_id) != last_seen_id {
                        all.push(t);
                    }
                }
            }

            // 同じtimestampが続くケースに備えて +1ms
            if last_ts >= current_start {
                current_start = last_ts + 1;
            } else {
                break;
            }
        }

        // timestamp順にソート（念のため）
        all.sort_by_key(|t| t.timestamp_ms);

        Ok(all)
    }
}

/// 外部価格マッチ結果（trade単位）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedExternalPrice {
    pub target_ts_ms: i64,
    pub matched_ts_ms: i64,
    pub price_usdc_per_sol: f64,
    pub abs_time_diff_ms: i64,
}

/// 最近傍のaggTrade価格をマッチ
pub fn nearest_price_match(
    trades: &[BinanceAggTrade],
    target_ts_ms: i64,
) -> Option<MatchedExternalPrice> {
    if trades.is_empty() {
        return None;
    }

    // 線形探索（100~数千件なら十分）
    let mut best_idx = 0usize;
    let mut best_diff = (trades[0].timestamp_ms - target_ts_ms).abs();

    for (i, t) in trades.iter().enumerate().skip(1) {
        let d = (t.timestamp_ms - target_ts_ms).abs();
        if d < best_diff {
            best_diff = d;
            best_idx = i;
        }
    }

    let t = &trades[best_idx];
    let price = t.price_f64().ok()?;

    Some(MatchedExternalPrice {
        target_ts_ms,
        matched_ts_ms: t.timestamp_ms,
        price_usdc_per_sol: price,
        abs_time_diff_ms: best_diff,
    })
}

/// bps差分
pub fn edge_bps(exec_price: f64, ext_price: f64) -> Option<f64> {
    if exec_price <= 0.0 || ext_price <= 0.0 {
        return None;
    }
    Some(((exec_price - ext_price).abs() / ext_price) * 10_000.0)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_bps_basic() {
        let e = edge_bps(101.0, 100.0).unwrap();
        assert!((e - 100.0).abs() < 1e-12); // 1% = 100bps
    }

    #[test]
    fn nearest_price_match_basic() {
        let xs = vec![
            BinanceAggTrade {
                agg_trade_id: 1,
                price: "86.0".into(),
                qty: "1".into(),
                first_trade_id: 1,
                last_trade_id: 1,
                timestamp_ms: 1000,
                is_buyer_maker: false,
                is_best_match: true,
            },
            BinanceAggTrade {
                agg_trade_id: 2,
                price: "87.0".into(),
                qty: "1".into(),
                first_trade_id: 2,
                last_trade_id: 2,
                timestamp_ms: 2000,
                is_buyer_maker: false,
                is_best_match: true,
            },
        ];

        let m = nearest_price_match(&xs, 1800).unwrap();
        assert_eq!(m.matched_ts_ms, 2000);
        assert!((m.price_usdc_per_sol - 87.0).abs() < 1e-12);
    }
}
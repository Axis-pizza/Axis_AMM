use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Coinbase Exchange public trades API response item (older API format)
/// https://api.exchange.coinbase.com/products/{product_id}/trades
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTrade {
    pub trade_id: i64,
    pub price: String,
    pub size: String,
    pub time: String, // ISO8601
    pub side: String, // buy/sell
}

impl CoinbaseTrade {
    pub fn price_f64(&self) -> Result<f64> {
        self.price
            .parse::<f64>()
            .with_context(|| format!("failed to parse Coinbase price: {}", self.price))
    }

    pub fn size_f64(&self) -> Result<f64> {
        self.size
            .parse::<f64>()
            .with_context(|| format!("failed to parse Coinbase size: {}", self.size))
    }

    pub fn timestamp_ms(&self) -> Result<i64> {
        // RFC3339 parse
        let dt = chrono::DateTime::parse_from_rfc3339(&self.time)
            .with_context(|| format!("failed to parse Coinbase trade time: {}", self.time))?;
        Ok(dt.timestamp_millis())
    }
}

#[derive(Debug, Clone)]
pub struct CoinbaseClient {
    http: Client,
    base_url: String,
}

impl CoinbaseClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("solana-tfmm-rs/0.1")
            .build()
            .context("failed to build Coinbase reqwest client")?;

        Ok(Self {
            http,
            base_url: "https://api.exchange.coinbase.com".to_string(),
        })
    }

    /// 1ページ取得（最大1000件）
    /// Coinbase trades API は新しいものから返る。CB-AFTER ヘッダでページング。
    pub fn get_trades_page(
        &self,
        product_id: &str,          // "SOL-USD"
        limit: usize,              // 1..=1000
        after: Option<&str>,       // pagination cursor
    ) -> Result<(Vec<CoinbaseTrade>, Option<String>)> {
        let limit = limit.clamp(1, 1000);

        let url = format!("{}/products/{}/trades?limit={}", self.base_url, product_id, limit);
        let mut req = self.http.get(&url);

        if let Some(after_cursor) = after {
            req = req.header("CB-AFTER", after_cursor);
        }

        let resp = req
            .send()
            .with_context(|| format!("Coinbase trades request failed: {url}"))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().context("failed to read Coinbase response body")?;

        if !status.is_success() {
            return Err(anyhow!(
                "Coinbase API error: status={} body={}",
                status,
                truncate(&text, 400)
            ));
        }

        let trades: Vec<CoinbaseTrade> =
            serde_json::from_str(&text).context("failed to parse Coinbase trades JSON")?;

        // 次ページ用カーソル（あれば）
        let next_after = headers
            .get("cb-after")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok((trades, next_after))
    }

    /// 時間レンジをカバーするまで複数ページ取得（ざっくり）
    ///
    /// 注意:
    /// - trades API は新しい順
    /// - レンジを完全保証するものではなく、まずは十分な coverage を狙う
    pub fn get_trades_covering_range(
        &self,
        product_id: &str,
        target_start_ms: i64,
        target_end_ms: i64,
        per_page_limit: usize,
        max_pages: usize,
    ) -> Result<Vec<CoinbaseTrade>> {
        if target_end_ms < target_start_ms {
            return Err(anyhow!("target_end_ms < target_start_ms"));
        }

        let mut all = Vec::<CoinbaseTrade>::new();
        let mut after: Option<String> = None;

        for _page in 0..max_pages {
            let (batch, next_after) =
                self.get_trades_page(product_id, per_page_limit, after.as_deref())?;

            if batch.is_empty() {
                break;
            }

            // 追加
            all.extend(batch);

            // いま集まった trade の最古時刻が target_start_ms より古ければ十分
            let mut min_ts_seen = i64::MAX;
            for t in &all {
                if let Ok(ts) = t.timestamp_ms() {
                    if ts < min_ts_seen {
                        min_ts_seen = ts;
                    }
                }
            }

            if min_ts_seen <= target_start_ms {
                break;
            }

            after = next_after;
            if after.is_none() {
                break;
            }
        }

        // timestamp 取得できるものだけ残す
        let mut filtered: Vec<CoinbaseTrade> = all
            .into_iter()
            .filter(|t| t.timestamp_ms().is_ok())
            .collect();

        // 時刻で昇順ソート
        filtered.sort_by_key(|t| t.timestamp_ms().unwrap_or(i64::MIN));

        // targetレンジに少し余裕を持って絞る（±5s）
        let pad_ms = 5_000_i64;
        let start = target_start_ms - pad_ms;
        let end = target_end_ms + pad_ms;

        filtered.retain(|t| {
            if let Ok(ts) = t.timestamp_ms() {
                ts >= start && ts <= end
            } else {
                false
            }
        });

        Ok(filtered)
    }
}

/// 外部価格マッチ結果（trade単位）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedExternalPrice {
    pub target_ts_ms: i64,
    pub matched_ts_ms: i64,
    pub price_usd_per_sol: f64,
    pub abs_time_diff_ms: i64,
}

pub fn nearest_price_match(
    trades: &[CoinbaseTrade],
    target_ts_ms: i64,
) -> Option<MatchedExternalPrice> {
    if trades.is_empty() {
        return None;
    }

    let mut best_idx = None::<usize>;
    let mut best_diff = i64::MAX;

    for (i, t) in trades.iter().enumerate() {
        let ts = t.timestamp_ms().ok()?;
        let d = (ts - target_ts_ms).abs();
        if d < best_diff {
            best_diff = d;
            best_idx = Some(i);
        }
    }

    let idx = best_idx?;
    let t = &trades[idx];
    let ts = t.timestamp_ms().ok()?;
    let price = t.price_f64().ok()?;

    Some(MatchedExternalPrice {
        target_ts_ms,
        matched_ts_ms: ts,
        price_usd_per_sol: price,
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
}
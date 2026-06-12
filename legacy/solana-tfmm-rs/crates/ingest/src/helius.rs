use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::time::Duration;

/// Helius address transaction API の最小クライアント（REST / blocking）
#[derive(Debug, Clone)]
pub struct HeliusClient {
    api_key: String,
    http: Client,
    base_url: String,
}

impl HeliusClient {
    pub fn from_env() -> Result<Self> {
        let _ = dotenv::dotenv();

        let api_key = env::var("HELIUS_API_KEY")
            .context("HELIUS_API_KEY が未設定です (.env または環境変数)")?;

        if api_key.trim().is_empty() || api_key == "YOUR_HELIUS_API_KEY" {
            return Err(anyhow!(
                "HELIUS_API_KEY がプレースホルダーです。実キーを設定してください"
            ));
        }

        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .context("reqwest blocking client の初期化に失敗")?;

        Ok(Self {
            api_key,
            http,
            base_url: "https://api.helius.xyz/v0".to_string(),
        })
    }

    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .context("reqwest blocking client の初期化に失敗")?;

        Ok(Self {
            api_key: api_key.into(),
            http,
            base_url: "https://api.helius.xyz/v0".to_string(),
        })
    }

    pub fn get_address_transactions(
        &self,
        address: &str,
        limit: usize,
        tx_type: Option<&str>,
        before: Option<&str>,
    ) -> Result<Vec<Value>> {
        let mut url = format!("{}/addresses/{}/transactions", self.base_url, address);

        let mut qs = vec![
            format!("api-key={}", self.api_key),
            format!("limit={}", limit),
        ];

        if let Some(t) = tx_type {
            qs.push(format!("type={}", t));
        }
        if let Some(b) = before {
            qs.push(format!("before={}", b));
        }

        url.push('?');
        url.push_str(&qs.join("&"));

        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("Helius API リクエスト失敗: {url}"))?;

        let status = resp.status();
        let text = resp.text().context("Heliusレスポンス本文の取得に失敗")?;

        if !status.is_success() {
            return Err(anyhow!(
                "Helius API error: status={} body={}",
                status,
                truncate(&text, 500)
            ));
        }

        let value: Value =
            serde_json::from_str(&text).context("HeliusレスポンスJSONのパースに失敗")?;

        let arr = value
            .as_array()
            .ok_or_else(|| anyhow!("Heliusレスポンスが配列ではありません: {}", truncate(&text, 500)))?;

        Ok(arr.clone())
    }
}

/// 実際に使いやすいように抽出した最小 tx プレビュー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTxPreview {
    pub signature: Option<String>,
    pub slot: Option<u64>,
    pub timestamp: Option<i64>,
    pub tx_type: Option<String>,
    pub source: Option<String>,
    pub fee: Option<u64>,
    pub description: Option<String>,
}

pub fn extract_tx_preview(v: &Value) -> HeliusTxPreview {
    let signature = v.get("signature").and_then(|x| x.as_str()).map(|s| s.to_string());
    let slot = v.get("slot").and_then(|x| x.as_u64());
    let timestamp = v.get("timestamp").and_then(|x| x.as_i64());
    let tx_type = v.get("type").and_then(|x| x.as_str()).map(|s| s.to_string());
    let source = v.get("source").and_then(|x| x.as_str()).map(|s| s.to_string());
    let fee = v.get("fee").and_then(|x| x.as_u64());
    let description = v
        .get("description")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    HeliusTxPreview {
        signature,
        slot,
        timestamp,
        tx_type,
        source,
        fee,
        description,
    }
}

/// Slot単位の件数集計（txベース）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotCount {
    pub slot: u64,
    pub tx_count: usize,
}

pub fn aggregate_tx_counts_by_slot(values: &[Value]) -> Vec<SlotCount> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<u64, usize> = BTreeMap::new();
    for v in values {
        if let Some(slot) = v.get("slot").and_then(|x| x.as_u64()) {
            *map.entry(slot).or_insert(0) += 1;
        }
    }

    map.into_iter()
        .map(|(slot, tx_count)| SlotCount { slot, tx_count })
        .collect()
}

// =========================
// Description parser (Phase 3.5)
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSwapDescription {
    pub amount_in: f64,
    pub token_in: String,
    pub amount_out: f64,
    pub token_out: String,
}

/// tx + description parse をまとめた trade preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapTradePreview {
    pub signature: Option<String>,
    pub slot: u64,
    pub timestamp: Option<i64>,
    pub fee: Option<u64>,
    pub source: Option<String>,

    pub amount_in: f64,
    pub token_in: String,
    pub amount_out: f64,
    pub token_out: String,

    /// token_in per token_out（向き依存）
    pub execution_price_in_per_out: f64,

    /// USDC 建て notional（USDCが含まれる場合のみ）
    pub notional_usdc: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSwapAggregate {
    pub slot: u64,
    pub swap_count: usize,

    pub mean_exec_price_in_per_out: f64,
    pub min_exec_price_in_per_out: f64,
    pub max_exec_price_in_per_out: f64,

    pub price_range_bps: f64,

    pub total_notional_usdc: f64,
    pub notional_count: usize,
}

pub fn parse_swap_description(desc: &str) -> Option<ParsedSwapDescription> {
    let swapped_pos = desc.find(" swapped ")?;
    let tail = &desc[swapped_pos + " swapped ".len()..];

    let parts: Vec<&str> = tail.split_whitespace().collect();

    // 想定: "<amount_in> <token_in> for <amount_out> <token_out>"
    if parts.len() < 5 {
        return None;
    }

    let amount_in = parts[0].replace(',', "").parse::<f64>().ok()?;
    let token_in = parts[1].to_string();

    if parts[2] != "for" {
        return None;
    }

    let amount_out = parts[3].replace(',', "").parse::<f64>().ok()?;
    let token_out = parts[4].to_string();

    if amount_in <= 0.0 || amount_out <= 0.0 {
        return None;
    }

    Some(ParsedSwapDescription {
        amount_in,
        token_in,
        amount_out,
        token_out,
    })
}

pub fn extract_swap_trade_preview(v: &Value) -> Option<SwapTradePreview> {
    let base = extract_tx_preview(v);

    let slot = base.slot?;
    let desc = base.description.as_deref()?;
    let parsed = parse_swap_description(desc)?;

    let execution_price_in_per_out = parsed.amount_in / parsed.amount_out;

    let notional_usdc = if parsed.token_in.eq_ignore_ascii_case("USDC") {
        Some(parsed.amount_in)
    } else if parsed.token_out.eq_ignore_ascii_case("USDC") {
        Some(parsed.amount_out)
    } else {
        None
    };

    Some(SwapTradePreview {
        signature: base.signature,
        slot,
        timestamp: base.timestamp,
        fee: base.fee,
        source: base.source,

        amount_in: parsed.amount_in,
        token_in: parsed.token_in,
        amount_out: parsed.amount_out,
        token_out: parsed.token_out,

        execution_price_in_per_out,
        notional_usdc,
    })
}

pub fn extract_swap_trade_previews(values: &[Value]) -> Vec<SwapTradePreview> {
    values.iter().filter_map(extract_swap_trade_preview).collect()
}

pub fn aggregate_swaps_by_slot(trades: &[SwapTradePreview]) -> Vec<SlotSwapAggregate> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<u64, Vec<&SwapTradePreview>> = BTreeMap::new();
    for t in trades {
        map.entry(t.slot).or_default().push(t);
    }

    let mut out = Vec::with_capacity(map.len());

    for (slot, ts) in map {
        let swap_count = ts.len();
        let prices: Vec<f64> = ts.iter().map(|t| t.execution_price_in_per_out).collect();

        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        let min = prices.iter().copied().fold(f64::INFINITY, |a, b| a.min(b));
        let max = prices.iter().copied().fold(f64::NEG_INFINITY, |a, b| a.max(b));

        let price_range_bps = if mean > 0.0 {
            (max - min).abs() / mean * 10_000.0
        } else {
            0.0
        };

        let mut total_notional_usdc = 0.0_f64;
        let mut notional_count = 0_usize;
        for t in ts {
            if let Some(n) = t.notional_usdc {
                total_notional_usdc += n;
                notional_count += 1;
            }
        }

        out.push(SlotSwapAggregate {
            slot,
            swap_count,
            mean_exec_price_in_per_out: mean,
            min_exec_price_in_per_out: min,
            max_exec_price_in_per_out: max,
            price_range_bps,
            total_notional_usdc,
            notional_count,
        });
    }

    out
}

// =========================
// Phase 3.6: SOL/USDC 正規化
// =========================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairNormalizationStatus {
    SolUsdcNormalized,
    NotSolUsdcPair,
    InvalidAmount,
}

/// SOL/USDC のみを対象に、価格を USDC per SOL に正規化した trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedSwapTrade {
    pub signature: Option<String>,
    pub slot: u64,
    pub timestamp: Option<i64>,
    pub fee: Option<u64>,
    pub source: Option<String>,

    pub amount_in: f64,
    pub token_in: String,
    pub amount_out: f64,
    pub token_out: String,

    /// 常に USDC / SOL
    pub exec_price_usdc_per_sol: f64,

    /// USDC 建て notional（近似）
    pub notional_usdc: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizationReport {
    pub total_trades: usize,
    pub normalized_sol_usdc: usize,
    pub dropped_not_sol_usdc_pair: usize,
    pub dropped_invalid_amount: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedSlotSwapAggregate {
    pub slot: u64,
    pub swap_count: usize,

    /// 全て USDC/SOL に統一済み
    pub mean_price_usdc_per_sol: f64,
    pub min_price_usdc_per_sol: f64,
    pub max_price_usdc_per_sol: f64,
    pub price_range_bps: f64,

    pub total_notional_usdc: f64,
    pub mean_notional_usdc: f64,

    /// 参考: fee集計（取れるものだけ）
    pub fee_count: usize,
    pub mean_fee_lamports: Option<f64>,
}

fn is_token_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// SOL/USDC だけに絞って、価格を常に USDC per SOL に揃える
pub fn normalize_sol_usdc_trade(
    t: &SwapTradePreview,
) -> (Option<NormalizedSwapTrade>, PairNormalizationStatus) {
    if t.amount_in <= 0.0 || t.amount_out <= 0.0 {
        return (None, PairNormalizationStatus::InvalidAmount);
    }

    let in_is_sol = is_token_eq(&t.token_in, "SOL");
    let in_is_usdc = is_token_eq(&t.token_in, "USDC");
    let out_is_sol = is_token_eq(&t.token_out, "SOL");
    let out_is_usdc = is_token_eq(&t.token_out, "USDC");

    // 対象は SOL <-> USDC のみ
    let is_sol_usdc_pair = (in_is_sol && out_is_usdc) || (in_is_usdc && out_is_sol);
    if !is_sol_usdc_pair {
        return (None, PairNormalizationStatus::NotSolUsdcPair);
    }

    let (exec_price_usdc_per_sol, notional_usdc) = if in_is_usdc && out_is_sol {
        // USDC -> SOL
        // 価格 = amount_in / amount_out (USDC/SOL)
        (t.amount_in / t.amount_out, t.amount_in)
    } else {
        // SOL -> USDC
        // 価格 = amount_out / amount_in (USDC/SOL)
        (t.amount_out / t.amount_in, t.amount_out)
    };

    let n = NormalizedSwapTrade {
        signature: t.signature.clone(),
        slot: t.slot,
        timestamp: t.timestamp,
        fee: t.fee,
        source: t.source.clone(),

        amount_in: t.amount_in,
        token_in: t.token_in.clone(),
        amount_out: t.amount_out,
        token_out: t.token_out.clone(),

        exec_price_usdc_per_sol,
        notional_usdc,
    };

    (Some(n), PairNormalizationStatus::SolUsdcNormalized)
}

/// 複数tradeを正規化し、レポートも返す
pub fn normalize_sol_usdc_trades(
    trades: &[SwapTradePreview],
) -> (Vec<NormalizedSwapTrade>, NormalizationReport) {
    let mut out = Vec::new();
    let mut normalized_sol_usdc = 0usize;
    let mut dropped_not_sol_usdc_pair = 0usize;
    let mut dropped_invalid_amount = 0usize;

    for t in trades {
        let (norm, status) = normalize_sol_usdc_trade(t);
        match status {
            PairNormalizationStatus::SolUsdcNormalized => {
                if let Some(n) = norm {
                    out.push(n);
                    normalized_sol_usdc += 1;
                }
            }
            PairNormalizationStatus::NotSolUsdcPair => dropped_not_sol_usdc_pair += 1,
            PairNormalizationStatus::InvalidAmount => dropped_invalid_amount += 1,
        }
    }

    let report = NormalizationReport {
        total_trades: trades.len(),
        normalized_sol_usdc,
        dropped_not_sol_usdc_pair,
        dropped_invalid_amount,
    };

    (out, report)
}

/// 正規化済み trade を slotごとに集約
pub fn aggregate_normalized_swaps_by_slot(
    trades: &[NormalizedSwapTrade],
) -> Vec<NormalizedSlotSwapAggregate> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<u64, Vec<&NormalizedSwapTrade>> = BTreeMap::new();
    for t in trades {
        map.entry(t.slot).or_default().push(t);
    }

    let mut out = Vec::with_capacity(map.len());

    for (slot, ts) in map {
        let swap_count = ts.len();

        let prices: Vec<f64> = ts.iter().map(|t| t.exec_price_usdc_per_sol).collect();
        let mean_price = prices.iter().sum::<f64>() / prices.len() as f64;
        let min_price = prices.iter().copied().fold(f64::INFINITY, |a, b| a.min(b));
        let max_price = prices.iter().copied().fold(f64::NEG_INFINITY, |a, b| a.max(b));

        let price_range_bps = if mean_price > 0.0 {
            (max_price - min_price).abs() / mean_price * 10_000.0
        } else {
            0.0
        };

        let total_notional_usdc = ts.iter().map(|t| t.notional_usdc).sum::<f64>();
        let mean_notional_usdc = if swap_count > 0 {
            total_notional_usdc / swap_count as f64
        } else {
            0.0
        };

        let fees: Vec<u64> = ts.iter().filter_map(|t| t.fee).collect();
        let fee_count = fees.len();
        let mean_fee_lamports = if fees.is_empty() {
            None
        } else {
            Some(fees.iter().sum::<u64>() as f64 / fees.len() as f64)
        };

        out.push(NormalizedSlotSwapAggregate {
            slot,
            swap_count,
            mean_price_usdc_per_sol: mean_price,
            min_price_usdc_per_sol: min_price,
            max_price_usdc_per_sol: max_price,
            price_range_bps,
            total_notional_usdc,
            mean_notional_usdc,
            fee_count,
            mean_fee_lamports,
        });
    }

    out
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
    use serde_json::json;

    #[test]
    fn extract_tx_preview_basic() {
        let v = json!({
            "signature": "abc",
            "slot": 123,
            "timestamp": 1700000000,
            "type": "SWAP",
            "source": "JUPITER",
            "fee": 5000,
            "description": "swap tx"
        });

        let p = extract_tx_preview(&v);
        assert_eq!(p.signature.as_deref(), Some("abc"));
        assert_eq!(p.slot, Some(123));
        assert_eq!(p.tx_type.as_deref(), Some("SWAP"));
    }

    #[test]
    fn aggregate_tx_counts_by_slot_basic() {
        let xs = vec![
            json!({"slot": 10}),
            json!({"slot": 10}),
            json!({"slot": 12}),
        ];

        let out = aggregate_tx_counts_by_slot(&xs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].slot, 10);
        assert_eq!(out[0].tx_count, 2);
        assert_eq!(out[1].slot, 12);
        assert_eq!(out[1].tx_count, 1);
    }

    #[test]
    fn parse_swap_description_basic() {
        let desc = "Alice swapped 180.714539 USDC for 2.10115039 SOL";
        let p = parse_swap_description(desc).unwrap();
        assert_eq!(p.token_in, "USDC");
        assert_eq!(p.token_out, "SOL");
        assert!((p.amount_in - 180.714539).abs() < 1e-12);
        assert!((p.amount_out - 2.10115039).abs() < 1e-12);
    }

    #[test]
    fn extract_swap_trade_preview_basic() {
        let v = json!({
            "signature": "abc",
            "slot": 123,
            "timestamp": 1700000000,
            "type": "SWAP",
            "source": "RAYDIUM",
            "fee": 27000,
            "description": "Alice swapped 180.714539 USDC for 2.10115039 SOL"
        });

        let t = extract_swap_trade_preview(&v).unwrap();
        assert_eq!(t.slot, 123);
        assert_eq!(t.token_in, "USDC");
        assert_eq!(t.token_out, "SOL");
        assert!(t.execution_price_in_per_out > 0.0);
        assert!(t.notional_usdc.is_some());
    }

    #[test]
    fn aggregate_swaps_by_slot_basic() {
        let trades = vec![
            SwapTradePreview {
                signature: Some("a".into()),
                slot: 10,
                timestamp: Some(1),
                fee: Some(5000),
                source: Some("RAYDIUM".into()),
                amount_in: 100.0,
                token_in: "USDC".into(),
                amount_out: 1.0,
                token_out: "SOL".into(),
                execution_price_in_per_out: 100.0,
                notional_usdc: Some(100.0),
            },
            SwapTradePreview {
                signature: Some("b".into()),
                slot: 10,
                timestamp: Some(1),
                fee: Some(5000),
                source: Some("RAYDIUM".into()),
                amount_in: 202.0,
                token_in: "USDC".into(),
                amount_out: 2.0,
                token_out: "SOL".into(),
                execution_price_in_per_out: 101.0,
                notional_usdc: Some(202.0),
            },
            SwapTradePreview {
                signature: Some("c".into()),
                slot: 12,
                timestamp: Some(2),
                fee: Some(5000),
                source: Some("RAYDIUM".into()),
                amount_in: 99.0,
                token_in: "USDC".into(),
                amount_out: 1.0,
                token_out: "SOL".into(),
                execution_price_in_per_out: 99.0,
                notional_usdc: Some(99.0),
            },
        ];

        let out = aggregate_swaps_by_slot(&trades);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].slot, 10);
        assert_eq!(out[0].swap_count, 2);
        assert!(out[0].mean_exec_price_in_per_out > 0.0);
        assert!(out[0].price_range_bps > 0.0);
        assert!((out[0].total_notional_usdc - 302.0).abs() < 1e-12);
    }

    #[test]
    fn normalize_sol_usdc_trade_handles_both_directions() {
        let usdc_to_sol = SwapTradePreview {
            signature: None,
            slot: 1,
            timestamp: None,
            fee: None,
            source: None,
            amount_in: 86.0,
            token_in: "USDC".into(),
            amount_out: 1.0,
            token_out: "SOL".into(),
            execution_price_in_per_out: 86.0,
            notional_usdc: Some(86.0),
        };

        let sol_to_usdc = SwapTradePreview {
            signature: None,
            slot: 1,
            timestamp: None,
            fee: None,
            source: None,
            amount_in: 1.0,
            token_in: "SOL".into(),
            amount_out: 86.0,
            token_out: "USDC".into(),
            execution_price_in_per_out: 86.0, // 実際ここは向き依存だが normalize は amountベースで再計算
            notional_usdc: Some(86.0),
        };

        let (n1, s1) = normalize_sol_usdc_trade(&usdc_to_sol);
        let (n2, s2) = normalize_sol_usdc_trade(&sol_to_usdc);

        assert_eq!(s1, PairNormalizationStatus::SolUsdcNormalized);
        assert_eq!(s2, PairNormalizationStatus::SolUsdcNormalized);

        let p1 = n1.unwrap().exec_price_usdc_per_sol;
        let p2 = n2.unwrap().exec_price_usdc_per_sol;

        assert!((p1 - 86.0).abs() < 1e-12);
        assert!((p2 - 86.0).abs() < 1e-12);
    }

    #[test]
    fn normalize_sol_usdc_trades_filters_noise() {
        let trades = vec![
            SwapTradePreview {
                signature: None,
                slot: 1,
                timestamp: None,
                fee: None,
                source: None,
                amount_in: 100.0,
                token_in: "USDC".into(),
                amount_out: 1.0,
                token_out: "SOL".into(),
                execution_price_in_per_out: 100.0,
                notional_usdc: Some(100.0),
            },
            SwapTradePreview {
                signature: None,
                slot: 2,
                timestamp: None,
                fee: None,
                source: None,
                amount_in: 1.0,
                token_in: "USDC".into(),
                amount_out: 1.0,
                token_out: "USDT".into(),
                execution_price_in_per_out: 1.0,
                notional_usdc: Some(1.0),
            },
        ];

        let (norm, report) = normalize_sol_usdc_trades(&trades);
        assert_eq!(norm.len(), 1);
        assert_eq!(report.total_trades, 2);
        assert_eq!(report.normalized_sol_usdc, 1);
        assert_eq!(report.dropped_not_sol_usdc_pair, 1);
    }

    #[test]
    fn aggregate_normalized_swaps_by_slot_basic() {
        let trades = vec![
            NormalizedSwapTrade {
                signature: None,
                slot: 10,
                timestamp: None,
                fee: Some(5000),
                source: None,
                amount_in: 100.0,
                token_in: "USDC".into(),
                amount_out: 1.0,
                token_out: "SOL".into(),
                exec_price_usdc_per_sol: 100.0,
                notional_usdc: 100.0,
            },
            NormalizedSwapTrade {
                signature: None,
                slot: 10,
                timestamp: None,
                fee: Some(10000),
                source: None,
                amount_in: 202.0,
                token_in: "USDC".into(),
                amount_out: 2.0,
                token_out: "SOL".into(),
                exec_price_usdc_per_sol: 101.0,
                notional_usdc: 202.0,
            },
        ];

        let out = aggregate_normalized_swaps_by_slot(&trades);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slot, 10);
        assert_eq!(out[0].swap_count, 2);
        assert!((out[0].mean_price_usdc_per_sol - 100.5).abs() < 1e-12);
        assert!(out[0].price_range_bps > 0.0);
        assert!((out[0].total_notional_usdc - 302.0).abs() < 1e-12);
        assert_eq!(out[0].fee_count, 2);
        assert!(out[0].mean_fee_lamports.is_some());
    }
}
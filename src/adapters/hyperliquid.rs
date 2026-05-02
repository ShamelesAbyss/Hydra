//! Hyperliquid mainnet adapter (testnet removed, mainnet-only per user requirement).

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Lenient deserializer: accepts string / int / float / bool / null and
/// coerces to String. Defends against Hyperliquid occasionally returning
/// numeric types where previous responses returned strings (e.g. `leverage.value`
/// can be `20` or `"20"` depending on whether custom leverage was set).
mod flex_string {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        match Value::deserialize(d)? {
            Value::String(s) => Ok(s),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok(String::new()),
            other => Ok(other.to_string()),
        }
    }
}
/// Same as `flex_string` but yields `Option<String>` and tolerates missing keys.
mod flex_string_opt {
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        match Option::<Value>::deserialize(d)? {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(Value::Number(n)) => Ok(Some(n.to_string())),
            Some(Value::Bool(b)) => Ok(Some(b.to_string())),
            Some(other) => Ok(Some(other.to_string())),
        }
    }
}

use crate::config::{defaults, AppConfig};
use crate::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, VenueKind};
use super::{VenueAdapter, VenuePosition};

pub struct HyperliquidAdapter {
    http: reqwest::Client,
    base_url: String,
    account_address: Option<String>,
    exchange: Arc<Mutex<Option<hyperliquid_rust_sdk::ExchangeClient>>>,
    wallet_key: Option<String>,
    /// v1.4.0: per-asset size decimals fetched once at startup from the
    /// `/info {"type":"meta"}` endpoint. Hyperliquid rejects any size with
    /// more decimals than the asset allows (e.g. HYPE = 2, ETH = 4, BTC = 5).
    /// Without this, HYPE orders silently bounce all day.
    sz_decimals: Arc<Mutex<std::collections::HashMap<String, u8>>>,
}

impl HyperliquidAdapter {
    pub async fn new(cfg: &AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let derived_addr = cfg.hl_private_key.as_ref().and_then(|k| derive_eth_address(k).ok());
        let account_address = cfg.hl_account_address.clone().or(derived_addr);
        tracing::info!(network="mainnet", account=?account_address, "hyperliquid adapter init");
        let adapter = Self {
            http,
            base_url: defaults::HYPERLIQUID_API.to_string(),
            account_address,
            exchange: Arc::new(Mutex::new(None)),
            wallet_key: cfg.hl_private_key.clone(),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        // Best-effort prefetch of per-asset size precision. Any failure here
        // falls back to a safe default of 2 decimals at order time.
        adapter.warm_meta().await;
        Ok(adapter)
    }

    /// Fetch the venue meta (universe → szDecimals per asset) and cache it.
    /// Idempotent: subsequent calls refresh the cache without disrupting open
    /// trades. Logged at INFO so the operator can confirm HYPE precision in
    /// the cockpit feed.
    async fn warm_meta(&self) {
        #[derive(Deserialize)]
        struct AssetMeta {
            name: String,
            #[serde(rename = "szDecimals")]
            sz_decimals: u8,
        }
        #[derive(Deserialize)]
        struct Universe { universe: Vec<AssetMeta> }
        match self.info::<Universe>(json!({"type":"meta"})).await {
            Ok(meta) => {
                let mut guard = self.sz_decimals.lock().await;
                guard.clear();
                for a in &meta.universe {
                    guard.insert(a.name.clone(), a.sz_decimals);
                }
                let count = guard.len();
                let hype = guard.get("HYPE").copied();
                tracing::info!(
                    asset_count = count,
                    hype_sz_decimals = ?hype,
                    "hyperliquid meta warmed (per-asset szDecimals cached)"
                );
            }
            Err(e) => {
                tracing::warn!(error=%e, "could not warm hyperliquid meta — will use 2dp default for sizing");
            }
        }
    }

    /// Per-asset size precision. Defaults to 2dp for safety if meta is unknown
    /// (2dp works for HYPE/SUI/most newer perps; ETH/BTC asked for fewer than
    /// 2dp would still be valid since their actual cap is 4-5).
    async fn sz_decimals_for(&self, symbol: &str) -> u8 {
        let guard = self.sz_decimals.lock().await;
        *guard.get(symbol).unwrap_or(&2)
    }

    async fn info<T: for<'de> Deserialize<'de>>(&self, body: serde_json::Value) -> Result<T> {
        let url = format!("{}/info", self.base_url);
        // Smart backoff: up to 4 tries with 250ms → 500ms → 1s → 2s. Retries on
        // both transport errors AND body-decode errors (HL occasionally returns
        // 200 with a non-JSON body during hot periods).
        let mut backoff = 250u64;
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..4 {
            match self.http.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.text().await {
                        Ok(txt) => match serde_json::from_str::<T>(&txt) {
                            Ok(v) => return Ok(v),
                            Err(e) => last_err = Some(anyhow!("decoding hyperliquid info response: {e}; body={}",
                                if txt.len() > 256 { format!("{}…", &txt[..256]) } else { txt })),
                        },
                        Err(e) => last_err = Some(anyhow::Error::new(e)),
                    }
                }
                Ok(resp) => {
                    let s = resp.status();
                    let txt = resp.text().await.unwrap_or_default();
                    last_err = Some(anyhow!("hyperliquid info {s}: {txt}"));
                }
                Err(e) => last_err = Some(anyhow::Error::new(e)),
            }
            if attempt < 3 {
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                backoff *= 2;
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("hyperliquid info unknown error")))
    }

    async fn ensure_exchange(&self) -> Result<()> {
        let mut guard = self.exchange.lock().await;
        if guard.is_some() { return Ok(()); }
        let key = self.wallet_key.as_ref().ok_or_else(|| anyhow!("HYPERLIQUID_PRIVATE_KEY not set"))?;
        let wallet = ethers::signers::LocalWallet::from_str(key.trim_start_matches("0x"))
            .context("parsing Hyperliquid private key")?;
        let client = hyperliquid_rust_sdk::ExchangeClient::new(
            None, wallet, Some(hyperliquid_rust_sdk::BaseUrl::Mainnet), None, None
        ).await.map_err(|e| anyhow!("init Hyperliquid ExchangeClient: {e}"))?;
        *guard = Some(client);
        Ok(())
    }
}

fn derive_eth_address(priv_hex: &str) -> Result<String> {
    use ethers::signers::{LocalWallet, Signer};
    let w = LocalWallet::from_str(priv_hex.trim_start_matches("0x")).context("parse private key")?;
    Ok(format!("{:?}", w.address()))
}

/// Round a perp price to Hyperliquid's tick rule: max 5 significant figures,
/// max 6 decimals. For integer-range prices (e.g. BTC > $10k) it returns an int.
fn round_perp_price(px: f64) -> f64 {
    if px <= 0.0 || !px.is_finite() { return 0.0; }
    let magnitude = px.abs().log10().floor() as i32;
    // 5 sig figs → decimals = 4 - magnitude. Floor at 0 (integer), cap at 6.
    let decimals = (4 - magnitude).max(0).min(6);
    let scale = 10f64.powi(decimals);
    (px * scale).round() / scale
}

/// Floor `value` to `dp` decimal places. Uses `floor` (not `round`) so we
/// never accidentally bump a position above the requested risk-budget size.
fn round_to_decimals(value: f64, dp: u8) -> f64 {
    if value <= 0.0 || !value.is_finite() { return 0.0; }
    let scale = 10f64.powi(dp as i32);
    (value * scale).floor() / scale
}

/// Parse an ExchangeResponse debug string to determine real acceptance.
/// ethers Debug format for enum tuple variants is `Filled(FilledOrder { ... })`,
/// `Resting(RestingOrder { ... })`, `Error("...")`.
fn parse_order_response(raw: &str, fallback_sz: f64) -> (bool, f64, Option<f64>, Option<String>) {
    let has_filled  = raw.contains("Filled(")  || raw.contains("Filled {");
    let has_resting = raw.contains("Resting(") || raw.contains("Resting {");
    let has_error   = raw.contains("Error(");

    let accepted = !has_error && (has_filled || has_resting);

    // Extract total_sz / avg_px from the Filled(...) payload if present.
    let filled_sz = if has_filled {
        extract_quoted(raw, "total_sz: ").and_then(|s| s.parse::<f64>().ok()).unwrap_or(fallback_sz)
    } else { 0.0 };
    let avg_px = if has_filled {
        extract_quoted(raw, "avg_px: ").and_then(|s| s.parse::<f64>().ok())
    } else { None };

    // oid is an unquoted integer.
    let oid = raw.split("oid: ").nth(1)
        .map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .filter(|s| !s.is_empty());

    (accepted, filled_sz, avg_px, oid)
}

fn extract_quoted(raw: &str, prefix: &str) -> Option<String> {
    let start = raw.find(prefix)?;
    let after = &raw[start + prefix.len()..];
    let q_start = after.find('"')? + 1;
    let after_q = &after[q_start..];
    let q_end = after_q.find('"')?;
    Some(after_q[..q_end].to_string())
}

#[async_trait]
impl VenueAdapter for HyperliquidAdapter {
    fn kind(&self) -> VenueKind { VenueKind::Hyperliquid }

    async fn ping(&self) -> Result<()> {
        let _mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
        Ok(())
    }

    async fn snapshot(&self, symbol: &str, lookback: usize) -> Result<MarketSnapshot> {
        let mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
        let mid_str = mids.get(symbol).and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("hyperliquid: no mid for {symbol}"))?;
        let mid: f64 = mid_str.parse()?;
        let now_ms = Utc::now().timestamp_millis();
        let start_ms = now_ms - (lookback as i64) * 60_000;
        let body = json!({"type":"candleSnapshot","req":{"coin":symbol,"interval":"1m","startTime":start_ms,"endTime":now_ms}});
        let raw: Vec<RawCandle> = self.info(body).await.unwrap_or_default();
        let candles: Vec<Candle> = raw.into_iter().filter_map(|c| c.to_candle()).collect();

        let book: Result<BookResp> = self.info(json!({"type":"l2Book","coin":symbol})).await;
        let (bid, ask, spread_bps) = match book {
            Ok(b) => {
                let bid = b.levels.get(0).and_then(|lv| lv.first()).and_then(|l| l.px.parse::<f64>().ok());
                let ask = b.levels.get(1).and_then(|lv| lv.first()).and_then(|l| l.px.parse::<f64>().ok());
                let sp = match (bid, ask) {
                    (Some(b), Some(a)) if a > 0.0 => Some((a - b) / a * 10_000.0),
                    _ => None,
                };
                (bid, ask, sp)
            }
            Err(_) => (None, None, None),
        };
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.to_string(),
            mid, bid, ask, spread_bps,
            last_update: Utc::now(),
            candles,
        })
    }

    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        let addr = self.account_address.as_ref().ok_or_else(|| anyhow!("no account address"))?;
        let state: ClearingHouseState = self.info(json!({"type":"clearinghouseState","user":addr})).await?;
        let mut out = Vec::new();
        for p in state.asset_positions {
            let sz: f64 = p.position.szi.parse().unwrap_or(0.0);
            if sz.abs() < 1e-12 { continue; }
            let entry: f64 = p.position.entry_px.parse().unwrap_or(0.0);
            let upnl: f64 = p.position.unrealized_pnl.parse().unwrap_or(0.0);
            let lev: f64 = p.position.leverage.value.parse().unwrap_or(1.0);
            out.push(VenuePosition { symbol: p.position.coin, size: sz, entry_price: entry, unrealized_pnl: upnl, leverage: lev });
        }
        Ok(out)
    }

    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        self.ensure_exchange().await?;

        // ---- Resolve price for IOC execution ----
        // If caller didn't supply one, fetch the venue mid and cross by 3% so the
        // order is marketable. Hyperliquid rejects price = 0.
        let raw_px = match intent.limit_price {
            Some(p) if p > 0.0 => p,
            _ => {
                let mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
                let mid: f64 = mids.get(&intent.symbol)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| anyhow!("no live mid for {}", intent.symbol))?;
                if intent.side.is_buy() { mid * 1.03 } else { mid * 0.97 }
            }
        };
        // Hyperliquid perp: max 5 sig figs, max 6 decimals. Round to 5 sig figs.
        let limit_px = round_perp_price(raw_px);
        // v1.4.0: round size to the venue's per-asset szDecimals, not a fixed
        // 4dp. This was the root cause of every HYPE order being rejected
        // (HYPE = 2dp, while sizes like 2.2553 had 4dp).
        let dp = self.sz_decimals_for(&intent.symbol).await;
        let sz = round_to_decimals(intent.size, dp);
        if sz <= 0.0 {
            return Ok(OrderResult {
                intent_id: intent.id.clone(), accepted: false,
                venue_order_id: None, filled_size: 0.0, avg_fill_price: None,
                message: format!("size {} rounds to 0 at {}dp for {}", intent.size, dp, intent.symbol),
            });
        }

        let guard = self.exchange.lock().await;
        let client = guard.as_ref().unwrap();
        let order = hyperliquid_rust_sdk::ClientOrderRequest {
            asset: intent.symbol.clone(),
            is_buy: intent.side.is_buy(),
            reduce_only: intent.reduce_only,
            limit_px,
            sz,
            cloid: None,
            order_type: hyperliquid_rust_sdk::ClientOrder::Limit(
                hyperliquid_rust_sdk::ClientLimit { tif: "Ioc".to_string() },
            ),
        };
        let resp = client.order(order, None).await.map_err(|e| anyhow!("hyperliquid order error: {e}"))?;

        // Deep-inspect the response: an outer Ok() with an inner Error(...) status
        // is still a rejection. Only count it as accepted if we saw a Filled or
        // Resting status (no inner Error).
        let raw = format!("{:?}", resp);
        let (accepted, filled_sz, avg_px, venue_oid) = parse_order_response(&raw, sz);
        let message = if accepted {
            match avg_px {
                Some(px) => format!("FILLED @ ${:.4} sz={:.4}", px, filled_sz),
                None => format!("accepted @ ~${:.4}", limit_px),
            }
        } else {
            // Truncate noisy raw debug in rejection messages.
            let short = if raw.len() > 200 { format!("{}…", &raw[..200]) } else { raw.clone() };
            format!("REJECTED  {}", short)
        };
        Ok(OrderResult {
            intent_id: intent.id.clone(),
            accepted,
            venue_order_id: venue_oid,
            filled_size: filled_sz,
            avg_fill_price: avg_px,
            message,
        })
    }

    async fn close_position(&self, symbol: &str, size: f64, side_is_buy_to_close: bool) -> Result<OrderResult> {
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: symbol.to_string(),
            side: if side_is_buy_to_close { crate::core::Side::Long } else { crate::core::Side::Short },
            size: size.abs(),
            limit_price: None, reduce_only: true,
            stop_price: None, take_profit: None, source_signal: None,
            created_at: Utc::now(),
        };
        self.submit_order(&intent).await
    }

    async fn account_equity_usd(&self) -> Result<f64> {
        let addr = match &self.account_address { Some(a) => a.clone(), None => return Ok(0.0) };

        // Perps sub-account equity (zero until a perp position is opened,
        // even under unified margin mode).
        let perps_eq: f64 = match self.info::<ClearingHouseState>(
            json!({"type":"clearinghouseState","user":addr}),
        ).await {
            Ok(s) => s.margin_summary.account_value.parse().unwrap_or(0.0),
            Err(e) => { tracing::warn!(error=%e, "clearinghouseState failed"); 0.0 }
        };

        // Spot sub-account balances. Under unified margin this USDC collateralises
        // perps automatically; we sum USDC + any stablecoins + (optionally) valued
        // HYPE to report the true unified equity.
        let spot_usdc: f64 = match self.info::<SpotClearinghouseState>(
            json!({"type":"spotClearinghouseState","user":addr}),
        ).await {
            Ok(s) => s.balances.iter()
                .filter(|b| matches!(b.coin.as_str(), "USDC" | "USDT" | "USD"))
                .filter_map(|b| b.total.parse::<f64>().ok())
                .sum(),
            Err(e) => { tracing::warn!(error=%e, "spotClearinghouseState failed"); 0.0 }
        };

        let total = perps_eq + spot_usdc;
        tracing::info!(perps_equity=perps_eq, spot_stables=spot_usdc, unified_total=total,
            "hyperliquid equity breakdown");
        Ok(total)
    }
}

#[derive(Deserialize)]
struct RawCandle {
    t: i64,
    #[serde(deserialize_with = "flex_string::deserialize")] o: String,
    #[serde(deserialize_with = "flex_string::deserialize")] h: String,
    #[serde(deserialize_with = "flex_string::deserialize")] l: String,
    #[serde(deserialize_with = "flex_string::deserialize")] c: String,
    #[serde(deserialize_with = "flex_string::deserialize")] v: String,
}
impl RawCandle {
    fn to_candle(self) -> Option<Candle> {
        Some(Candle {
            ts: Utc.timestamp_millis_opt(self.t).single()?,
            open: self.o.parse().ok()?,
            high: self.h.parse().ok()?,
            low: self.l.parse().ok()?,
            close: self.c.parse().ok()?,
            volume: self.v.parse().unwrap_or(0.0),
        })
    }
}
#[derive(Deserialize)]
struct BookLevel {
    #[serde(deserialize_with = "flex_string::deserialize")] px: String,
    #[allow(dead_code)] #[serde(deserialize_with = "flex_string::deserialize")] sz: String,
    #[allow(dead_code)] n: u64,
}
#[derive(Deserialize)]
struct BookResp { levels: Vec<Vec<BookLevel>> }

#[derive(Deserialize, Default)]
struct SpotClearinghouseState {
    #[serde(default)]
    balances: Vec<SpotBalance>,
}
#[derive(Deserialize)]
struct SpotBalance {
    coin: String,
    #[serde(default, deserialize_with = "flex_string::deserialize")]
    total: String,
    #[allow(dead_code)] #[serde(default, deserialize_with = "flex_string::deserialize")] hold: String,
    #[allow(dead_code)] #[serde(default)] token: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearingHouseState {
    #[serde(default)] asset_positions: Vec<AssetPosition>,
    margin_summary: MarginSummary,
}
#[derive(Deserialize)]
struct AssetPosition {
    position: AssetPositionInner,
    #[serde(default)] #[allow(dead_code)] r#type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetPositionInner {
    coin: String,
    #[serde(deserialize_with = "flex_string::deserialize")] szi: String,
    #[serde(deserialize_with = "flex_string::deserialize")] entry_px: String,
    #[serde(deserialize_with = "flex_string::deserialize")] unrealized_pnl: String,
    leverage: Leverage,
}
#[derive(Deserialize)]
struct Leverage {
    #[serde(deserialize_with = "flex_string::deserialize")] value: String,
    #[allow(dead_code)] #[serde(default)] r#type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginSummary {
    #[serde(deserialize_with = "flex_string::deserialize")] account_value: String,
    #[allow(dead_code)] #[serde(default, deserialize_with = "flex_string_opt::deserialize")] total_margin_used: Option<String>,
    #[allow(dead_code)] #[serde(default, deserialize_with = "flex_string_opt::deserialize")] total_ntl_pos: Option<String>,
}


#[cfg(test)]
mod tests {
    //! Regression tests for Hyperliquid JSON schema drift.
    //!
    //! Hyperliquid's `/info` endpoint historically sent numeric-valued fields
    //! as JSON strings (e.g. `"20"`), but under certain account conditions it
    //! switches to native numbers (e.g. `20`). Before v1.2.4 this caused the
    //! strict `Deserialize` impls to panic with
    //! `invalid type: integer 20, expected a string`, killing position reads.
    //! These tests lock in the lenient behaviour.
    use super::*;

    #[test]
    fn leverage_decodes_when_value_is_integer() {
        // Observed live-market payload (TP hit on Feb 2026).
        let raw = serde_json::json!({"value": 20, "type": "cross"});
        let lev: Leverage = serde_json::from_value(raw).expect("leverage should decode from integer");
        assert_eq!(lev.value, "20");
    }

    #[test]
    fn leverage_decodes_when_value_is_string() {
        let raw = serde_json::json!({"value": "10", "type": "isolated"});
        let lev: Leverage = serde_json::from_value(raw).expect("leverage should decode from string");
        assert_eq!(lev.value, "10");
    }

    #[test]
    fn asset_position_inner_decodes_mixed_types() {
        // Mix: string szi, numeric entry_px, string unrealized_pnl,
        // integer leverage.value — exactly the kind of drift we saw.
        let raw = serde_json::json!({
            "coin": "ETH",
            "szi": "0.05",
            "entryPx": 2000.5,
            "unrealizedPnl": "-1.23",
            "leverage": {"value": 5, "type": "cross"}
        });
        let p: AssetPositionInner = serde_json::from_value(raw).expect("mixed-type position decode");
        assert_eq!(p.szi, "0.05");
        assert_eq!(p.entry_px, "2000.5");
        assert_eq!(p.unrealized_pnl, "-1.23");
        assert_eq!(p.leverage.value, "5");
    }

    #[test]
    fn margin_summary_decodes_numeric_account_value() {
        let raw = serde_json::json!({
            "accountValue": 123.45,
            "totalMarginUsed": 10,
            "totalNtlPos": "50.1"
        });
        let m: MarginSummary = serde_json::from_value(raw).expect("margin summary decode");
        assert_eq!(m.account_value, "123.45");
        assert_eq!(m.total_margin_used.as_deref(), Some("10"));
        assert_eq!(m.total_ntl_pos.as_deref(), Some("50.1"));
    }

    #[test]
    fn margin_summary_decodes_missing_optionals() {
        let raw = serde_json::json!({"accountValue": "0.0"});
        let m: MarginSummary = serde_json::from_value(raw).expect("margin summary decode");
        assert_eq!(m.account_value, "0.0");
        assert!(m.total_margin_used.is_none());
        assert!(m.total_ntl_pos.is_none());
    }

    #[test]
    fn spot_balance_decodes_numeric_total() {
        let raw = serde_json::json!({"coin": "USDC", "total": 94.12, "hold": 0});
        let b: SpotBalance = serde_json::from_value(raw).expect("spot balance decode");
        assert_eq!(b.coin, "USDC");
        assert_eq!(b.total, "94.12");
    }

    #[test]
    fn raw_candle_decodes_numeric_ohlcv() {
        let raw = serde_json::json!({
            "t": 1_700_000_000_000_i64,
            "o": 100.0, "h": 101, "l": "99.5", "c": 100.25, "v": 0
        });
        let rc: RawCandle = serde_json::from_value(raw).expect("candle decode");
        let c = rc.to_candle().expect("candle convert");
        assert!((c.open  - 100.0 ).abs() < 1e-9);
        assert!((c.high  - 101.0 ).abs() < 1e-9);
        assert!((c.low   -  99.5 ).abs() < 1e-9);
        assert!((c.close - 100.25).abs() < 1e-9);
    }

    // ─── v1.4.0 per-asset size precision regression suite ────────────────────
    //
    // Reproduces the Apr-25 incident: 12 HYPE orders sent at 4dp (e.g. 2.2553)
    // were rejected by Hyperliquid because HYPE only allows 2dp. The fix is
    // `round_to_decimals` honouring the venue's per-asset szDecimals.

    #[test]
    fn hype_size_rounds_to_two_decimals_floor() {
        // Sizes from the user's actual rejection log.
        assert_eq!(round_to_decimals(2.255284410307115, 2), 2.25);
        assert_eq!(round_to_decimals(2.273625, 2), 2.27);
    }

    #[test]
    fn eth_size_rounds_to_four_decimals_floor() {
        assert_eq!(round_to_decimals(0.040412345, 4), 0.0404);
    }

    #[test]
    fn btc_size_rounds_to_five_decimals_floor() {
        assert_eq!(round_to_decimals(0.0012345678, 5), 0.00123);
    }

    #[test]
    fn round_to_decimals_handles_bad_input_safely() {
        assert_eq!(round_to_decimals(0.0, 2), 0.0);
        assert_eq!(round_to_decimals(-1.5, 2), 0.0);
        assert_eq!(round_to_decimals(f64::NAN, 2), 0.0);
        assert_eq!(round_to_decimals(f64::INFINITY, 2), 0.0);
    }

    #[test]
    fn floor_never_exceeds_input_size() {
        // Critical invariant: rounding must NEVER increase the size, otherwise
        // we'd silently exceed the risk-budget allocation.
        for input in [0.123456_f64, 1.999999, 5.0, 10.000001, 0.000099] {
            for dp in [0u8, 1, 2, 3, 4, 5] {
                let r = round_to_decimals(input, dp);
                assert!(r <= input + 1e-12, "dp={} input={} got={}", dp, input, r);
            }
        }
    }
}

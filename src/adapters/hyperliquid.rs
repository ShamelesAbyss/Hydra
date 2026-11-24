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

use super::{VenueAdapter, VenuePosition};
use crate::config::{defaults, AppConfig};
use crate::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, VenueKind};

pub struct HyperliquidAdapter {
    http: reqwest::Client,
    live_authorized: bool,
    max_slippage_bps: f64,
    base_url: String,
    account_address: Option<String>,
    exchange: Arc<Mutex<Option<hyperliquid_rust_sdk::ExchangeClient>>>,
    wallet_key: Option<String>,
    identity_verified_at: Arc<Mutex<Option<std::time::Instant>>>,
    /// v1.4.0: per-asset size decimals fetched once at startup from the
    /// `/info {"type":"meta"}` endpoint. Hyperliquid rejects any size with
    /// more decimals than the asset allows (e.g. HYPE = 2, ETH = 4, BTC = 5).
    /// Without this, HYPE orders silently bounce all day.
    sz_decimals: Arc<Mutex<std::collections::HashMap<String, u8>>>,
}

impl HyperliquidAdapter {
    /// Read-only history retrieval. Pages are durable and replayable, but API
    /// retention and fee conventions do not certify a complete trade lifecycle.
    pub async fn sync_history(
        &self,
        storage: crate::storage::Storage,
        since_ms: i64,
        end_ms: i64,
    ) -> Result<serde_json::Value> {
        use crate::ledger::{
            AccountScope, Evidence, EvidenceSource, FinancialLedger, LedgerEnvironment,
        };
        anyhow::ensure!(
            since_ms >= 0 && end_ms > since_ms && end_ms <= Utc::now().timestamp_millis(),
            "invalid history window"
        );
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("explicit account identity required"))?;
        let ledger = FinancialLedger::new(
            storage,
            AccountScope::hyperliquid(LedgerEnvironment::Mainnet, account)?,
        )?;
        let mut total_pages = 0;
        let mut fills_inserted = 0;
        let mut funding_inserted = 0;
        for kind in ["userFillsByTime", "userFunding"] {
            let mut cursor = since_ms;
            let mut terminated = false;
            for _ in 0..128 {
                let mut request =
                    json!({"type":kind,"user":account,"startTime":cursor,"endTime":end_ms});
                if kind == "userFillsByTime" {
                    request["aggregateByTime"] = json!(false);
                }
                let raw: serde_json::Value = self.info(request).await?;
                let array = raw
                    .as_array()
                    .ok_or_else(|| anyhow!("history must be an array"))?;
                let length = array.len();
                let payload = serde_json::to_string(&raw)?;
                let evidence = Evidence {
                    source: EvidenceSource::RestSnapshot,
                    observed_at_ms: Utc::now().timestamp_millis(),
                };
                let times = if kind == "userFillsByTime" {
                    let fills = crate::ledger_hyperliquid::parse_fills(&payload)?;
                    anyhow::ensure!(
                        fills
                            .iter()
                            .all(|f| f.timestamp_ms >= cursor && f.timestamp_ms <= end_ms),
                        "fill outside requested window"
                    );
                    let times = fills.iter().map(|f| f.timestamp_ms).collect::<Vec<_>>();
                    fills_inserted += ledger.ingest(&fills, &[], evidence)?.new_fills;
                    times
                } else {
                    let funding = crate::ledger_hyperliquid::parse_funding(&payload)?;
                    anyhow::ensure!(
                        funding
                            .iter()
                            .all(|f| f.timestamp_ms >= cursor && f.timestamp_ms <= end_ms),
                        "funding outside requested window"
                    );
                    let times = funding.iter().map(|f| f.timestamp_ms).collect::<Vec<_>>();
                    funding_inserted += ledger.ingest(&[], &funding, evidence)?.new_funding;
                    times
                };
                total_pages += 1;
                match crate::history::next_cursor(cursor, end_ms, &times, length)? {
                    None => {
                        terminated = true;
                        break;
                    }
                    Some(next) => cursor = next,
                }
            }
            anyhow::ensure!(
                terminated,
                "history page budget exceeded; retained evidence is incomplete"
            );
        }
        Ok(
            json!({"schema_version":1,"pages":total_pages,"new_fills":fills_inserted,"new_funding":funding_inserted,
            "cash_flow":ledger.realized_cash_flow()?,"trade_settlement_certified":false}),
        )
    }

    pub async fn new(cfg: &AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let account_address = crate::ownership::configured_account(cfg)?;
        tracing::info!(network="mainnet", account=?account_address, "hyperliquid adapter init");
        let adapter = Self {
            http,
            live_authorized: cfg.live_trading,
            max_slippage_bps: cfg.risk.max_slippage_bps,
            base_url: defaults::HYPERLIQUID_API.to_string(),
            account_address,
            exchange: Arc::new(Mutex::new(None)),
            wallet_key: cfg.hl_private_key.clone(),
            identity_verified_at: Arc::new(Mutex::new(None)),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        // Prefetch metadata; orders fail closed if the instrument is unavailable.
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
        struct Universe {
            universe: Vec<AssetMeta>,
        }
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
                tracing::warn!(error=%e, "could not warm hyperliquid meta — order submission blocked without instrument metadata");
            }
        }
    }

    /// Authoritative metadata is required for every submitted instrument.
    async fn sz_decimals_for(&self, symbol: &str) -> Result<u8> {
        let guard = self.sz_decimals.lock().await;
        let dp = guard
            .get(symbol)
            .copied()
            .ok_or_else(|| anyhow!("metadata unavailable for {symbol}"))?;
        anyhow::ensure!(dp <= 6, "unsupported perp size precision");
        Ok(dp)
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
                Ok(resp) if resp.status().is_success() => match resp.text().await {
                    Ok(txt) => match serde_json::from_str::<T>(&txt) {
                        Ok(v) => return Ok(v),
                        Err(e) => {
                            last_err = Some(anyhow!(
                                "decoding hyperliquid info response: {e}; body={}",
                                if txt.len() > 256 {
                                    format!("{}…", &txt[..256])
                                } else {
                                    txt
                                }
                            ))
                        }
                    },
                    Err(e) => last_err = Some(anyhow::Error::new(e)),
                },
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

    async fn verify_identity(&self) -> Result<()> {
        let mut verified = self.identity_verified_at.lock().await;
        if verified.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(30)) {
            return Ok(());
        }
        let key = self
            .wallet_key
            .as_ref()
            .ok_or_else(|| anyhow!("signing key missing"))?;
        let signer =
            derive_eth_address(key).map_err(|_| anyhow!("invalid signing key; details omitted"))?;
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing"))?;
        let signer_role = self.info(json!({"type":"userRole","user":signer})).await?;
        let account_role = self.info(json!({"type":"userRole","user":account})).await?;
        crate::account::verify_signer(&signer, account, &signer_role, &account_role)?;
        *verified = Some(std::time::Instant::now());
        Ok(())
    }

    async fn ensure_exchange(&self) -> Result<()> {
        let mut guard = self.exchange.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let key = self
            .wallet_key
            .as_ref()
            .ok_or_else(|| anyhow!("HYPERLIQUID_PRIVATE_KEY not set"))?;
        let wallet = ethers::signers::LocalWallet::from_str(key.trim_start_matches("0x"))
            .context("parsing Hyperliquid private key")?;
        let client = hyperliquid_rust_sdk::ExchangeClient::new(
            None,
            wallet,
            Some(hyperliquid_rust_sdk::BaseUrl::Mainnet),
            None,
            None,
        )
        .await
        .map_err(|e| anyhow!("init Hyperliquid ExchangeClient: {e}"))?;
        *guard = Some(client);
        Ok(())
    }
}

fn derive_eth_address(priv_hex: &str) -> Result<String> {
    use ethers::signers::{LocalWallet, Signer};
    let w =
        LocalWallet::from_str(priv_hex.trim_start_matches("0x")).context("parse private key")?;
    Ok(format!("{:?}", w.address()))
}

/// Round a perp price to Hyperliquid's tick rule: max 5 significant figures,
/// max 6 decimals. For integer-range prices (e.g. BTC > $10k) it returns an int.
fn round_perp_price(px: f64, sz_decimals: u8, buy: bool) -> f64 {
    if px <= 0.0 || !px.is_finite() {
        return 0.0;
    }
    let magnitude = px.abs().log10().floor() as i32;
    // 5 sig figs → decimals = 4 - magnitude. Floor at 0 (integer), cap at 6.
    let decimals = (4 - magnitude).clamp(0, 6_i32.saturating_sub(sz_decimals as i32));
    let scale = 10f64.powi(decimals);
    if buy {
        (px * scale).floor() / scale
    } else {
        (px * scale).ceil() / scale
    }
}

/// Floor `value` to `dp` decimal places. Uses `floor` (not `round`) so we
/// never accidentally bump a position above the requested risk-budget size.
fn round_to_decimals(value: f64, dp: u8) -> f64 {
    if value <= 0.0 || !value.is_finite() {
        return 0.0;
    }
    let scale = 10f64.powi(dp as i32);
    (value * scale).floor() / scale
}

/// Single-order typed acknowledgment. Unrecognized/malformed responses stay unresolved.
fn parse_order_response(
    resp: hyperliquid_rust_sdk::ExchangeResponseStatus,
    id: &str,
) -> Result<OrderResult> {
    use hyperliquid_rust_sdk::{ExchangeDataStatus as Status, ExchangeResponseStatus as Response};
    let mut out = OrderResult {
        intent_id: id.into(),
        accepted: false,
        venue_order_id: None,
        filled_size: 0.0,
        avg_fill_price: None,
        message: String::new(),
    };
    let response = match resp {
        Response::Err(_) => {
            out.message = "exchange rejected request".into();
            return Ok(out);
        }
        Response::Ok(r) => r,
    };
    anyhow::ensure!(
        response.response_type == "order",
        "unexpected exchange response type"
    );
    let statuses = response
        .data
        .ok_or_else(|| anyhow!("missing order statuses"))?
        .statuses;
    anyhow::ensure!(statuses.len() == 1, "expected exactly one order status");
    match statuses.into_iter().next().unwrap() {
        Status::Filled(fill) => {
            let qty: f64 = fill.total_sz.parse().context("invalid filled size")?;
            let px: f64 = fill.avg_px.parse().context("invalid fill price")?;
            anyhow::ensure!(
                qty.is_finite() && qty > 0.0 && px.is_finite() && px > 0.0,
                "invalid fill"
            );
            out.accepted = true;
            out.filled_size = qty;
            out.avg_fill_price = Some(px);
            out.venue_order_id = Some(fill.oid.to_string());
            out.message = "confirmed fill; fees/funding pending".into();
        }
        Status::Resting(order) => {
            out.accepted = true;
            out.venue_order_id = Some(order.oid.to_string());
            out.message = "resting; no confirmed fill".into();
        }
        Status::Error(_) => out.message = "exchange rejected order".into(),
        _ => anyhow::bail!("order acknowledgment unresolved; reconciliation required"),
    }
    Ok(out)
}

#[async_trait]
impl VenueAdapter for HyperliquidAdapter {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }

    async fn ping(&self) -> Result<()> {
        let _mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
        Ok(())
    }

    async fn snapshot(&self, symbol: &str, lookback: usize) -> Result<MarketSnapshot> {
        let mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
        let mid_str = mids
            .get(symbol)
            .and_then(|v| v.as_str())
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
                let bid = b
                    .levels
                    .get(0)
                    .and_then(|lv| lv.first())
                    .and_then(|l| l.px.parse::<f64>().ok());
                let ask = b
                    .levels
                    .get(1)
                    .and_then(|lv| lv.first())
                    .and_then(|l| l.px.parse::<f64>().ok());
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
            mid,
            bid,
            ask,
            spread_bps,
            last_update: Utc::now(),
            candles,
        })
    }

    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        let addr = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("no account address"))?;
        let state: ClearingHouseState = self
            .info(json!({"type":"clearinghouseState","user":addr}))
            .await?;
        let mut out = Vec::new();
        for p in state.asset_positions {
            let sz: f64 = p.position.szi.parse().context("invalid position size")?;
            anyhow::ensure!(sz.is_finite(), "non-finite position size");
            if sz.abs() < 1e-12 {
                continue;
            }
            let entry: f64 = p.position.entry_px.parse().context("invalid entry price")?;
            let upnl: f64 = p
                .position
                .unrealized_pnl
                .parse()
                .context("invalid unrealized PnL")?;
            let lev: f64 = p
                .position
                .leverage
                .value
                .parse()
                .context("invalid leverage")?;
            anyhow::ensure!(
                entry.is_finite()
                    && entry > 0.0
                    && upnl.is_finite()
                    && lev.is_finite()
                    && lev > 0.0,
                "invalid account position"
            );
            out.push(VenuePosition {
                symbol: p.position.coin,
                size: sz,
                entry_price: entry,
                unrealized_pnl: upnl,
                leverage: lev,
            });
        }
        Ok(out)
    }

    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        anyhow::ensure!(self.live_authorized, "ENABLE_LIVE_TRADING=false");
        anyhow::ensure!(
            self.max_slippage_bps.is_finite()
                && self.max_slippage_bps > 0.0
                && self.max_slippage_bps <= 1000.0,
            "MAX_SLIPPAGE_BPS must be in (0, 1000]"
        );
        let dp = self.sz_decimals_for(&intent.symbol).await?;
        let raw_px = match intent.limit_price {
            Some(p) if p.is_finite() && p > 0.0 => p,
            Some(_) => anyhow::bail!("invalid limit price"),
            None => {
                let mids: serde_json::Value = self.info(json!({"type":"allMids"})).await?;
                let mid: f64 = mids
                    .get(&intent.symbol)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| anyhow!("missing mid"))?;
                anyhow::ensure!(mid.is_finite() && mid > 0.0, "invalid mid");
                let slip = self.max_slippage_bps / 10_000.0;
                mid * if intent.side.is_buy() {
                    1.0 + slip
                } else {
                    1.0 - slip
                }
            }
        };
        let limit_px = round_perp_price(raw_px, dp, intent.side.is_buy());
        let sz = round_to_decimals(intent.size, dp);
        anyhow::ensure!(
            limit_px.is_finite() && limit_px > 0.0 && sz > 0.0,
            "order rounds to zero"
        );
        self.verify_identity().await?;
        self.ensure_exchange().await?;
        let guard = self.exchange.lock().await;
        let client = guard.as_ref().unwrap();
        let order = hyperliquid_rust_sdk::ClientOrderRequest {
            asset: intent.symbol.clone(),
            is_buy: intent.side.is_buy(),
            reduce_only: intent.reduce_only,
            limit_px,
            sz,
            cloid: Some(intent.id.parse().context("invalid client order UUID")?),
            order_type: hyperliquid_rust_sdk::ClientOrder::Limit(
                hyperliquid_rust_sdk::ClientLimit {
                    tif: "Ioc".to_string(),
                },
            ),
        };
        let resp = client
            .order(order, None)
            .await
            .map_err(|e| anyhow!("hyperliquid order error: {e}"))?;

        parse_order_response(resp, &intent.id)
    }

    async fn close_position(
        &self,
        symbol: &str,
        size: f64,
        side_is_buy_to_close: bool,
    ) -> Result<OrderResult> {
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: symbol.to_string(),
            side: if side_is_buy_to_close {
                crate::core::Side::Long
            } else {
                crate::core::Side::Short
            },
            size: size.abs(),
            limit_price: None,
            reduce_only: true,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: Utc::now(),
        };
        self.submit_order(&intent).await
    }

    async fn account_equity_usd(&self) -> Result<f64> {
        let addr = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing; equity unavailable"))?;
        let mode: String = self
            .info(json!({"type":"userAbstraction","user":addr}))
            .await?;
        anyhow::ensure!(
            mode == "disabled",
            "account mode {mode} has no verified collateral model yet; new exposure blocked"
        );
        let state: ClearingHouseState = self
            .info(json!({"type":"clearinghouseState","user":addr}))
            .await?;
        crate::account::standard_perp_equity(&mode, &state.margin_summary.account_value)
    }

    async fn risk_snapshot(&self) -> Result<crate::aegis::AccountSnapshot> {
        use rust_decimal::Decimal;
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account missing"))?;
        let started = Utc::now().timestamp_millis();
        let mode: String = self
            .info(json!({"type":"userAbstraction","user":account}))
            .await?;
        anyhow::ensure!(mode == "disabled", "unsupported collateral model for Aegis");
        let state: ClearingHouseState = self
            .info(json!({"type":"clearinghouseState","user":account}))
            .await?;
        let orders: Vec<RiskOpenOrder> = self
            .info(json!({"type":"frontendOpenOrders","user":account}))
            .await?;
        let mids: std::collections::BTreeMap<String, String> =
            self.info(json!({"type":"allMids"})).await?;
        let equity = Decimal::from_str(&state.margin_summary.account_value)?;
        anyhow::ensure!(equity >= Decimal::ZERO, "invalid equity");
        let mut exposure = std::collections::BTreeMap::<String, Decimal>::new();
        for position in state.asset_positions {
            let size = Decimal::from_str(&position.position.szi)?.abs();
            let symbol = position.position.coin;
            let price = Decimal::from_str(
                mids.get(&symbol)
                    .ok_or_else(|| anyhow!("missing account position mark"))?,
            )?;
            anyhow::ensure!(price > Decimal::ZERO, "invalid mark");
            let notional = size
                .checked_mul(price)
                .ok_or_else(|| anyhow!("exposure overflow"))?;
            let prior = exposure.entry(symbol).or_default();
            *prior = prior
                .checked_add(notional)
                .ok_or_else(|| anyhow!("exposure overflow"))?;
        }
        for order in orders {
            anyhow::ensure!(
                order.children.is_empty(),
                "nested open orders require explicit risk reconciliation"
            );
            if order.reduce_only {
                continue;
            }
            let qty = Decimal::from_str(&order.sz)?;
            let price = Decimal::from_str(&order.limit_px)?;
            let mark = Decimal::from_str(
                mids.get(&order.coin)
                    .ok_or_else(|| anyhow!("unsupported open-order instrument"))?,
            )?;
            anyhow::ensure!(
                qty > Decimal::ZERO && price > Decimal::ZERO && mark > Decimal::ZERO,
                "invalid pending order"
            );
            let value = qty
                .checked_mul(price.max(mark))
                .ok_or_else(|| anyhow!("pending exposure overflow"))?;
            let prior = exposure.entry(order.coin).or_default();
            *prior = prior
                .checked_add(value)
                .ok_or_else(|| anyhow!("pending exposure overflow"))?;
        }
        anyhow::ensure!(
            Utc::now().timestamp_millis() - started <= 15_000,
            "account snapshot acquisition stale"
        );
        Ok(crate::aegis::AccountSnapshot {
            revision: started,
            observed_at_ms: started,
            equity,
            exposure_by_market: exposure,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RiskOpenOrder {
    coin: String,
    sz: String,
    limit_px: String,
    reduce_only: bool,
    #[serde(default)]
    children: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawCandle {
    t: i64,
    #[serde(deserialize_with = "flex_string::deserialize")]
    o: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    h: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    l: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    c: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    v: String,
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
    #[serde(deserialize_with = "flex_string::deserialize")]
    px: String,
    #[allow(dead_code)]
    #[serde(deserialize_with = "flex_string::deserialize")]
    sz: String,
    #[allow(dead_code)]
    n: u64,
}
#[derive(Deserialize)]
struct BookResp {
    levels: Vec<Vec<BookLevel>>,
}

#[cfg(test)]
#[derive(Deserialize)]
struct SpotBalance {
    coin: String,
    #[serde(default, deserialize_with = "flex_string::deserialize")]
    total: String,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "flex_string::deserialize")]
    hold: String,
    #[allow(dead_code)]
    #[serde(default)]
    token: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearingHouseState {
    #[serde(default)]
    asset_positions: Vec<AssetPosition>,
    margin_summary: MarginSummary,
}
#[derive(Deserialize)]
struct AssetPosition {
    position: AssetPositionInner,
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetPositionInner {
    coin: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    szi: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    entry_px: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    unrealized_pnl: String,
    leverage: Leverage,
}
#[derive(Deserialize)]
struct Leverage {
    #[serde(deserialize_with = "flex_string::deserialize")]
    value: String,
    #[allow(dead_code)]
    #[serde(default)]
    r#type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginSummary {
    #[serde(deserialize_with = "flex_string::deserialize")]
    account_value: String,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "flex_string_opt::deserialize")]
    total_margin_used: Option<String>,
    #[allow(dead_code)]
    #[serde(default, deserialize_with = "flex_string_opt::deserialize")]
    total_ntl_pos: Option<String>,
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

    fn response(statuses: serde_json::Value) -> hyperliquid_rust_sdk::ExchangeResponseStatus {
        serde_json::from_value(
            json!({"status":"ok","response":{"type":"order","data":{"statuses":statuses}}}),
        )
        .unwrap()
    }

    #[test]
    fn typed_fills_never_substitute_requested_quantity() {
        let result = parse_order_response(
            response(json!([{"filled":{"totalSz":"0.25","avgPx":"123.4","oid":42}}])),
            "id",
        )
        .unwrap();
        assert_eq!(result.filled_size, 0.25);
        assert_eq!(result.avg_fill_price, Some(123.4));
        assert_eq!(result.venue_order_id.as_deref(), Some("42"));
    }

    #[test]
    fn malformed_unknown_and_multi_status_acknowledgments_are_unresolved() {
        for statuses in [
            json!([]),
            json!(["success"]),
            json!(["waitingForFill"]),
            json!([{"filled":{"totalSz":"NaN","avgPx":"123","oid":42}}]),
            json!([{"filled":{"totalSz":"1","avgPx":"0","oid":42}}]),
            json!([{"resting":{"oid":1}},{"resting":{"oid":2}}]),
        ] {
            assert!(parse_order_response(response(statuses), "id").is_err());
        }
    }

    #[test]
    fn typed_resting_and_rejection_are_not_fills() {
        let resting =
            parse_order_response(response(json!([{"resting":{"oid":42}}])), "id").unwrap();
        assert!(resting.accepted);
        assert_eq!(resting.filled_size, 0.0);
        let rejected = parse_order_response(response(json!([{"error":"rejected"}])), "id").unwrap();
        assert!(!rejected.accepted);
        assert_eq!(rejected.filled_size, 0.0);
    }

    #[test]
    fn perp_ticks_respect_asset_decimals_and_never_widen_slippage_bound() {
        assert_eq!(round_perp_price(0.012345, 5, true), 0.0);
        assert_eq!(round_perp_price(1234.56, 2, true), 1234.5);
        assert_eq!(round_perp_price(1234.56, 2, false), 1234.6);
        for dp in 0..=6 {
            for px in [0.012345, 1.23456, 123.456, 123456.7] {
                assert!(round_perp_price(px, dp, true) <= px + 1e-10);
                assert!(round_perp_price(px, dp, false) >= px - 1e-10);
            }
        }
    }

    #[tokio::test]
    async fn disabled_adapter_and_missing_metadata_fail_without_network() {
        let adapter = HyperliquidAdapter {
            http: reqwest::Client::new(),
            live_authorized: false,
            max_slippage_bps: 30.0,
            base_url: "http://127.0.0.1:1".into(),
            account_address: None,
            exchange: Arc::new(Mutex::new(None)),
            wallet_key: None,
            identity_verified_at: Arc::new(Mutex::new(None)),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        assert!(adapter.sz_decimals_for("ETH").await.is_err());
        assert!(adapter
            .close_position("ETH", 1.0, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("ENABLE_LIVE_TRADING"));
    }

    #[test]
    fn leverage_decodes_when_value_is_integer() {
        // Observed live-market payload (TP hit on Feb 2026).
        let raw = serde_json::json!({"value": 20, "type": "cross"});
        let lev: Leverage =
            serde_json::from_value(raw).expect("leverage should decode from integer");
        assert_eq!(lev.value, "20");
    }

    #[test]
    fn leverage_decodes_when_value_is_string() {
        let raw = serde_json::json!({"value": "10", "type": "isolated"});
        let lev: Leverage =
            serde_json::from_value(raw).expect("leverage should decode from string");
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
        let p: AssetPositionInner =
            serde_json::from_value(raw).expect("mixed-type position decode");
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
        assert!((c.open - 100.0).abs() < 1e-9);
        assert!((c.high - 101.0).abs() < 1e-9);
        assert!((c.low - 99.5).abs() < 1e-9);
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

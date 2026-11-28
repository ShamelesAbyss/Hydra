//! Hyperliquid mainnet adapter (testnet removed, mainnet-only per user requirement).

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde_json::json;
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

type AssetMetaEntry = (u32, u8, Option<u32>);
type AssetMetaCache = Arc<Mutex<std::collections::HashMap<String, AssetMetaEntry>>>;

pub struct HyperliquidAdapter {
    http: reqwest::Client,
    live_authorized: bool,
    transport_class: crate::live_gate::VenueTransportClass,
    max_slippage_bps: f64,
    base_url: String,
    account_address: Option<String>,
    signer: Option<Arc<crate::treasury::EngineSignerAuthority>>,
    identity_verified_at: Arc<Mutex<Option<std::time::Instant>>>,
    /// v1.4.0: per-asset `(asset_index, sz_decimals, max_leverage)` fetched from
    /// `/info {"type":"meta"}`. Hyperliquid rejects sizes with extra decimals and
    /// enforces per-asset leverage ceilings.
    sz_decimals: AssetMetaCache,
    /// Shared restricted-pilot authority evaluated at the final signing/transmission boundary.
    pilot_authority: Arc<std::sync::Mutex<Option<crate::live_gate::PilotAuthorityCell>>>,
    #[cfg(test)]
    test_submissions: Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    test_nonce_root: Option<tempfile::TempDir>,
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

    pub fn new_mainnet_ws_url(&self) -> &'static str {
        if self.transport_class.is_testnet() {
            "wss://api.hyperliquid-testnet.xyz/ws"
        } else {
            "wss://api.hyperliquid.xyz/ws"
        }
    }

    pub fn ws_url(&self) -> &'static str {
        self.new_mainnet_ws_url()
    }

    /// Construct a Hyperliquid Testnet sandbox adapter (`TestnetSandbox`).
    /// Structurally isolated from `ProductionMainnet` (EIP-712 source `"b"`, testnet endpoint);
    /// requires explicit `account_address` + disposable testnet key (never mainnet production keys).
    pub async fn new_testnet(cfg: &AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let mut account_address = crate::ownership::configured_account(cfg)?;
        let signer = if let Some(raw_key) = cfg
            .hl_private_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            let auth = crate::treasury::EngineSignerAuthority::from_disposable_sandbox_key(
                account_address.as_deref(),
                raw_key,
            )?;
            if let Ok(Some(prod_profile)) =
                crate::treasury::TreasuryStore::default_store().and_then(|s| s.load_profile())
            {
                anyhow::ensure!(
                    !auth.signer_address().eq_ignore_ascii_case(&prod_profile.derived_signer_address)
                        && !auth.wallet_address().eq_ignore_ascii_case(&prod_profile.dedicated_wallet_address),
                    "TestnetSandbox refused: disposable key must not match configured ProductionMainnet Treasury identity"
                );
            }
            if account_address.is_none() {
                account_address = Some(auth.wallet_address().to_string());
            }
            Some(Arc::new(auth))
        } else {
            None
        };
        let adapter = Self {
            http,
            live_authorized: cfg.live_trading,
            transport_class: crate::live_gate::VenueTransportClass::TestnetSandbox,
            max_slippage_bps: cfg.risk.max_slippage_bps,
            base_url: "https://api.hyperliquid-testnet.xyz".to_string(),
            account_address,
            signer,
            identity_verified_at: Arc::new(Mutex::new(None)),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pilot_authority: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            test_nonce_root: None,
        };
        adapter.warm_meta().await;
        Ok(adapter)
    }

    pub async fn new(cfg: &AppConfig) -> Result<Self> {
        let signer = if cfg.live_trading {
            let req = crate::live_gate::LiveAuthorizationRequest::from_treasury_store(
                crate::live_gate::ExecutionChannel::VenueAdapterDirect,
                cfg.live_trading,
                true,
            );
            // Construction confers no entry authority: entries additionally require an attached
            // ProductionMainnet grant at the final signing boundary.
            crate::live_gate::require_live_runtime_construction(&req)?;
            let store = crate::treasury::TreasuryStore::default_store()?;
            let auth = store
                .open_engine_signer(crate::treasury::SigningAccessScope::EngineExclusive, false)?;
            Some(Arc::new(auth))
        } else {
            crate::treasury::TreasuryStore::default_store()
                .ok()
                .and_then(|store| {
                    store
                        .open_engine_signer(
                            crate::treasury::SigningAccessScope::EngineExclusive,
                            false,
                        )
                        .ok()
                })
                .map(Arc::new)
        };
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let mut account_address = crate::ownership::configured_account(cfg)?;
        if account_address.is_none() {
            account_address = signer.as_ref().map(|s| s.wallet_address().to_string());
        }
        tracing::info!(network="mainnet", account=?account_address, "hyperliquid adapter init");
        let adapter = Self {
            http,
            live_authorized: cfg.live_trading,
            transport_class: crate::live_gate::VenueTransportClass::ProductionMainnet,
            max_slippage_bps: cfg.risk.max_slippage_bps,
            base_url: defaults::HYPERLIQUID_API.to_string(),
            account_address,
            signer,
            identity_verified_at: Arc::new(Mutex::new(None)),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pilot_authority: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            test_nonce_root: None,
        };
        // Prefetch metadata; orders fail closed if the instrument is unavailable.
        adapter.warm_meta().await;
        Ok(adapter)
    }

    /// Fetch the venue meta (universe → szDecimals + maxLeverage per asset) and cache it.
    /// Idempotent: subsequent calls refresh the cache without disrupting open
    /// trades. Logged at INFO so the operator can confirm HYPE precision in
    /// the cockpit feed.
    async fn warm_meta(&self) {
        #[derive(Deserialize)]
        struct AssetMeta {
            name: String,
            #[serde(rename = "szDecimals")]
            sz_decimals: u8,
            #[serde(default, rename = "maxLeverage")]
            max_leverage: Option<u32>,
        }
        #[derive(Deserialize)]
        struct Universe {
            universe: Vec<AssetMeta>,
        }
        match self.info::<Universe>(json!({"type":"meta"})).await {
            Ok(meta) => {
                let mut guard = self.sz_decimals.lock().await;
                guard.clear();
                for (index, a) in meta.universe.iter().enumerate() {
                    guard.insert(
                        a.name.clone(),
                        (index as u32, a.sz_decimals, a.max_leverage),
                    );
                }
                let count = guard.len();
                let hype = guard.get("HYPE").map(|value| value.1);
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
            .map(|value| value.1)
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
                Ok(resp) if resp.status().is_success() => match read_info_response(resp).await {
                    Ok(txt) => match serde_json::from_str::<T>(&txt) {
                        Ok(v) => return Ok(v),
                        Err(e) => {
                            last_err = Some(anyhow!(
                                "decoding hyperliquid info response: {e}; body={}",
                                info_diagnostic_preview(&txt)
                            ))
                        }
                    },
                    Err(e) => last_err = Some(e),
                },
                Ok(resp) => {
                    let s = resp.status();
                    let txt = read_info_response(resp).await.unwrap_or_default();
                    last_err = Some(anyhow!(
                        "hyperliquid info {s}: {}",
                        info_diagnostic_preview(&txt)
                    ));
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

    async fn submit_order_impl(
        &self,
        intent: &OrderIntent,
        journal: Option<&crate::submission_wire::SubmissionJournal<'_>>,
    ) -> Result<super::OrderReceipt> {
        anyhow::ensure!(self.live_authorized, "ENABLE_LIVE_TRADING=false");
        let production = self.transport_class.is_production()
            || crate::live_gate::is_production_exchange_url(&self.base_url);
        if production && intent.reduce_only {
            crate::live_gate::require_risk_reduction_authority(
                crate::live_gate::ExecutionChannel::VenueAdapterDirect,
                self.live_authorized,
            )?;
        }
        if let Some(journal) = journal {
            journal.verify_intent(intent)?;
        }
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
        // Final exposure-increasing authority on the exact rounded order (deny-by-default on
        // production; re-evaluated at Utc::now() before signing and again before transmission).
        let entry_guard: Option<Box<dyn Fn() -> Result<()> + Send + Sync>> = if intent.reduce_only {
            None
        } else {
            let cell = self.pilot_authority.lock().unwrap().clone();
            let account = self.account_address.clone().unwrap_or_default();
            let signer_addr = self
                .signer
                .as_ref()
                .map(|s| s.signer_address().to_string())
                .unwrap_or_default();
            let symbol = intent.symbol.clone();
            let strategy = intent.source_signal.clone().unwrap_or_default();
            let notional = rust_decimal::Decimal::try_from(limit_px * sz)
                .map_err(|_| anyhow!("rounded entry notional not representable"))?;
            let fee = (notional * rust_decimal::Decimal::new(5, 4)).round_dp(6);
            Some(Box::new(move || {
                let suspended = crate::live_gate::LiveAuthorizationRequest::from_treasury_store(
                    crate::live_gate::ExecutionChannel::VenueAdapterDirect,
                    true,
                    false,
                )
                .emergency_suspended;
                crate::live_gate::require_entry_mutation_authority(
                    cell.as_ref(),
                    &crate::live_gate::EntryMutationContext {
                        production,
                        account: &account,
                        signer: &signer_addr,
                        symbol: &symbol,
                        strategy_id: &strategy,
                        order_type: crate::live_gate::PilotOrderType::LimitIoc,
                        requested_leverage: rust_decimal::Decimal::ONE,
                        requested_notional_usd: notional + fee,
                        current_exposure_usd: rust_decimal::Decimal::ZERO,
                        emergency_suspended: suspended,
                    },
                    Utc::now().timestamp_millis(),
                )
            }))
        };
        if let Some(g) = entry_guard.as_ref() {
            g()?;
        }
        self.verify_identity().await?;
        let action = crate::hyperliquid_wire::OrderAction::ioc(
            self.asset_id_for(&intent.symbol).await?,
            intent.side.is_buy(),
            limit_px,
            sz,
            intent.reduce_only,
            &intent.id,
        )?;
        if let Some(journal) = journal {
            let account = self
                .account_address
                .as_deref()
                .ok_or_else(|| anyhow!("native wire account missing"))?;
            journal.record(account, &action, dp)?;
        }
        let signer_auth = self
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("EngineSignerAuthority missing"))?;
        #[cfg(not(test))]
        let resp = {
            let mainnet = self.transport_class.is_production();
            let guard: Option<crate::hyperliquid_wire::PreMutationGuard<'_>> = entry_guard
                .as_deref()
                .map(|g| g as crate::hyperliquid_wire::PreMutationGuard<'_>);
            match journal {
                Some(journal) => {
                    crate::hyperliquid_wire::submit_ioc_journaled_with_signer(
                        signer_auth,
                        &action,
                        journal,
                        mainnet,
                        guard,
                    )
                    .await?
                }
                None => {
                    crate::hyperliquid_wire::submit_ioc_with_signer(
                        signer_auth,
                        &action,
                        mainnet,
                        guard,
                    )
                    .await?
                }
            }
        };
        #[cfg(test)]
        let resp = {
            let _ = signer_auth;
            if let Some(g) = entry_guard.as_ref() {
                g()?;
            }
            let root = self
                .test_nonce_root
                .as_ref()
                .ok_or_else(|| anyhow!("offline nonce root required"))?;
            let signer = self
                .account_address
                .as_deref()
                .ok_or_else(|| anyhow!("offline public signer required"))?;
            let nonce = crate::hyperliquid_wire::reserve_nonce_at(
                crate::signer_nonce::NonceCoordinator::at_root(root.path()),
                signer,
            )
            .await?;
            if let Some(journal) = journal {
                journal.assert_persisted_action(&action)?;
                journal.record_signing_preparation(signer, nonce, &action)?;
                journal.assert_persisted_preparation(&action)?;
            }
            self.test_submissions
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (quantity, price, _) = action.fill_bounds()?;
            serde_json::from_value(
                json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":42,"totalSz":quantity.to_string(),"avgPx":price.to_string()}}]}}}),
            )?
        };

        parse_order_receipt(resp, intent, &action, dp)
    }

    async fn verify_identity(&self) -> Result<()> {
        let mut verified = self.identity_verified_at.lock().await;
        if verified.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(30)) {
            return Ok(());
        }
        let signer_auth = self
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("EngineSignerAuthority missing"))?;
        let signer = signer_auth.signer_address();
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing"))?;
        let signer_role = self.info(json!({"type":"userRole","user":signer})).await?;
        let account_role = self.info(json!({"type":"userRole","user":account})).await?;
        crate::account::verify_signer(signer, account, &signer_role, &account_role)?;
        *verified = Some(std::time::Instant::now());
        Ok(())
    }

    async fn asset_id_for(&self, symbol: &str) -> Result<u32> {
        self.sz_decimals
            .lock()
            .await
            .get(symbol)
            .map(|value| value.0)
            .ok_or_else(|| anyhow!("metadata unavailable for {symbol}"))
    }

    async fn fetch_symbol_funding_bps_1h(&self, symbol: &str) -> Result<Option<f64>> {
        #[derive(Deserialize)]
        struct MetaAsset {
            name: String,
        }
        #[derive(Deserialize)]
        struct MetaUniverse {
            universe: Vec<MetaAsset>,
        }
        #[derive(Deserialize)]
        struct AssetCtx {
            #[serde(default, deserialize_with = "flex_string_opt::deserialize")]
            funding: Option<String>,
        }
        let pair: (MetaUniverse, Vec<AssetCtx>) =
            self.info(json!({"type":"metaAndAssetCtxs"})).await?;
        let Some(idx) = pair.0.universe.iter().position(|a| a.name == symbol) else {
            return Ok(None);
        };
        let Some(ctx) = pair.1.get(idx) else {
            return Ok(None);
        };
        let Some(raw) = ctx.funding.as_deref() else {
            return Ok(None);
        };
        let rate: f64 = raw.parse()?;
        if !rate.is_finite() {
            return Ok(None);
        }
        Ok(Some(rate * 10_000.0))
    }
}

fn info_diagnostic_preview(value: &str) -> String {
    crate::telemetry::redact_diagnostic(value)
        .chars()
        .take(256)
        .collect()
}

/// Bound decoded response bytes before JSON allocation, including compressed
/// responses. Read-only retries keep the existing bounded backoff policy.
async fn read_info_response(response: reqwest::Response) -> Result<String> {
    read_info_response_limited(response, 4 * 1024 * 1024).await
}
async fn read_info_response_limited(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<String> {
    anyhow::ensure!(
        max_bytes > 0 && max_bytes <= 4 * 1024 * 1024,
        "invalid info response byte budget"
    );
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= max_bytes as u64,
            "info response exceeds byte budget"
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes
                .len()
                .checked_add(chunk.len())
                .is_some_and(|len| len <= max_bytes),
            "info response exceeds byte budget"
        );
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| anyhow!("info response is not valid UTF-8"))
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
    resp: crate::hyperliquid_wire::ExchangeResponseStatus,
    id: &str,
) -> Result<OrderResult> {
    use crate::hyperliquid_wire::{
        ExchangeDataStatus as Status, ExchangeResponseStatus as Response,
    };
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

fn parse_order_receipt(
    resp: crate::hyperliquid_wire::ExchangeResponseStatus,
    intent: &OrderIntent,
    action: &crate::hyperliquid_wire::OrderAction,
    dp: u8,
) -> Result<super::OrderReceipt> {
    let precise_fill = match &resp {
        crate::hyperliquid_wire::ExchangeResponseStatus::Ok(response) => {
            response.data.as_ref().and_then(|data| {
                if data.statuses.len() == 1 {
                    match &data.statuses[0] {
                        crate::hyperliquid_wire::ExchangeDataStatus::Filled(fill) => {
                            Some(crate::execution_facts::PreciseFill {
                                quantity: fill.total_sz.clone(),
                                average_price: fill.avg_px.clone(),
                                order_id: fill.oid.to_string(),
                            })
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            })
        }
        _ => None,
    };
    let result = parse_order_response(resp, &intent.id)?;
    if let Some(facts) = &precise_fill {
        facts.validate(intent, &result)?;
        let (maximum, limit, buy) = action.fill_bounds()?;
        facts.validate_wire_bounds(maximum, limit, buy, dp)?;
    }
    Ok(super::OrderReceipt {
        result,
        precise_fill,
    })
}

#[async_trait]
impl VenueAdapter for HyperliquidAdapter {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }

    fn is_production_venue(&self) -> bool {
        self.transport_class.is_production()
            || crate::live_gate::is_production_exchange_url(&self.base_url)
    }

    fn attach_pilot_authority(&self, cell: crate::live_gate::PilotAuthorityCell) {
        *self.pilot_authority.lock().unwrap() = Some(cell);
    }

    fn account_identity(&self) -> Option<(String, String)> {
        let account = self.account_address.clone()?;
        let signer = self.signer.as_ref()?.signer_address().to_string();
        Some((account, signer))
    }

    async fn fetch_account_fills(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::ledger::ExchangeFill>> {
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing; fills unavailable"))?;
        anyhow::ensure!(start_ms >= 0 && end_ms > start_ms, "invalid fill window");
        let mut out = Vec::new();
        let mut cursor = start_ms;
        for _ in 0..64 {
            let raw: serde_json::Value = self
                .info(json!({"type":"userFillsByTime","user":account,"startTime":cursor,"endTime":end_ms,"aggregateByTime":false}))
                .await?;
            let len = raw.as_array().map_or(0, Vec::len);
            let fills = crate::ledger_hyperliquid::parse_fills(&serde_json::to_string(&raw)?)?;
            let times: Vec<i64> = fills.iter().map(|f| f.timestamp_ms).collect();
            out.extend(fills);
            match crate::history::next_cursor(cursor, end_ms, &times, len)? {
                None => return Ok(out),
                Some(next) => cursor = next,
            }
        }
        anyhow::bail!("fill history page budget exceeded; settlement coverage incomplete")
    }

    async fn fetch_account_funding(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::ledger::FundingPayment>> {
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing; funding unavailable"))?;
        anyhow::ensure!(start_ms >= 0 && end_ms > start_ms, "invalid funding window");
        let mut out = Vec::new();
        let mut cursor = start_ms;
        for _ in 0..64 {
            let raw: serde_json::Value = self
                .info(json!({"type":"userFunding","user":account,"startTime":cursor,"endTime":end_ms}))
                .await?;
            let len = raw.as_array().map_or(0, Vec::len);
            let rows = crate::ledger_hyperliquid::parse_funding(&serde_json::to_string(&raw)?)?;
            let times: Vec<i64> = rows.iter().map(|f| f.timestamp_ms).collect();
            out.extend(rows);
            match crate::history::next_cursor(cursor, end_ms, &times, len)? {
                None => return Ok(out),
                Some(next) => cursor = next,
            }
        }
        anyhow::bail!("funding history page budget exceeded; settlement coverage incomplete")
    }

    async fn fetch_account_cash_flows(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::settlement::ExternalCashFlow>> {
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account identity missing; cash flows unavailable"))?;
        anyhow::ensure!(
            start_ms >= 0 && end_ms > start_ms,
            "invalid cash-flow window"
        );
        let mut out = Vec::new();
        let mut cursor = start_ms;
        for _ in 0..64 {
            let raw: serde_json::Value = self
                .info(json!({"type":"userNonFundingLedgerUpdates","user":account,"startTime":cursor,"endTime":end_ms}))
                .await?;
            let len = raw.as_array().map_or(0, Vec::len);
            let rows = crate::ledger_hyperliquid::parse_non_funding_ledger_updates(
                &serde_json::to_string(&raw)?,
            )?;
            let times: Vec<i64> = rows.iter().map(|f| f.timestamp_ms).collect();
            out.extend(rows);
            match crate::history::next_cursor(cursor, end_ms, &times, len)? {
                None => return Ok(out),
                Some(next) => cursor = next,
            }
        }
        anyhow::bail!("cash-flow history page budget exceeded; settlement coverage incomplete")
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
        let (bid, ask, spread_bps, l2_depth_usd) = match book {
            Ok(b) => {
                let bid = b
                    .levels
                    .first()
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
                let mut depth_sum = 0.0f64;
                let mut valid_levels = 0usize;
                for side in b.levels.iter().take(2) {
                    for lv in side.iter().take(10) {
                        if let (Ok(px), Ok(sz)) = (lv.px.parse::<f64>(), lv.sz.parse::<f64>()) {
                            if px.is_finite() && px > 0.0 && sz.is_finite() && sz > 0.0 {
                                depth_sum += px * sz;
                                valid_levels += 1;
                            }
                        }
                    }
                }
                let depth = (valid_levels > 0 && depth_sum.is_finite() && depth_sum > 0.0)
                    .then_some(depth_sum);
                (bid, ask, sp, depth)
            }
            Err(_) => (None, None, None, None),
        };
        let funding_bps_1h = self
            .fetch_symbol_funding_bps_1h(symbol)
            .await
            .ok()
            .flatten();
        let observed_exchange_ts_ms = candles.last().map(|c| c.ts.timestamp_millis());
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.to_string(),
            mid,
            bid,
            ask,
            spread_bps,
            l2_depth_usd,
            funding_bps_1h,
            last_update: Utc::now(),
            observed_exchange_ts_ms,
            candles,
            microstructure: None,
        })
    }

    async fn instrument_constraints(
        &self,
    ) -> Result<
        std::collections::BTreeMap<String, crate::execution_recovery::InstrumentOrderConstraints>,
    > {
        if self.sz_decimals.lock().await.is_empty() {
            self.warm_meta().await;
        }
        let guard = self.sz_decimals.lock().await;
        let mut out = std::collections::BTreeMap::new();
        for (sym, &(_idx, dp, max_lev)) in guard.iter() {
            if let Some(lev) = max_lev.filter(|&v| v > 0) {
                let tick_decimals = 6u32.saturating_sub(dp as u32);
                out.insert(
                    sym.clone(),
                    crate::execution_recovery::InstrumentOrderConstraints {
                        symbol: sym.clone(),
                        tick_size: rust_decimal::Decimal::new(1, tick_decimals),
                        lot_size: rust_decimal::Decimal::new(1, dp as u32),
                        min_notional_usd: rust_decimal::Decimal::from(10),
                        max_leverage: rust_decimal::Decimal::from(lev),
                    },
                );
            }
        }
        Ok(out)
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

    async fn position_snapshot(
        &self,
    ) -> Result<Option<super::position_evidence::PositionSnapshot>> {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let account = self
                .account_address
                .as_ref()
                .ok_or_else(|| anyhow!("account missing"))?;
            let started = Utc::now().timestamp_millis();
            let state: serde_json::Value = self
                .info(json!({"type":"clearinghouseState","user":account,"dex":""}))
                .await?;
            Ok(Some(super::position_evidence::parse_hyperliquid(
                account,
                &serde_json::to_string(&state)?,
                started,
                Utc::now().timestamp_millis(),
            )?))
        })
        .await
        .map_err(|_| anyhow!("position evidence acquisition exceeded 15 seconds"))?
    }

    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        Ok(self.submit_order_with_evidence(intent).await?.result)
    }
    async fn submit_order_with_evidence(
        &self,
        intent: &OrderIntent,
    ) -> Result<super::OrderReceipt> {
        self.submit_order_impl(intent, None).await
    }
    async fn submit_order_journaled(
        &self,
        intent: &OrderIntent,
        journal: &crate::submission_wire::SubmissionJournal<'_>,
    ) -> Result<super::OrderReceipt> {
        self.submit_order_impl(intent, Some(journal)).await
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

    async fn account_exposure_evidence(
        &self,
    ) -> Result<super::account_evidence::AccountExposureEvidence> {
        let env = if self.transport_class.is_testnet() {
            "testnet"
        } else {
            "mainnet"
        };
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let account = self
                .account_address
                .as_ref()
                .ok_or_else(|| anyhow!("account missing"))?;
            let started = Utc::now().timestamp_millis();
            let mode: String = self
                .info(json!({"type":"userAbstraction","user":account}))
                .await?;
            anyhow::ensure!(mode == "disabled", "unsupported collateral model for Aegis");
            let state: serde_json::Value = self
                .info(json!({"type":"clearinghouseState","user":account,"dex":""}))
                .await?;
            let orders: serde_json::Value = self
                .info(json!({"type":"frontendOpenOrders","user":account,"dex":""}))
                .await?;
            let mids: serde_json::Value = self.info(json!({"type":"allMids","dex":""})).await?;
            super::account_evidence::parse_hyperliquid_for_env(
                account,
                env,
                &mode,
                &serde_json::to_string(&state)?,
                &serde_json::to_string(&orders)?,
                &serde_json::to_string(&mids)?,
                started,
                Utc::now().timestamp_millis(),
            )
        })
        .await
        .map_err(|_| anyhow!("account evidence acquisition exceeded 15 seconds"))?
    }

    async fn order_status_observation(
        &self,
        intent: &OrderIntent,
    ) -> Result<crate::order_status::Observation> {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let account = self
                .account_address
                .as_ref()
                .ok_or_else(|| anyhow!("account missing"))?;
            let request = crate::order_status::request_body(account, intent)?;
            let started = Utc::now().timestamp_millis();
            let response = self
                .http
                .post(format!("{}/info", self.base_url))
                .json(&request)
                .send()
                .await?;
            let status = response.status();
            let body = read_info_response_limited(response, 64 * 1024).await?;
            anyhow::ensure!(
                status.is_success(),
                "hyperliquid order status read failed: {status}"
            );
            crate::order_status::parse_hyperliquid(
                account,
                intent,
                &body,
                started,
                Utc::now().timestamp_millis(),
            )
        })
        .await
        .map_err(|_| anyhow!("order status acquisition exceeded 15 seconds"))?
    }

    async fn risk_snapshot(&self) -> Result<crate::aegis::AccountSnapshot> {
        Ok(self.account_exposure_evidence().await?.snapshot)
    }

    async fn open_exchange_orders(
        &self,
    ) -> Result<Vec<crate::execution_recovery::ExchangeOrderSnapshot>> {
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account missing"))?;
        let orders: Vec<OpenOrderWire> = self
            .info(json!({"type":"frontendOpenOrders","user":account,"dex":""}))
            .await?;
        let now_ms = Utc::now().timestamp_millis();
        let mut out = Vec::with_capacity(orders.len());
        for o in orders {
            let sz: rust_decimal::Decimal = o.sz.parse().unwrap_or(rust_decimal::Decimal::ZERO);
            let orig: rust_decimal::Decimal = o.orig_sz.parse().unwrap_or(sz);
            let filled = (orig - sz).max(rust_decimal::Decimal::ZERO);
            let state = if filled > rust_decimal::Decimal::ZERO && sz > rust_decimal::Decimal::ZERO
            {
                crate::execution_recovery::OrderLifecycleState::PartiallyFilled
            } else {
                crate::execution_recovery::OrderLifecycleState::Acknowledged
            };
            out.push(crate::execution_recovery::ExchangeOrderSnapshot {
                source: crate::ledger::EvidenceSource::RestSnapshot,
                cloid: o.cloid,
                exchange_oid: o.oid,
                symbol: o.coin,
                side: if o.side.eq_ignore_ascii_case("B") {
                    crate::core::Side::Long
                } else {
                    crate::core::Side::Short
                },
                state,
                filled_qty: filled,
                avg_fill_px: o.limit_px.parse().ok(),
                observed_at_ms: o.timestamp.unwrap_or(now_ms),
            });
        }
        Ok(out)
    }

    async fn list_protective_stops(
        &self,
    ) -> Result<Vec<crate::protective_orders::ProtectiveOrderSpec>> {
        let account = self
            .account_address
            .as_ref()
            .ok_or_else(|| anyhow!("account missing"))?;
        let orders: Vec<OpenOrderWire> = self
            .info(json!({"type":"frontendOpenOrders","user":account,"dex":""}))
            .await?;
        let mut out = Vec::new();
        for o in orders {
            if !o.reduce_only {
                continue;
            }
            let Some(trig_str) = o.trigger_px else {
                continue;
            };
            let Ok(trig) = trig_str.parse::<rust_decimal::Decimal>() else {
                continue;
            };
            let Ok(qty) = o.sz.parse::<rust_decimal::Decimal>() else {
                continue;
            };
            let protective_side = if o.side.eq_ignore_ascii_case("B") {
                crate::core::Side::Long
            } else {
                crate::core::Side::Short
            };
            let position_side = match protective_side {
                crate::core::Side::Long => crate::core::Side::Short,
                crate::core::Side::Short => crate::core::Side::Long,
            };
            out.push(crate::protective_orders::ProtectiveOrderSpec {
                symbol: o.coin,
                owner_strategy: "exchange-native".into(),
                position_side,
                protective_side,
                qty,
                trigger_price: trig,
                reduce_only: true,
                cloid: o.cloid.unwrap_or_else(|| format!("oid-{}", o.oid)),
                exchange_oid: Some(o.oid),
            });
        }
        Ok(out)
    }

    async fn place_protective_stop(
        &self,
        spec: &crate::protective_orders::ProtectiveOrderSpec,
    ) -> Result<crate::protective_orders::ProtectiveOrderSpec> {
        anyhow::ensure!(
            spec.reduce_only,
            "protective stop placement MUST enforce reduce_only=true"
        );
        if self.transport_class.is_production()
            || crate::live_gate::is_production_exchange_url(&self.base_url)
        {
            crate::live_gate::require_risk_reduction_authority(
                crate::live_gate::ExecutionChannel::VenueAdapterDirect,
                self.live_authorized,
            )?;
        }
        let dp = self.sz_decimals_for(&spec.symbol).await?;
        let asset = self.asset_id_for(&spec.symbol).await?;
        let trig_f: f64 = spec.trigger_price.to_string().parse()?;
        let qty_f = round_to_decimals(spec.qty.to_string().parse()?, dp);
        anyhow::ensure!(
            trig_f > 0.0 && qty_f > 0.0,
            "protective stop rounds to zero"
        );
        let buy_to_close = spec.protective_side.is_buy();
        let slip = (self.max_slippage_bps / 10_000.0).clamp(0.001, 0.10);
        let exec_px = round_perp_price(
            trig_f * if buy_to_close { 1.0 + slip } else { 1.0 - slip },
            dp,
            buy_to_close,
        );
        let trig_px = round_perp_price(trig_f, dp, buy_to_close);
        let action = crate::hyperliquid_wire::TriggerOrderAction::reduce_only_stop(
            asset,
            buy_to_close,
            trig_px,
            exec_px,
            qty_f,
            &spec.cloid,
        )?;
        let signer_auth = self
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("EngineSignerAuthority missing for protective stop"))?;
        let mainnet = self.transport_class.is_production();
        let resp =
            crate::hyperliquid_wire::submit_trigger_stop_with_signer(signer_auth, &action, mainnet)
                .await?;
        let parsed = parse_order_response(resp, &spec.cloid)?;
        anyhow::ensure!(
            parsed.accepted,
            "exchange did not accept protective stop: {}",
            parsed.message
        );
        let mut acked = spec.clone();
        acked.exchange_oid = parsed.venue_order_id.and_then(|s| s.parse().ok());
        Ok(acked)
    }

    async fn cancel_protective_stop(
        &self,
        symbol: &str,
        exchange_oid: Option<u64>,
        _cloid: &str,
    ) -> Result<()> {
        let oid = exchange_oid.ok_or_else(|| anyhow!("exchange_oid required to cancel stop"))?;
        if self.transport_class.is_production()
            || crate::live_gate::is_production_exchange_url(&self.base_url)
        {
            crate::live_gate::require_risk_reduction_authority(
                crate::live_gate::ExecutionChannel::VenueAdapterDirect,
                self.live_authorized,
            )?;
        }
        let asset = self.asset_id_for(symbol).await?;
        let signer_auth = self
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("EngineSignerAuthority missing for protective cancel"))?;
        let cancel = crate::hyperliquid_wire::CancelAction::by_oid(asset, oid)?;
        let resp = crate::hyperliquid_wire::submit_cancel_by_oid_with_signer(
            signer_auth,
            &cancel,
            self.transport_class.is_production(),
        )
        .await?;
        match resp {
            crate::hyperliquid_wire::ExchangeResponseStatus::Ok(_) => Ok(()),
            crate::hyperliquid_wire::ExchangeResponseStatus::Err(e) => {
                anyhow::bail!("exchange rejected protective cancel: {e}")
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenOrderWire {
    coin: String,
    side: String,
    #[serde(deserialize_with = "flex_string::deserialize")]
    sz: String,
    #[serde(default, deserialize_with = "flex_string::deserialize")]
    orig_sz: String,
    #[serde(default, deserialize_with = "flex_string::deserialize")]
    limit_px: String,
    oid: u64,
    #[serde(default)]
    cloid: Option<String>,
    #[serde(default)]
    reduce_only: bool,
    #[serde(default, deserialize_with = "flex_string_opt::deserialize")]
    trigger_px: Option<String>,
    #[serde(default)]
    timestamp: Option<i64>,
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

    #[test]
    fn info_failure_previews_are_utf8_bounded_and_sensitive_fields_are_redacted() {
        let preview = info_diagnostic_preview(&"界".repeat(300));
        assert_eq!(preview.chars().count(), 256);
        assert_eq!(
            info_diagnostic_preview("signature: PRIVATE_FIXTURE"),
            "[redacted sensitive diagnostic]"
        );
    }

    fn response(statuses: serde_json::Value) -> crate::hyperliquid_wire::ExchangeResponseStatus {
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
            transport_class: crate::live_gate::VenueTransportClass::LoopbackTestFixture,
            max_slippage_bps: 30.0,
            base_url: "http://127.0.0.1:1".into(),
            account_address: None,
            signer: None,
            identity_verified_at: Arc::new(Mutex::new(None)),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pilot_authority: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            test_nonce_root: None,
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
    #[tokio::test]
    async fn info_reader_bounds_declared_and_chunked_bodies_before_json_decoding() {
        use std::io::{Read, Write};
        for budget in [64 * 1024, 4 * 1024 * 1024] {
            for chunked in [false, true] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = listener.local_addr().unwrap();
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = [0u8; 4096];
                    let _ = stream.read(&mut request);
                    if chunked {
                        let _=write!(stream,"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",budget+1);
                        let _ = stream.write_all(&vec![b'x'; budget + 1]);
                        let _ = stream.write_all(b"\r\n0\r\n\r\n");
                    } else {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            budget + 1
                        );
                    }
                });
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap();
                let response = client
                    .get(format!("http://{address}"))
                    .send()
                    .await
                    .unwrap();
                let error = if budget == 4 * 1024 * 1024 {
                    read_info_response(response).await.unwrap_err()
                } else {
                    read_info_response_limited(response, budget)
                        .await
                        .unwrap_err()
                };
                assert!(error.to_string().contains("byte budget"), "{error}");
                server.join().unwrap();
            }
        }
    }
    #[test]
    fn native_receipt_preserves_decimal_strings_and_checks_wire_limits() {
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: crate::core::Side::Long,
            size: 2.0,
            limit_price: Some(101.0),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: Utc::now(),
        };
        let action =
            crate::hyperliquid_wire::OrderAction::ioc(4, true, 101.0, 2.0, false, &intent.id)
                .unwrap();
        let receipt = parse_order_receipt(
            response(
                json!([{"filled":{"oid":42,"totalSz":"1.0000","avgPx":"100.000000000000000001"}}]),
            ),
            &intent,
            &action,
            4,
        )
        .unwrap();
        let precise = receipt.precise_fill.unwrap();
        assert_eq!(precise.quantity, "1.0000");
        assert_eq!(precise.average_price, "100.000000000000000001");
        assert_eq!(receipt.result.avg_fill_price, Some(100.0));
        for (qty, price, oid) in [
            ("1.00001", "100", 42),
            ("2.00000000000000000001", "100", 42),
            ("1", "101.00000000000000000001", 42),
            ("1", "100", 0),
        ] {
            assert!(parse_order_receipt(
                response(json!([{"filled":{"oid":oid,"totalSz":qty,"avgPx":price}}])),
                &intent,
                &action,
                4
            )
            .is_err());
        }
        let rejected = parse_order_receipt(
            response(json!([{"error":"offline rejection"}])),
            &intent,
            &action,
            4,
        )
        .unwrap();
        assert!(!rejected.result.accepted && rejected.precise_fill.is_none());
    }
    fn wire_fixture() -> (crate::storage::Storage, OrderIntent, HyperliquidAdapter) {
        let db = crate::storage::Storage::open(":memory:").unwrap();
        db.migrate().unwrap();
        db.bind_trading_account("0x1111111111111111111111111111111111111111", false)
            .unwrap();
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: crate::core::Side::Long,
            size: 1.234567,
            limit_price: Some(101.12345),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: Utc::now(),
        };
        db.reserve_intent(&intent).unwrap();
        let adapter = HyperliquidAdapter {
            http: reqwest::Client::new(),
            live_authorized: true,
            transport_class: crate::live_gate::VenueTransportClass::LoopbackTestFixture,
            max_slippage_bps: 30.0,
            base_url: "http://127.0.0.1:1".into(),
            account_address: Some("0x1111111111111111111111111111111111111111".into()),
            signer: Some(Arc::new(
                crate::treasury::EngineSignerAuthority::from_test_fixture(
                    "0x1111111111111111111111111111111111111111",
                    &format!("{:064x}", 1),
                )
                .unwrap(),
            )),
            identity_verified_at: Arc::new(Mutex::new(Some(std::time::Instant::now()))),
            sz_decimals: Arc::new(Mutex::new(std::collections::HashMap::from([(
                "ETH".into(),
                (4, 4, Some(20)),
            )]))),
            pilot_authority: Arc::new(std::sync::Mutex::new(None)),
            test_submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            test_nonce_root: Some(tempfile::tempdir().unwrap()),
        };
        (db, intent, adapter)
    }
    #[tokio::test]
    async fn journaled_native_sender_observes_actual_rounded_action_before_send() {
        let (db, intent, adapter) = wire_fixture();
        let journal = crate::submission_wire::SubmissionJournal::new(&db, &intent);
        let receipt = adapter
            .submit_order_journaled(&intent, &journal)
            .await
            .unwrap();
        assert_eq!(receipt.result.filled_size, 1.2345);
        assert_eq!(
            adapter
                .test_submissions
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let action = crate::hyperliquid_wire::OrderAction::ioc(
            4,
            true,
            round_perp_price(101.12345, 4, true),
            1.2345,
            false,
            &intent.id,
        )
        .unwrap();
        journal.assert_persisted_action(&action).unwrap();
        assert!(adapter
            .submit_order_journaled(&intent, &journal)
            .await
            .is_err());
        assert_eq!(
            adapter
                .test_submissions
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
    #[tokio::test]
    async fn native_wire_persistence_failure_prevents_sender() {
        let (db, intent, adapter) = wire_fixture();
        db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_native_wire BEFORE INSERT ON execution_wire_requests BEGIN SELECT RAISE(ABORT,'offline-wire-failure'); END;")).unwrap();
        let error = adapter
            .submit_order_journaled(
                &intent,
                &crate::submission_wire::SubmissionJournal::new(&db, &intent),
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("offline-wire-failure"),
            "{error}"
        );
        assert_eq!(
            adapter
                .test_submissions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        let count: i64 = db
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM execution_wire_requests", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(count, 0);
    }
    #[tokio::test]
    async fn native_capability_and_account_conflicts_prevent_sender() {
        for account_conflict in [false, true] {
            let (db, intent, mut adapter) = wire_fixture();
            let mut passed = intent.clone();
            if account_conflict {
                adapter.account_address = Some("0x2222222222222222222222222222222222222222".into());
            } else {
                passed.symbol = "BTC".into();
            }
            assert!(adapter
                .submit_order_journaled(
                    &passed,
                    &crate::submission_wire::SubmissionJournal::new(&db, &intent)
                )
                .await
                .is_err());
            assert_eq!(
                adapter
                    .test_submissions
                    .load(std::sync::atomic::Ordering::SeqCst),
                0
            );
        }
    }
    #[tokio::test]
    async fn native_signing_preparation_fault_prevents_simulated_sender() {
        let (db, intent, adapter) = wire_fixture();
        db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_preparation BEFORE INSERT ON execution_signing_preparations BEGIN SELECT RAISE(ABORT,'offline-preparation-fault');END")).unwrap();
        let error = adapter
            .submit_order_journaled(
                &intent,
                &crate::submission_wire::SubmissionJournal::new(&db, &intent),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("offline-preparation-fault"));
        assert_eq!(
            adapter
                .test_submissions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[tokio::test]
    async fn native_nonce_history_failure_prevents_simulated_sender() {
        let (db, intent, adapter) = wire_fixture();
        std::fs::write(
            adapter
                .test_nonce_root
                .as_ref()
                .unwrap()
                .path()
                .join("nonces.sqlite"),
            b"offline corrupt nonce fixture",
        )
        .unwrap();
        assert!(adapter
            .submit_order_journaled(
                &intent,
                &crate::submission_wire::SubmissionJournal::new(&db, &intent)
            )
            .await
            .is_err());
        assert_eq!(
            adapter
                .test_submissions
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        let count: i64 = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT count(*) FROM execution_signing_preparations",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(count, 0);
        assert!(db.ensure_execution_resolved().is_err());
    }
}

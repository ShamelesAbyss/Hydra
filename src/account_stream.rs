//! Continuous Hyperliquid private/account WebSocket stream (`v2.0.0 | THE AWAKENING`).
//!
//! Subscribes to `orderUpdates`, `userFills`, and `userFundings` for the configured
//! canonical account identity, deduplicates events in bounded buffers, sends periodic
//! ping heartbeats, reconnects with exponential backoff, and marks `needs_rest_backfill`
//! on every connect/reconnect so REST backfill + settlement reconciliation closes any gap.

use crate::core::Side;
use crate::execution_recovery::{ExchangeOrderSnapshot, OrderLifecycleState};
use crate::ledger::{EvidenceSource, ExchangeFill, FundingPayment, TokenAmount};
use crate::ledger_hyperliquid::{parse_fills, supported_perp};
use anyhow::{ensure, Result};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message as WsMessage;

pub const MAX_ACCOUNT_STREAM_BUFFER: usize = 2048;
pub const MAX_ACCOUNT_SEEN_KEYS: usize = 4096;
const MAX_ACCOUNT_WS_FRAME_BYTES: usize = 4 * 1024 * 1024;

pub fn validate_account_ws_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url.trim())?;
    ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "account WS URL must not contain credentials"
    );
    let host = parsed.host_str().unwrap_or("");
    let is_hl = matches!(parsed.scheme(), "wss")
        && (host.eq_ignore_ascii_case("api.hyperliquid.xyz")
            || host.eq_ignore_ascii_case("api.hyperliquid-testnet.xyz"))
        && parsed.port().is_none();
    let is_loopback = matches!(parsed.scheme(), "ws")
        && matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
        && parsed.port().is_some();
    ensure!(
        is_hl || is_loopback,
        "refusing unapproved account WebSocket URL"
    );
    Ok(())
}

#[derive(Debug, Clone)]
pub struct AccountStreamConfig {
    pub ws_url: String,
    pub account: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountStreamBatch {
    pub orders: Vec<ExchangeOrderSnapshot>,
    pub fills: Vec<ExchangeFill>,
    pub fundings: Vec<FundingPayment>,
    pub connected: bool,
    pub reconnect_count: u64,
    pub needs_rest_backfill: bool,
    pub last_event_ms: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Default)]
struct AccountStreamInner {
    orders: VecDeque<ExchangeOrderSnapshot>,
    fills: VecDeque<ExchangeFill>,
    fundings: VecDeque<FundingPayment>,
    seen_order_keys: BTreeSet<String>,
    seen_order_fifo: VecDeque<String>,
    seen_fill_ids: BTreeSet<String>,
    seen_fill_fifo: VecDeque<String>,
    seen_funding_ids: BTreeSet<String>,
    seen_funding_fifo: VecDeque<String>,
    connected: bool,
    reconnect_count: u64,
    needs_rest_backfill: bool,
    last_event_ms: i64,
    last_error: Option<String>,
}

impl AccountStreamInner {
    fn insert_seen(set: &mut BTreeSet<String>, fifo: &mut VecDeque<String>, key: String) -> bool {
        if !set.insert(key.clone()) {
            return false;
        }
        fifo.push_back(key);
        while fifo.len() > MAX_ACCOUNT_SEEN_KEYS {
            if let Some(old) = fifo.pop_front() {
                set.remove(&old);
            }
        }
        true
    }

    fn push_order(&mut self, snap: ExchangeOrderSnapshot) {
        let key = format!(
            "{}:{:?}:{}:{}",
            snap.exchange_oid, snap.state, snap.filled_qty, snap.observed_at_ms
        );
        if Self::insert_seen(&mut self.seen_order_keys, &mut self.seen_order_fifo, key) {
            self.last_event_ms = self.last_event_ms.max(snap.observed_at_ms);
            self.orders.push_back(snap);
            while self.orders.len() > MAX_ACCOUNT_STREAM_BUFFER {
                self.orders.pop_front();
            }
        }
    }

    fn push_fill(&mut self, fill: ExchangeFill) {
        let fid = fill.fill_id.clone();
        if Self::insert_seen(&mut self.seen_fill_ids, &mut self.seen_fill_fifo, fid) {
            self.last_event_ms = self.last_event_ms.max(fill.timestamp_ms);
            self.fills.push_back(fill);
            while self.fills.len() > MAX_ACCOUNT_STREAM_BUFFER {
                self.fills.pop_front();
            }
        }
    }

    fn push_funding(&mut self, fp: FundingPayment) {
        let fid = fp.funding_id.clone();
        if Self::insert_seen(&mut self.seen_funding_ids, &mut self.seen_funding_fifo, fid) {
            self.last_event_ms = self.last_event_ms.max(fp.timestamp_ms);
            self.fundings.push_back(fp);
            while self.fundings.len() > MAX_ACCOUNT_STREAM_BUFFER {
                self.fundings.pop_front();
            }
        }
    }
}

#[derive(Clone)]
pub struct AccountStreamHandle {
    inner: Arc<Mutex<AccountStreamInner>>,
    stop: Arc<AtomicBool>,
}

impl AccountStreamHandle {
    pub fn drain_updates(&self) -> AccountStreamBatch {
        let mut g = self.inner.lock().unwrap();
        AccountStreamBatch {
            orders: g.orders.drain(..).collect(),
            fills: g.fills.drain(..).collect(),
            fundings: g.fundings.drain(..).collect(),
            connected: g.connected,
            reconnect_count: g.reconnect_count,
            needs_rest_backfill: g.needs_rest_backfill,
            last_event_ms: g.last_event_ms,
            last_error: g.last_error.clone(),
        }
    }

    pub fn mark_rest_backfill_complete(&self, seen_reconnects: u64) {
        let mut g = self.inner.lock().unwrap();
        // Keep `needs_rest_backfill = true` if another reconnect happened while REST was running.
        if g.reconnect_count == seen_reconnects {
            g.needs_rest_backfill = false;
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for AccountStreamHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.stop) <= 2 {
            self.stop.store(true, Ordering::SeqCst);
        }
    }
}

pub fn spawn_account_stream(cfg: AccountStreamConfig) -> Result<AccountStreamHandle> {
    validate_account_ws_url(&cfg.ws_url)?;
    let canonical = crate::ownership::canonical_account(&cfg.account)?;
    let inner = Arc::new(Mutex::new(AccountStreamInner {
        needs_rest_backfill: true,
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = AccountStreamHandle {
        inner: inner.clone(),
        stop: stop.clone(),
    };
    tokio::spawn(run_account_stream_loop(cfg.ws_url, canonical, inner, stop));
    Ok(handle)
}

async fn run_account_stream_loop(
    ws_url: String,
    account: String,
    inner: Arc<Mutex<AccountStreamInner>>,
    stop: Arc<AtomicBool>,
) {
    let mut backoff_ms = 1_000u64;
    while !stop.load(Ordering::SeqCst) {
        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((mut ws, _)) => {
                backoff_ms = 1_000;
                let mut sub_ok = true;
                for sub_type in ["orderUpdates", "userFills", "userFundings"] {
                    let msg = serde_json::json!({
                        "method": "subscribe",
                        "subscription": {
                            "type": sub_type,
                            "user": account,
                        }
                    });
                    if let Err(e) = ws.send(WsMessage::Text(msg.to_string().into())).await {
                        let mut g = inner.lock().unwrap();
                        g.connected = false;
                        g.needs_rest_backfill = true;
                        g.last_error = Some(format!("subscribe {sub_type} failed: {e}"));
                        sub_ok = false;
                        break;
                    }
                }
                if sub_ok {
                    {
                        let mut g = inner.lock().unwrap();
                        g.connected = true;
                        g.needs_rest_backfill = true;
                        g.last_error = None;
                    }
                    let mut ping_interval = tokio::time::interval(Duration::from_secs(20));
                    ping_interval.tick().await;
                    loop {
                        if stop.load(Ordering::SeqCst) {
                            let _ = ws.close(None).await;
                            return;
                        }
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if let Err(e) = ws.send(WsMessage::Text(r#"{"method":"ping"}"#.into())).await {
                                    let mut g = inner.lock().unwrap();
                                    g.last_error = Some(format!("ws ping failed: {e}"));
                                    break;
                                }
                            }
                            frame = ws.next() => {
                                match frame {
                                    Some(Ok(WsMessage::Text(txt))) => {
                                        if txt.len() <= MAX_ACCOUNT_WS_FRAME_BYTES {
                                            process_account_ws_frame(&txt, &inner);
                                        }
                                    }
                                    Some(Ok(WsMessage::Ping(p))) => {
                                        let _ = ws.send(WsMessage::Pong(p)).await;
                                    }
                                    Some(Ok(WsMessage::Close(_))) | None => {
                                        break;
                                    }
                                    Some(Ok(_)) => {}
                                    Some(Err(e)) => {
                                        let mut g = inner.lock().unwrap();
                                        g.last_error = Some(format!("ws frame error: {e}"));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    let mut g = inner.lock().unwrap();
                    g.connected = false;
                    g.reconnect_count = g.reconnect_count.saturating_add(1);
                    g.needs_rest_backfill = true;
                }
            }
            Err(e) => {
                let mut g = inner.lock().unwrap();
                g.connected = false;
                g.needs_rest_backfill = true;
                g.last_error = Some(format!("ws connect failed: {e}"));
            }
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(30_000);
    }
}

#[derive(Deserialize)]
struct WsEnvelope {
    channel: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsOrderUpdateItem {
    order: WsOrderCore,
    status: String,
    #[serde(default)]
    status_timestamp: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsOrderCore {
    coin: String,
    side: String,
    limit_px: String,
    sz: String,
    oid: u64,
    timestamp: i64,
    #[serde(default)]
    orig_sz: Option<String>,
    #[serde(default)]
    cloid: Option<String>,
}

#[derive(Deserialize)]
struct WsFundingItem {
    time: i64,
    coin: String,
    usdc: String,
    #[serde(default)]
    hash: Option<String>,
}

fn process_account_ws_frame(raw: &str, inner: &Arc<Mutex<AccountStreamInner>>) {
    let Ok(env) = serde_json::from_str::<WsEnvelope>(raw) else {
        return;
    };
    let mut g = inner.lock().unwrap();
    match env.channel.as_str() {
        "orderUpdates" => {
            if let Ok(items) = serde_json::from_value::<Vec<WsOrderUpdateItem>>(env.data) {
                for item in items {
                    if supported_perp(&item.order.coin).is_err() || item.order.oid == 0 {
                        continue;
                    }
                    let sz = Decimal::from_str_exact(item.order.sz.trim()).unwrap_or(Decimal::ZERO);
                    let orig = item
                        .order
                        .orig_sz
                        .as_deref()
                        .and_then(|s| Decimal::from_str_exact(s.trim()).ok())
                        .unwrap_or(sz);
                    let filled = (orig - sz).max(Decimal::ZERO);
                    let state = map_hl_status(&item.status, filled, sz);
                    let side = if item.order.side.eq_ignore_ascii_case("B") {
                        Side::Long
                    } else {
                        Side::Short
                    };
                    let cloid = item.order.cloid.and_then(|id| {
                        uuid::Uuid::parse_str(id.strip_prefix("0x").unwrap_or(&id))
                            .ok()
                            .map(|u| u.to_string())
                    });
                    let obs_ms = item.status_timestamp.unwrap_or(item.order.timestamp).max(1);
                    g.push_order(ExchangeOrderSnapshot {
                        source: EvidenceSource::UserWebSocket,
                        cloid,
                        exchange_oid: item.order.oid,
                        symbol: item.order.coin,
                        side,
                        state,
                        filled_qty: filled,
                        avg_fill_px: Decimal::from_str_exact(item.order.limit_px.trim()).ok(),
                        observed_at_ms: obs_ms,
                    });
                }
            }
        }
        "userFills" => {
            let fills_val = env.data.get("fills").cloned().unwrap_or(env.data);
            if let Ok(json_str) = serde_json::to_string(&fills_val) {
                if let Ok(fills) = parse_fills(&json_str) {
                    for f in fills {
                        g.push_fill(f);
                    }
                }
            }
        }
        "userFundings" => {
            let fund_val = env.data.get("fundings").cloned().unwrap_or(env.data);
            if let Ok(rows) = serde_json::from_value::<Vec<WsFundingItem>>(fund_val) {
                for row in rows {
                    if supported_perp(&row.coin).is_err() || row.time <= 0 {
                        continue;
                    }
                    let Ok(amount) = Decimal::from_str_exact(row.usdc.trim()) else {
                        continue;
                    };
                    let fid = match row.hash {
                        Some(ref h) if h.starts_with("0x") && h.len() == 66 => {
                            format!("{}:{}:{}", h.to_ascii_lowercase(), row.time, row.coin)
                        }
                        _ => format!("ws-fund:{}:{}:{}", row.time, row.coin, amount),
                    };
                    g.push_funding(FundingPayment {
                        funding_id: fid,
                        symbol: row.coin,
                        payment: Some(TokenAmount {
                            token: "USDC".into(),
                            amount,
                        }),
                        timestamp_ms: row.time,
                    });
                }
            }
        }
        _ => {
            let _ = Utc::now();
        }
    }
}

fn map_hl_status(status: &str, filled: Decimal, remaining: Decimal) -> OrderLifecycleState {
    let low = status.trim().to_ascii_lowercase();
    if low == "filled" || (remaining == Decimal::ZERO && filled > Decimal::ZERO) {
        OrderLifecycleState::Filled
    } else if low.contains("cancel") {
        OrderLifecycleState::Cancelled
    } else if low.contains("reject")
        || low.contains("margin")
        || low.contains("bad")
        || low.contains("error")
        || low.contains("reduceonly")
    {
        OrderLifecycleState::Rejected
    } else if filled > Decimal::ZERO {
        OrderLifecycleState::PartiallyFilled
    } else {
        OrderLifecycleState::Acknowledged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn account_ws_url_validation_allows_hl_and_loopback_and_rejects_others() {
        assert!(validate_account_ws_url("wss://api.hyperliquid.xyz/ws").is_ok());
        assert!(validate_account_ws_url("wss://api.hyperliquid-testnet.xyz/ws").is_ok());
        assert!(validate_account_ws_url("ws://127.0.0.1:9999/ws").is_ok());
        assert!(validate_account_ws_url("wss://evil.example.com/ws").is_err());
        assert!(validate_account_ws_url("ws://example.com:80/ws").is_err());
    }

    #[tokio::test]
    async fn private_account_stream_ingests_dedups_and_flags_reconnect_gap() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            // Connection 1: send orderUpdate, userFills (with duplicate), userFundings, then drop.
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut subs = 0usize;
            while subs < 3 {
                if let Some(Ok(WsMessage::Text(t))) = ws.next().await {
                    if t.contains("\"subscribe\"") {
                        subs += 1;
                    }
                }
            }
            let ord = serde_json::json!({
                "channel": "orderUpdates",
                "data": [{
                    "order": {
                        "coin": "BTC", "side": "B", "limitPx": "60000", "sz": "0.0",
                        "origSz": "0.01", "oid": 777, "timestamp": 1_700_000_000_000i64,
                        "cloid": "0x11111111111111111111111111111111"
                    },
                    "status": "filled",
                    "statusTimestamp": 1_700_000_000_100i64
                }]
            });
            let fills = serde_json::json!({
                "channel": "userFills",
                "data": {
                    "fills": [
                        {"tid": 55, "oid": 777, "coin": "BTC", "side": "B", "px": "60000", "sz": "0.01", "time": 1_700_000_000_100i64, "fee": "0.3", "feeToken": "USDC"},
                        {"tid": 55, "oid": 777, "coin": "BTC", "side": "B", "px": "60000", "sz": "0.01", "time": 1_700_000_000_100i64, "fee": "0.3", "feeToken": "USDC"}
                    ]
                }
            });
            let fund = serde_json::json!({
                "channel": "userFundings",
                "data": {
                    "fundings": [{"time": 1_700_003_600_000i64, "coin": "BTC", "usdc": "-0.05"}]
                }
            });
            ws.send(WsMessage::Text(ord.to_string().into()))
                .await
                .unwrap();
            ws.send(WsMessage::Text(fills.to_string().into()))
                .await
                .unwrap();
            ws.send(WsMessage::Text(fund.to_string().into()))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(80)).await;
            let _ = ws.close(None).await;

            // Connection 2: bounded wait for reconnect so test never hangs
            if let Ok(Ok((s2, _))) =
                tokio::time::timeout(Duration::from_secs(6), listener.accept()).await
            {
                if let Ok(mut ws2) = tokio_tungstenite::accept_async(s2).await {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    let _ = ws2.close(None).await;
                }
            }
        });

        let handle = spawn_account_stream(AccountStreamConfig {
            ws_url: format!("ws://{addr}/ws"),
            account: "0x1111111111111111111111111111111111111111".into(),
        })
        .unwrap();

        let mut all_orders = Vec::new();
        let mut all_fills = Vec::new();
        let mut all_fundings = Vec::new();
        let mut saw_needs_backfill = false;
        let mut first_reconnect_count = 0u64;
        let mut got_all = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let batch = handle.drain_updates();
            all_orders.extend(batch.orders);
            all_fills.extend(batch.fills);
            all_fundings.extend(batch.fundings);
            saw_needs_backfill |= batch.needs_rest_backfill;
            first_reconnect_count = first_reconnect_count.max(batch.reconnect_count);
            if !all_orders.is_empty() && !all_fills.is_empty() && !all_fundings.is_empty() {
                assert_eq!(
                    all_fills.len(),
                    1,
                    "duplicate fill tid must be deduplicated"
                );
                assert!(saw_needs_backfill);
                handle.mark_rest_backfill_complete(first_reconnect_count);
                got_all = true;
                break;
            }
        }
        assert!(got_all);
        // Wait for disconnect + reconnect (backoff=1s) to complete & flag needs_rest_backfill again.
        let mut saw_reconnect = false;
        for _ in 0..120 {
            tokio::time::sleep(Duration::from_millis(40)).await;
            let batch = handle.drain_updates();
            if batch.reconnect_count >= 1 && batch.connected && batch.needs_rest_backfill {
                saw_reconnect = true;
                break;
            }
        }
        assert!(saw_reconnect);
        handle.stop();
        server_task.abort();
    }
}

//! Genuine HUNTER public WebSocket and REST snapshot ingestion for Hyperliquid.
//!
//! Safety & truthfulness invariants:
//! - Preserves `exchange_ts_ms: Option<i64>` separately from local `received_at_ms: i64`.
//!   Never invents exchange sequence identifiers or overwrites unavailable exchange times.
//! - Computes L2 bid/ask depth and spread strictly from real `l2Book` price levels,
//!   never from open interest or hardcoded defaults.
//! - Separates open/in-progress candles from strictly completed chronological OHLCV bars.
//! - Computes derived directionality, volatility, and bar coverage strictly from completed
//!   candles, leaving metrics `None` when completed history is insufficient (`< 12` bars).
//! - Maintains bounded per-symbol memory for trades, deduplicated trade IDs, and candles.
//! - Continues protective market monitoring for `held_positions` independently of watchlist
//!   selection (`monitor_universe = watchlist ∪ held_positions`).

use crate::core::Candle;
use crate::hunter::{parse_hyperliquid_meta_and_ctxs, PublicHunterPolicy, PublicMarketObservation};
use crate::synapse_bus::DataProvenance;
use anyhow::{anyhow, ensure, Context, Result};
use chrono::{TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message as WsMessage;

pub const HYPERLIQUID_PUBLIC_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";
pub const MAX_TRADE_HISTORY_PER_SYMBOL: usize = 256;
pub const MAX_SEEN_TIDS_PER_SYMBOL: usize = 2048;
pub const MAX_COMPLETED_CANDLES_PER_SYMBOL: usize = 256;
pub const MAX_WS_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_INFO_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
pub const MIN_COMPLETED_BARS_FOR_DERIVED: usize = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L2BookObservation {
    pub symbol: String,
    pub best_bid: f64,
    pub best_ask: f64,
    pub mid: f64,
    pub spread_bps: f64,
    pub top_depth_usd: f64,
    pub bid_depth_usd: f64,
    pub ask_depth_usd: f64,
    pub levels_bid: usize,
    pub levels_ask: usize,
    #[serde(default)]
    pub bids: Vec<(f64, f64)>,
    #[serde(default)]
    pub asks: Vec<(f64, f64)>,
    pub exchange_ts_ms: Option<i64>,
    pub received_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L2DepthSample {
    pub received_at_ms: i64,
    pub spread_bps: f64,
    pub bid_depth_usd: f64,
    pub ask_depth_usd: f64,
    pub top_bid_usd: f64,
    pub top_ask_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeObservation {
    pub symbol: String,
    pub px: f64,
    pub sz: f64,
    pub side: String,
    pub exchange_ts_ms: i64,
    pub received_at_ms: i64,
    pub tid: u64,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompletedCandleSeries {
    pub completed: VecDeque<Candle>,
    /// Tracks `(open_ts_ms, close_ts_ms, in_progress_candle)` until closed.
    pub open_candle: Option<(i64, i64, Candle)>,
    pub last_exchange_ts_ms: Option<i64>,
    pub last_received_at_ms: i64,
}

impl CompletedCandleSeries {
    pub fn completed_vec(&self) -> Vec<Candle> {
        self.completed.iter().cloned().collect()
    }

    pub fn push_candle(
        &mut self,
        open_ms: i64,
        close_ms: i64,
        candle: Candle,
        as_of_ms: i64,
        received_at_ms: i64,
    ) {
        if !candle.open.is_finite()
            || !candle.high.is_finite()
            || !candle.low.is_finite()
            || !candle.close.is_finite()
            || !candle.volume.is_finite()
            || candle.open <= 0.0
            || candle.close <= 0.0
            || candle.high < candle.low
            || candle.volume < 0.0
        {
            return;
        }
        self.last_received_at_ms = received_at_ms;
        self.last_exchange_ts_ms = Some(
            self.last_exchange_ts_ms
                .map_or(close_ms, |prev| prev.max(close_ms)),
        );

        // If a newer bar starts, promote any older in-progress candle that has closed.
        if let Some((prev_open, prev_close, prev_bar)) = self.open_candle.clone() {
            if open_ms > prev_open {
                self.insert_completed_bar(prev_bar);
                let _ = prev_close;
                self.open_candle = None;
            }
        }

        if close_ms <= as_of_ms {
            self.insert_completed_bar(candle);
            if let Some((cur_open, _, _)) = &self.open_candle {
                if *cur_open == open_ms {
                    self.open_candle = None;
                }
            }
        } else {
            self.open_candle = Some((open_ms, close_ms, candle));
        }
    }

    fn insert_completed_bar(&mut self, bar: Candle) {
        if let Some(last) = self.completed.back_mut() {
            if last.ts == bar.ts {
                *last = bar;
                return;
            }
            if bar.ts < last.ts {
                // Ignore out-of-order historical bar older than our latest completed bar.
                return;
            }
        }
        self.completed.push_back(bar);
        while self.completed.len() > MAX_COMPLETED_CANDLES_PER_SYMBOL {
            self.completed.pop_front();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstrumentMetaCtx {
    pub symbol: String,
    pub sz_decimals: Option<u8>,
    pub max_leverage: u32,
    pub mark_px: f64,
    pub oracle_px: Option<f64>,
    pub funding_bps_1h: f64,
    pub open_interest_usd: f64,
    pub volume_24h_usd: f64,
    pub impact_spread_bps: Option<f64>,
    pub exchange_ts_ms: Option<i64>,
    pub received_at_ms: i64,
    #[serde(default)]
    pub mark_received_at_ms: Option<i64>,
    #[serde(default)]
    pub funding_received_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedMarketMetrics {
    pub directionality_er: Option<f64>,
    pub volatility_bar_std_pct: Option<f64>,
    pub coverage_fraction: Option<f64>,
    pub completed_bars: usize,
    pub provenance: DataProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamHealth {
    pub connected: bool,
    pub reconnect_count: u64,
    pub duplicate_trades_dropped: u64,
    pub out_of_order_events: u64,
    pub parse_errors: u64,
    pub rate_limit_backoffs: u64,
    pub stale_symbols: Vec<String>,
    pub missing_data_symbols: Vec<String>,
    pub last_exchange_ts_ms: Option<i64>,
    pub last_receipt_ts_ms: Option<i64>,
    pub last_pong_ms: Option<i64>,
    pub fallback_rest_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldFreshness {
    pub updated_at_ms: Option<i64>,
    pub exchange_ts_ms: Option<i64>,
    pub is_stale: bool,
}

impl FieldFreshness {
    pub fn evaluate(
        updated_at_ms: Option<i64>,
        exchange_ts_ms: Option<i64>,
        as_of_ms: i64,
        stale_after_ms: i64,
    ) -> Self {
        const MAX_CLOCK_SKEW_MS: i64 = 5_000;
        let receipt_stale = match updated_at_ms {
            Some(ts) => {
                ts > as_of_ms.saturating_add(MAX_CLOCK_SKEW_MS)
                    || as_of_ms.saturating_sub(ts) > stale_after_ms
            }
            None => true,
        };
        let exchange_stale = match exchange_ts_ms {
            Some(ex_ts) => {
                ex_ts > as_of_ms.saturating_add(MAX_CLOCK_SKEW_MS)
                    || as_of_ms.saturating_sub(ex_ts) > stale_after_ms
            }
            None => false,
        };
        Self {
            updated_at_ms,
            exchange_ts_ms,
            is_stale: receipt_stale || exchange_stale,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketFreshnessReport {
    pub mark: FieldFreshness,
    pub bid_ask: FieldFreshness,
    pub l2_depth: FieldFreshness,
    pub funding: FieldFreshness,
    pub instrument_meta: FieldFreshness,
    pub completed_candles: FieldFreshness,
    pub derived_stats: FieldFreshness,
}

impl MarketFreshnessReport {
    pub fn mark_is_fresh(&self) -> bool {
        !self.mark.is_stale
    }

    pub fn any_critical_stale(&self) -> bool {
        self.mark.is_stale
            || self.bid_ask.is_stale
            || self.l2_depth.is_stale
            || self.funding.is_stale
            || self.instrument_meta.is_stale
            || self.completed_candles.is_stale
            || self.derived_stats.is_stale
    }

    pub fn oldest_critical_update_ms(&self) -> Option<i64> {
        [
            &self.mark,
            &self.bid_ask,
            &self.l2_depth,
            &self.funding,
            &self.instrument_meta,
            &self.completed_candles,
            &self.derived_stats,
        ]
        .into_iter()
        .try_fold(i64::MAX, |acc, f| f.updated_at_ms.map(|ts| acc.min(ts)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedMarketSnapshot {
    pub symbol: String,
    pub mark_px: f64,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread_bps: Option<f64>,
    pub depth_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
    pub open_interest_usd: Option<f64>,
    pub funding_bps_1h: Option<f64>,
    pub volatility_pct: Option<f64>,
    pub directionality: Option<f64>,
    pub coverage_fraction: Option<f64>,
    pub completed_candles: Vec<Candle>,
    pub observed_history_minutes: u64,
    pub instrument_listing_age_days: Option<f64>,
    pub freshness: MarketFreshnessReport,
    pub exchange_ts_ms: Option<i64>,
    pub received_at_ms: i64,
    pub is_stale: bool,
    pub missing_fields: Vec<String>,
    pub provenance: DataProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsEventOutcome {
    Subscribed,
    Pong,
    L2Book,
    Trades(usize),
    Candle,
    AssetCtx,
    IgnoredChannel,
}

#[derive(Debug, Clone, Default)]
struct TradeStreamBuffer {
    recent: VecDeque<TradeObservation>,
    seen_order: VecDeque<u64>,
    seen_set: HashSet<u64>,
    last_exchange_ts_ms: Option<i64>,
}

impl TradeStreamBuffer {
    fn ingest(
        &mut self,
        trade: TradeObservation,
        out_of_order_counter: &mut u64,
        dup_counter: &mut u64,
    ) -> bool {
        if self.seen_set.contains(&trade.tid) {
            *dup_counter = dup_counter.saturating_add(1);
            return false;
        }
        if let Some(prev_ts) = self.last_exchange_ts_ms {
            if trade.exchange_ts_ms < prev_ts {
                *out_of_order_counter = out_of_order_counter.saturating_add(1);
            }
        }
        self.last_exchange_ts_ms = Some(
            self.last_exchange_ts_ms
                .map_or(trade.exchange_ts_ms, |p| p.max(trade.exchange_ts_ms)),
        );
        self.seen_set.insert(trade.tid);
        self.seen_order.push_back(trade.tid);
        while self.seen_order.len() > MAX_SEEN_TIDS_PER_SYMBOL {
            if let Some(evicted) = self.seen_order.pop_front() {
                self.seen_set.remove(&evicted);
            }
        }
        self.recent.push_back(trade);
        while self.recent.len() > MAX_TRADE_HISTORY_PER_SYMBOL {
            self.recent.pop_front();
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct HunterMarketState {
    pub l2_books: BTreeMap<String, L2BookObservation>,
    pub l2_history: BTreeMap<String, VecDeque<L2DepthSample>>,
    pub oi_history: BTreeMap<String, VecDeque<(i64, f64)>>,
    trades: BTreeMap<String, TradeStreamBuffer>,
    pub candles: BTreeMap<String, CompletedCandleSeries>,
    pub meta_ctx: BTreeMap<String, InstrumentMetaCtx>,
    pub health: StreamHealth,
    pub stale_after_ms: i64,
}

impl Default for HunterMarketState {
    fn default() -> Self {
        Self::new(30_000)
    }
}

impl HunterMarketState {
    pub fn new(stale_after_ms: i64) -> Self {
        Self {
            l2_books: BTreeMap::new(),
            l2_history: BTreeMap::new(),
            oi_history: BTreeMap::new(),
            trades: BTreeMap::new(),
            candles: BTreeMap::new(),
            meta_ctx: BTreeMap::new(),
            health: StreamHealth::default(),
            stale_after_ms: stale_after_ms.clamp(2_000, 300_000),
        }
    }

    pub fn trade_count(&self, symbol: &str) -> usize {
        self.trades.get(symbol).map_or(0, |b| b.recent.len())
    }

    pub fn recent_trades(&self, symbol: &str) -> Vec<TradeObservation> {
        self.trades
            .get(symbol)
            .map(|b| b.recent.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Computes portfolio-aware monitored symbols = `watchlist ∪ held_positions`.
    /// Protective monitoring for already-held positions never stops when watchlist rotates.
    pub fn monitor_universe(
        watchlist: &[String],
        held_positions: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut set: BTreeSet<String> = watchlist.iter().cloned().collect();
        for sym in held_positions {
            set.insert(sym.clone());
        }
        set.into_iter().collect()
    }

    /// Parses and ingests Hyperliquid `metaAndAssetCtxs` REST payload cleanly.
    pub fn apply_meta_and_ctxs_json(
        &mut self,
        payload: &[u8],
        received_at_ms: i64,
    ) -> Result<Vec<PublicMarketObservation>> {
        let obs_list = parse_hyperliquid_meta_and_ctxs(payload)?;
        let root: serde_json::Value = serde_json::from_slice(payload).unwrap_or_default();
        let mut decimals_by_symbol: BTreeMap<String, u8> = BTreeMap::new();
        if let Some(universe) = root
            .as_array()
            .and_then(|a| a.first())
            .and_then(|m| m.get("universe"))
            .and_then(|u| u.as_array())
        {
            for item in universe {
                if let (Some(name), Some(dec)) = (
                    item.get("name").and_then(|n| n.as_str()),
                    item.get("szDecimals").and_then(|d| d.as_u64()),
                ) {
                    if let Ok(d8) = u8::try_from(dec) {
                        decimals_by_symbol.insert(name.to_string(), d8);
                    }
                }
            }
        }

        for obs in &obs_list {
            let oi_hist = self.oi_history.entry(obs.symbol.clone()).or_default();
            oi_hist.push_back((received_at_ms, obs.open_interest_usd));
            while oi_hist.len() > 64 {
                oi_hist.pop_front();
            }
            self.meta_ctx.insert(
                obs.symbol.clone(),
                InstrumentMetaCtx {
                    symbol: obs.symbol.clone(),
                    sz_decimals: decimals_by_symbol.get(&obs.symbol).copied(),
                    max_leverage: obs.max_leverage,
                    mark_px: obs.mark_px,
                    oracle_px: obs.oracle_px,
                    funding_bps_1h: obs.funding_bps_1h,
                    open_interest_usd: obs.open_interest_usd,
                    volume_24h_usd: obs.volume_24h_usd,
                    impact_spread_bps: obs.impact_spread_bps,
                    exchange_ts_ms: None,
                    received_at_ms,
                    mark_received_at_ms: Some(received_at_ms),
                    funding_received_at_ms: Some(received_at_ms),
                },
            );
        }
        self.health.last_receipt_ts_ms = Some(
            self.health
                .last_receipt_ts_ms
                .map_or(received_at_ms, |p| p.max(received_at_ms)),
        );
        Ok(obs_list)
    }

    pub fn apply_l2_book_value(
        &mut self,
        data: &serde_json::Value,
        received_at_ms: i64,
    ) -> Result<()> {
        let symbol = data
            .get("coin")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("l2Book missing coin"))?;
        let levels = data
            .get("levels")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("l2Book missing levels"))?;
        ensure!(levels.len() >= 2, "l2Book requires bid & ask level arrays");

        let parse_side = |arr: &Vec<serde_json::Value>| -> Vec<(f64, f64)> {
            arr.iter()
                .filter_map(|lv| {
                    let px = parse_json_f64(lv.get("px")?)?;
                    let sz = parse_json_f64(lv.get("sz")?)?;
                    (px > 0.0 && sz >= 0.0).then_some((px, sz))
                })
                .collect()
        };

        let bids = levels[0]
            .as_array()
            .map(parse_side)
            .ok_or_else(|| anyhow!("l2Book bids not an array"))?;
        let asks = levels[1]
            .as_array()
            .map(parse_side)
            .ok_or_else(|| anyhow!("l2Book asks not an array"))?;

        let (best_bid, best_bid_sz) = bids
            .first()
            .copied()
            .ok_or_else(|| anyhow!("l2Book {symbol} empty bids"))?;
        let (best_ask, best_ask_sz) = asks
            .first()
            .copied()
            .ok_or_else(|| anyhow!("l2Book {symbol} empty asks"))?;
        ensure!(
            best_ask >= best_bid && best_bid > 0.0,
            "crossed or invalid L2 book for {symbol}"
        );
        let mid = 0.5 * (best_bid + best_ask);
        let spread_bps = ((best_ask - best_bid) / mid) * 10_000.0;
        let bid_depth_usd: f64 = bids.iter().take(20).map(|(p, s)| p * s).sum();
        let ask_depth_usd: f64 = asks.iter().take(20).map(|(p, s)| p * s).sum();
        let top_depth_usd = best_bid * best_bid_sz + best_ask * best_ask_sz;
        let exchange_ts_ms = data.get("time").and_then(|t| t.as_i64());

        if let Some(ex_ts) = exchange_ts_ms {
            if let Some(prev) = self.l2_books.get(symbol).and_then(|b| b.exchange_ts_ms) {
                if ex_ts < prev {
                    self.health.out_of_order_events =
                        self.health.out_of_order_events.saturating_add(1);
                }
            }
            self.health.last_exchange_ts_ms = Some(
                self.health
                    .last_exchange_ts_ms
                    .map_or(ex_ts, |p| p.max(ex_ts)),
            );
        }
        self.health.last_receipt_ts_ms = Some(
            self.health
                .last_receipt_ts_ms
                .map_or(received_at_ms, |p| p.max(received_at_ms)),
        );

        let top_bids: Vec<(f64, f64)> = bids.iter().take(20).copied().collect();
        let top_asks: Vec<(f64, f64)> = asks.iter().take(20).copied().collect();
        let depth_hist = self.l2_history.entry(symbol.to_string()).or_default();
        depth_hist.push_back(L2DepthSample {
            received_at_ms,
            spread_bps,
            bid_depth_usd,
            ask_depth_usd,
            top_bid_usd: best_bid * best_bid_sz,
            top_ask_usd: best_ask * best_ask_sz,
        });
        while depth_hist.len() > 64 {
            depth_hist.pop_front();
        }

        self.l2_books.insert(
            symbol.to_string(),
            L2BookObservation {
                symbol: symbol.to_string(),
                best_bid,
                best_ask,
                mid,
                spread_bps,
                top_depth_usd,
                bid_depth_usd,
                ask_depth_usd,
                levels_bid: bids.len(),
                levels_ask: asks.len(),
                bids: top_bids,
                asks: top_asks,
                exchange_ts_ms,
                received_at_ms,
            },
        );
        Ok(())
    }

    pub fn apply_candle_snapshot_json(
        &mut self,
        symbol: &str,
        payload: &[u8],
        received_at_ms: i64,
    ) -> Result<usize> {
        ensure!(
            payload.len() <= MAX_INFO_PAYLOAD_BYTES,
            "candleSnapshot payload exceeded 4 MiB bound"
        );
        let arr: Vec<serde_json::Value> =
            serde_json::from_slice(payload).context("decode candleSnapshot array")?;
        let mut applied = 0usize;
        for item in &arr {
            if self.ingest_candle_value(Some(symbol), item, received_at_ms) {
                applied += 1;
            }
        }
        Ok(applied)
    }

    fn ingest_candle_value(
        &mut self,
        fallback_symbol: Option<&str>,
        item: &serde_json::Value,
        received_at_ms: i64,
    ) -> bool {
        let Some(symbol) = item
            .get("s")
            .and_then(|v| v.as_str())
            .or(fallback_symbol)
            .map(str::to_string)
        else {
            return false;
        };
        let Some(open_ms) = item.get("t").and_then(|v| v.as_i64()) else {
            return false;
        };
        let close_ms = item
            .get("T")
            .and_then(|v| v.as_i64())
            .unwrap_or(open_ms + 59_999);
        let (Some(o), Some(h), Some(l), Some(c), Some(v)) = (
            item.get("o").and_then(parse_json_f64),
            item.get("h").and_then(parse_json_f64),
            item.get("l").and_then(parse_json_f64),
            item.get("c").and_then(parse_json_f64),
            item.get("v").and_then(parse_json_f64),
        ) else {
            return false;
        };
        let Some(ts) = Utc.timestamp_millis_opt(open_ms).single() else {
            return false;
        };
        let bar = Candle {
            ts,
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
        };
        let series = self.candles.entry(symbol).or_default();
        series.push_candle(open_ms, close_ms, bar, received_at_ms, received_at_ms);
        self.health.last_exchange_ts_ms = Some(
            self.health
                .last_exchange_ts_ms
                .map_or(close_ms, |p| p.max(close_ms)),
        );
        self.health.last_receipt_ts_ms = Some(
            self.health
                .last_receipt_ts_ms
                .map_or(received_at_ms, |p| p.max(received_at_ms)),
        );
        true
    }

    pub fn apply_ws_message(&mut self, raw: &str, received_at_ms: i64) -> Result<WsEventOutcome> {
        ensure!(
            raw.len() <= MAX_WS_PAYLOAD_BYTES,
            "WebSocket message exceeded 2 MiB bound"
        );
        let root: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            self.health.parse_errors = self.health.parse_errors.saturating_add(1);
            anyhow!("parse Hyperliquid WS JSON: {e}")
        })?;
        let channel = root.get("channel").and_then(|v| v.as_str()).unwrap_or("");
        match channel {
            "subscriptionResponse" => Ok(WsEventOutcome::Subscribed),
            "pong" => {
                self.health.last_pong_ms = Some(received_at_ms);
                self.health.last_receipt_ts_ms = Some(received_at_ms);
                Ok(WsEventOutcome::Pong)
            }
            "l2Book" => {
                let data = root
                    .get("data")
                    .ok_or_else(|| anyhow!("l2Book WS message missing data"))?;
                self.apply_l2_book_value(data, received_at_ms)?;
                Ok(WsEventOutcome::L2Book)
            }
            "trades" => {
                let items = root
                    .get("data")
                    .and_then(|d| d.as_array())
                    .ok_or_else(|| anyhow!("trades WS message missing array data"))?;
                let mut inserted = 0usize;
                for item in items {
                    let Some(symbol) = item.get("coin").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let (Some(px), Some(sz), Some(ex_ts), Some(tid)) = (
                        item.get("px").and_then(parse_json_f64),
                        item.get("sz").and_then(parse_json_f64),
                        item.get("time").and_then(|v| v.as_i64()),
                        item.get("tid").and_then(|v| v.as_u64()),
                    ) else {
                        self.health.parse_errors = self.health.parse_errors.saturating_add(1);
                        continue;
                    };
                    if !px.is_finite() || px <= 0.0 || !sz.is_finite() || sz < 0.0 {
                        continue;
                    }
                    let side = item
                        .get("side")
                        .and_then(|v| v.as_str())
                        .unwrap_or("U")
                        .to_string();
                    let hash = item.get("hash").and_then(|v| v.as_str()).map(String::from);
                    let buf = self.trades.entry(symbol.to_string()).or_default();
                    let obs = TradeObservation {
                        symbol: symbol.to_string(),
                        px,
                        sz,
                        side,
                        exchange_ts_ms: ex_ts,
                        received_at_ms,
                        tid,
                        hash,
                    };
                    if buf.ingest(
                        obs,
                        &mut self.health.out_of_order_events,
                        &mut self.health.duplicate_trades_dropped,
                    ) {
                        inserted += 1;
                    }
                    self.health.last_exchange_ts_ms = Some(
                        self.health
                            .last_exchange_ts_ms
                            .map_or(ex_ts, |p| p.max(ex_ts)),
                    );
                }
                self.health.last_receipt_ts_ms = Some(received_at_ms);
                Ok(WsEventOutcome::Trades(inserted))
            }
            "candle" => {
                if let Some(data) = root.get("data") {
                    if let Some(arr) = data.as_array() {
                        for item in arr {
                            self.ingest_candle_value(None, item, received_at_ms);
                        }
                    } else {
                        self.ingest_candle_value(None, data, received_at_ms);
                    }
                }
                Ok(WsEventOutcome::Candle)
            }
            "activeAssetCtx" => {
                if let Some(data) = root.get("data") {
                    if let Some(symbol) = data.get("coin").and_then(|v| v.as_str()) {
                        if let Some(ctx) = data.get("ctx") {
                            if let Some(mark_px) = ctx.get("markPx").and_then(parse_json_f64) {
                                if mark_px > 0.0 {
                                    let entry = self
                                        .meta_ctx
                                        .entry(symbol.to_string())
                                        .or_insert_with(|| InstrumentMetaCtx {
                                            symbol: symbol.to_string(),
                                            sz_decimals: None,
                                            max_leverage: 1,
                                            mark_px,
                                            oracle_px: None,
                                            funding_bps_1h: 0.0,
                                            open_interest_usd: 0.0,
                                            volume_24h_usd: 0.0,
                                            impact_spread_bps: None,
                                            exchange_ts_ms: None,
                                            received_at_ms,
                                            mark_received_at_ms: Some(received_at_ms),
                                            funding_received_at_ms: None,
                                        });
                                    entry.mark_px = mark_px;
                                    entry.mark_received_at_ms = Some(received_at_ms);
                                    if let Some(opx) = ctx.get("oraclePx").and_then(parse_json_f64)
                                    {
                                        entry.oracle_px = Some(opx);
                                    }
                                    if let Some(fund) = ctx.get("funding").and_then(parse_json_f64)
                                    {
                                        entry.funding_bps_1h = fund * 10_000.0;
                                        entry.funding_received_at_ms = Some(received_at_ms);
                                    }
                                    if let Some(oi) =
                                        ctx.get("openInterest").and_then(parse_json_f64)
                                    {
                                        entry.open_interest_usd = (oi * mark_px).max(0.0);
                                        entry.received_at_ms = received_at_ms;
                                    }
                                    if let Some(v) = ctx.get("dayNtlVlm").and_then(parse_json_f64) {
                                        entry.volume_24h_usd = v.max(0.0);
                                        entry.received_at_ms = received_at_ms;
                                    }
                                }
                            }
                        }
                    }
                }
                self.health.last_receipt_ts_ms = Some(received_at_ms);
                Ok(WsEventOutcome::AssetCtx)
            }
            _ => Ok(WsEventOutcome::IgnoredChannel),
        }
    }

    /// Computes statistical market metrics strictly from completed chronological candles.
    /// Returns `None` fields when fewer than `MIN_COMPLETED_BARS_FOR_DERIVED` completed
    /// bars exist — never fabricates default directionality, volatility, or coverage.
    pub fn derived_metrics(&self, symbol: &str, lookback: usize) -> DerivedMarketMetrics {
        let Some(series) = self.candles.get(symbol) else {
            return DerivedMarketMetrics {
                directionality_er: None,
                volatility_bar_std_pct: None,
                coverage_fraction: None,
                completed_bars: 0,
                provenance: DataProvenance::DerivedMarketStat,
            };
        };
        let n_all = series.completed.len();
        if n_all < MIN_COMPLETED_BARS_FOR_DERIVED {
            return DerivedMarketMetrics {
                directionality_er: None,
                volatility_bar_std_pct: None,
                coverage_fraction: None,
                completed_bars: n_all,
                provenance: DataProvenance::DerivedMarketStat,
            };
        }
        let take_n = lookback.clamp(MIN_COMPLETED_BARS_FOR_DERIVED, n_all);
        let bars: Vec<&Candle> = series.completed.iter().skip(n_all - take_n).collect();
        let first_close = bars.first().unwrap().close;
        let last_close = bars.last().unwrap().close;
        let net_move = (last_close - first_close).abs();
        let mut path_sum = 0.0;
        let mut log_rets = Vec::with_capacity(bars.len().saturating_sub(1));
        for win in bars.windows(2) {
            path_sum += (win[1].close - win[0].close).abs();
            if win[0].close > 0.0 && win[1].close > 0.0 {
                log_rets.push((win[1].close / win[0].close).ln());
            }
        }
        let directionality_er = if path_sum > 1e-12 {
            Some((net_move / path_sum).clamp(0.0, 1.0))
        } else {
            Some(0.0)
        };
        let volatility_bar_std_pct = if log_rets.len() >= 2 {
            let n = log_rets.len() as f64;
            let mean = log_rets.iter().sum::<f64>() / n;
            let var = log_rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
            Some((var.sqrt() * 100.0).max(0.0))
        } else {
            None
        };
        let span_mins =
            ((bars.last().unwrap().ts - bars.first().unwrap().ts).num_minutes() + 1).max(1) as f64;
        let coverage_fraction = Some((bars.len() as f64 / span_mins).clamp(0.0, 1.0));

        DerivedMarketMetrics {
            directionality_er,
            volatility_bar_std_pct,
            coverage_fraction,
            completed_bars: n_all,
            provenance: DataProvenance::DerivedMarketStat,
        }
    }

    pub fn refresh_health_diagnostics(&mut self, watched_symbols: &[String], now_ms: i64) {
        let mut stale = Vec::new();
        let mut missing = Vec::new();
        for sym in watched_symbols {
            let book = self.l2_books.get(sym);
            let ctx = self.meta_ctx.get(sym);
            if book.is_none() && ctx.is_none() {
                missing.push(sym.clone());
                continue;
            }
            let any_present_stale = book.is_some_and(|b| {
                b.received_at_ms > now_ms
                    || now_ms.saturating_sub(b.received_at_ms) > self.stale_after_ms
            }) || ctx.is_some_and(|c| {
                let mark_ts = c.mark_received_at_ms.unwrap_or(c.received_at_ms);
                let meta_stale = c.received_at_ms > now_ms
                    || now_ms.saturating_sub(c.received_at_ms) > self.stale_after_ms;
                let mark_stale =
                    mark_ts > now_ms || now_ms.saturating_sub(mark_ts) > self.stale_after_ms;
                let funding_stale = c.funding_received_at_ms.is_some_and(|ft| {
                    ft > now_ms || now_ms.saturating_sub(ft) > self.stale_after_ms
                });
                meta_stale || mark_stale || funding_stale
            });
            if any_present_stale {
                stale.push(sym.clone());
            }
        }
        self.health.stale_symbols = stale;
        self.health.missing_data_symbols = missing;
    }

    /// Assembles a strictly truthful `ObservedMarketSnapshot` for `symbol`.
    /// Unavailable fields remain `None` and are listed in `missing_fields`, and
    /// each field's freshness is tracked independently without masking stale updates.
    pub fn observed_market_snapshot(
        &self,
        symbol: &str,
        as_of_ms: i64,
    ) -> Option<ObservedMarketSnapshot> {
        let book = self.l2_books.get(symbol);
        let ctx = self.meta_ctx.get(symbol);
        if book.is_none() && ctx.is_none() {
            return None;
        }
        let mark_px = ctx
            .map(|c| c.mark_px)
            .or_else(|| book.map(|b| b.mid))
            .filter(|p| p.is_finite() && *p > 0.0)?;

        let best_bid = book.map(|b| b.best_bid);
        let best_ask = book.map(|b| b.best_ask);
        let spread_bps = book
            .map(|b| b.spread_bps)
            .or_else(|| ctx.and_then(|c| c.impact_spread_bps));
        let depth_usd = book.map(|b| b.bid_depth_usd.min(b.ask_depth_usd));
        let volume_24h_usd = ctx.map(|c| c.volume_24h_usd);
        let open_interest_usd = ctx.map(|c| c.open_interest_usd);
        let funding_bps_1h = ctx.and_then(|c| c.funding_received_at_ms.map(|_| c.funding_bps_1h));
        let derived = self.derived_metrics(symbol, 24);
        let candle_series = self.candles.get(symbol);
        let completed_candles = candle_series
            .map(CompletedCandleSeries::completed_vec)
            .unwrap_or_default();
        let observed_history_minutes = match (completed_candles.first(), completed_candles.last()) {
            (Some(first), Some(last)) if completed_candles.len() >= 2 => {
                ((last.ts - first.ts).num_minutes().max(0) as u64).saturating_add(1)
            }
            (Some(_), _) => 1,
            _ => 0,
        };

        let mark_ts = ctx
            .map(|c| c.mark_received_at_ms.unwrap_or(c.received_at_ms))
            .or_else(|| book.map(|b| b.received_at_ms));
        let mark_ex_ts = ctx
            .and_then(|c| c.exchange_ts_ms)
            .or_else(|| book.and_then(|b| b.exchange_ts_ms));
        let book_ts = book.map(|b| b.received_at_ms);
        let book_ex_ts = book.and_then(|b| b.exchange_ts_ms);
        let funding_ts = ctx.and_then(|c| c.funding_received_at_ms);
        let meta_ts = ctx.map(|c| c.received_at_ms);
        let candle_ts = candle_series
            .filter(|s| !s.completed.is_empty())
            .map(|s| s.last_received_at_ms);
        let candle_ex_ts = candle_series
            .filter(|s| !s.completed.is_empty())
            .and_then(|s| s.last_exchange_ts_ms);
        let derived_ts =
            if derived.directionality_er.is_some() && derived.coverage_fraction.is_some() {
                candle_ts
            } else {
                None
            };

        let freshness = MarketFreshnessReport {
            mark: FieldFreshness::evaluate(mark_ts, mark_ex_ts, as_of_ms, self.stale_after_ms),
            bid_ask: FieldFreshness::evaluate(book_ts, book_ex_ts, as_of_ms, self.stale_after_ms),
            l2_depth: FieldFreshness::evaluate(book_ts, book_ex_ts, as_of_ms, self.stale_after_ms),
            funding: FieldFreshness::evaluate(
                funding_ts,
                ctx.and_then(|c| c.exchange_ts_ms),
                as_of_ms,
                self.stale_after_ms,
            ),
            instrument_meta: FieldFreshness::evaluate(
                meta_ts,
                ctx.and_then(|c| c.exchange_ts_ms),
                as_of_ms,
                self.stale_after_ms,
            ),
            completed_candles: FieldFreshness::evaluate(
                candle_ts,
                candle_ex_ts,
                as_of_ms,
                self.stale_after_ms,
            ),
            derived_stats: FieldFreshness::evaluate(
                derived_ts,
                candle_ex_ts,
                as_of_ms,
                self.stale_after_ms,
            ),
        };

        let exchange_ts_ms = book
            .and_then(|b| b.exchange_ts_ms)
            .into_iter()
            .chain(ctx.and_then(|c| c.exchange_ts_ms))
            .chain(candle_series.and_then(|s| s.last_exchange_ts_ms))
            .max();
        // Aggregate receipt timestamp conservatively as the oldest present core feed so a
        // fresh L2 update never hides a stale mark, funding, or meta snapshot.
        let received_at_ms = [book_ts, mark_ts, meta_ts, funding_ts]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(0);
        let is_stale = freshness.any_critical_stale();

        let mut missing_fields = Vec::new();
        if book.is_none() {
            missing_fields.push("l2_book".into());
        }
        if depth_usd.is_none() {
            missing_fields.push("depth_usd".into());
        }
        if spread_bps.is_none() {
            missing_fields.push("spread_bps".into());
        }
        if ctx.is_none() {
            missing_fields.push("instrument_meta".into());
        }
        if funding_ts.is_none() {
            missing_fields.push("funding".into());
        }
        if derived.directionality_er.is_none() {
            missing_fields.push("directionality_er".into());
        }
        if derived.volatility_bar_std_pct.is_none() {
            missing_fields.push("volatility_bar_std_pct".into());
        }
        if derived.coverage_fraction.is_none() {
            missing_fields.push("coverage_fraction".into());
        }

        Some(ObservedMarketSnapshot {
            symbol: symbol.to_string(),
            mark_px,
            best_bid,
            best_ask,
            spread_bps,
            depth_usd,
            volume_24h_usd,
            open_interest_usd,
            funding_bps_1h,
            volatility_pct: derived.volatility_bar_std_pct,
            directionality: derived.directionality_er,
            coverage_fraction: derived.coverage_fraction,
            completed_candles,
            observed_history_minutes,
            // Hyperliquid metaAndAssetCtxs does not report instrument listing dates.
            instrument_listing_age_days: None,
            freshness,
            exchange_ts_ms,
            received_at_ms,
            is_stale,
            missing_fields,
            provenance: if book.is_some() || ctx.is_some() {
                DataProvenance::AuthoritativeExchange
            } else {
                DataProvenance::DerivedMarketStat
            },
        })
    }

    pub fn microstructure_snapshot(
        &self,
        symbol: &str,
        as_of_ms: i64,
        probe_notional_usd: f64,
    ) -> crate::hunter_microstructure::MicrostructureSnapshot {
        let book = self.l2_books.get(symbol);
        let depth_hist = self.l2_history.get(symbol);
        let trades = self.recent_trades(symbol);
        let candles = self
            .candles
            .get(symbol)
            .map(CompletedCandleSeries::completed_vec)
            .unwrap_or_default();
        let funding_bps = self
            .meta_ctx
            .get(symbol)
            .and_then(|c| c.funding_received_at_ms.map(|_| c.funding_bps_1h));
        let oi_hist = self.oi_history.get(symbol);
        crate::hunter_microstructure::compute_microstructure_snapshot(
            symbol,
            as_of_ms,
            book,
            depth_hist,
            &trades,
            &candles,
            funding_bps,
            oi_hist,
            probe_notional_usd,
        )
    }

    pub fn cross_market_correlations(
        &self,
        symbols: &[String],
        lookback_bars: usize,
    ) -> Result<crate::hunter_microstructure::CorrelationMatrix> {
        let mut map = BTreeMap::new();
        for sym in symbols {
            if let Some(series) = self.candles.get(sym) {
                map.insert(sym.clone(), series.completed_vec());
            }
        }
        crate::hunter_microstructure::compute_correlation_matrix(&map, lookback_bars)
    }
}

/// Async public WebSocket + REST snapshot ingester supporting both mainnet and loopback URLs.
pub struct HunterStreamClient {
    pub ws_url: String,
    pub info_url: String,
    pub state: Arc<RwLock<HunterMarketState>>,
    pub policy: PublicHunterPolicy,
}

impl HunterStreamClient {
    pub fn new(
        ws_url: impl Into<String>,
        info_url: impl Into<String>,
        stale_after_ms: i64,
    ) -> Self {
        Self {
            ws_url: ws_url.into(),
            info_url: info_url.into(),
            state: Arc::new(RwLock::new(HunterMarketState::new(stale_after_ms))),
            policy: PublicHunterPolicy::default(),
        }
    }

    pub fn production() -> Self {
        Self::new(
            HYPERLIQUID_PUBLIC_WS_URL,
            crate::hunter::HYPERLIQUID_PUBLIC_INFO_URL,
            30_000,
        )
    }

    /// Resolves active monitored symbols = `watchlist ∪ held_positions` when non-empty,
    /// otherwise bootstraps from the top liquid instruments observed in `meta_ctx` (or BTC/ETH/SOL
    /// prior to the initial REST discovery snapshot) so an empty watchlist never drops live streams.
    pub fn resolve_bootstrap_universe(
        &self,
        watchlist: &[String],
        held: &BTreeSet<String>,
    ) -> Vec<String> {
        let explicit = HunterMarketState::monitor_universe(watchlist, held);
        if !explicit.is_empty() {
            return explicit;
        }
        let guard = self.state.read().expect("hunter state lock");
        if !guard.meta_ctx.is_empty() {
            let obs: Vec<PublicMarketObservation> = guard
                .meta_ctx
                .values()
                .map(|c| PublicMarketObservation {
                    symbol: c.symbol.clone(),
                    mark_px: c.mark_px,
                    oracle_px: c.oracle_px,
                    prev_day_px: None,
                    change_24h_pct: None,
                    volume_24h_usd: c.volume_24h_usd,
                    open_interest_usd: c.open_interest_usd,
                    funding_bps_1h: c.funding_bps_1h,
                    impact_spread_bps: c.impact_spread_bps,
                    max_leverage: c.max_leverage,
                })
                .collect();
            if let Ok(rep) = crate::hunter::evaluate_public_markets(
                "bootstrap_discovery",
                &obs,
                &self.policy,
                &[],
                held,
            ) {
                if !rep.watchlist.is_empty() {
                    return rep.watchlist;
                }
            }
            let mut by_vol: Vec<_> = guard.meta_ctx.values().collect();
            by_vol.sort_by(|a, b| {
                b.volume_24h_usd
                    .total_cmp(&a.volume_24h_usd)
                    .then_with(|| a.symbol.cmp(&b.symbol))
            });
            let cap = self.policy.slots.clamp(3, 8);
            let picked: Vec<String> = by_vol
                .into_iter()
                .take(cap)
                .map(|c| c.symbol.clone())
                .collect();
            if !picked.is_empty() {
                return picked;
            }
        }
        vec!["BTC".into(), "ETH".into(), "SOL".into()]
    }

    /// Initializes or refreshes REST snapshots (`metaAndAssetCtxs`, `l2Book`, `candleSnapshot`)
    /// with bounded rate-limit exponential backoff.
    pub async fn refresh_rest_snapshots(
        &self,
        http: &reqwest::Client,
        watched_symbols: &[String],
    ) -> Result<()> {
        let meta_bytes = post_info_with_backoff(
            http,
            &self.info_url,
            serde_json::json!({"type":"metaAndAssetCtxs"}),
            &self.state,
        )
        .await?;
        let now_ms = Utc::now().timestamp_millis();
        {
            let mut guard = self.state.write().expect("hunter state lock");
            guard.apply_meta_and_ctxs_json(&meta_bytes, now_ms)?;
        }

        let target_symbols = if watched_symbols.is_empty() {
            self.resolve_bootstrap_universe(&[], &BTreeSet::new())
        } else {
            watched_symbols.to_vec()
        };
        let start_ms = now_ms - 60 * 60_000;
        for sym in target_symbols.iter().take(12) {
            if let Ok(book_bytes) = post_info_with_backoff(
                http,
                &self.info_url,
                serde_json::json!({"type": "l2Book", "coin": sym}),
                &self.state,
            )
            .await
            {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&book_bytes) {
                    let mut guard = self.state.write().expect("hunter state lock");
                    let _ = guard.apply_l2_book_value(&val, Utc::now().timestamp_millis());
                }
            }
            if let Ok(c_bytes) = post_info_with_backoff(
                http,
                &self.info_url,
                serde_json::json!({
                    "type": "candleSnapshot",
                    "req": { "coin": sym, "interval": "1m", "startTime": start_ms, "endTime": now_ms }
                }),
                &self.state,
            )
            .await
            {
                let mut guard = self.state.write().expect("hunter state lock");
                let _ = guard.apply_candle_snapshot_json(
                    sym,
                    &c_bytes,
                    Utc::now().timestamp_millis(),
                );
            }
        }
        Ok(())
    }

    /// Runs the resilient WebSocket ingestion loop until `stop` becomes `true`,
    /// reconnecting with exponential backoff and managing dynamic `watchlist ∪ held_positions`
    /// subscriptions plus heartbeat `ping`/`pong`.
    pub async fn run_until_stopped(
        &self,
        stop: Arc<AtomicBool>,
        held_positions: Arc<RwLock<BTreeSet<String>>>,
        watchlist: Arc<RwLock<Vec<String>>>,
    ) -> Result<()> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .user_agent(format!("HYDRA-Hunter/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        let mut backoff_s = 1u64;

        while !stop.load(Ordering::Relaxed) {
            let universe = {
                let wl = watchlist.read().expect("watchlist lock").clone();
                let held = held_positions.read().expect("held lock").clone();
                self.resolve_bootstrap_universe(&wl, &held)
            };
            if let Err(e) = self.refresh_rest_snapshots(&http, &universe).await {
                tracing::debug!(error=%e, "initial REST snapshot unavailable; continuing to WS");
            }

            match tokio_tungstenite::connect_async(&self.ws_url).await {
                Ok((ws_stream, _)) => {
                    backoff_s = 1;
                    {
                        let mut guard = self.state.write().expect("hunter lock");
                        guard.health.connected = true;
                        guard.health.fallback_rest_active = false;
                    }
                    let (mut write, mut read) = ws_stream.split();
                    let mut subscribed: BTreeSet<String> = BTreeSet::new();

                    let initial_symbols = {
                        let wl = watchlist.read().expect("watchlist lock").clone();
                        let held = held_positions.read().expect("held lock").clone();
                        self.resolve_bootstrap_universe(&wl, &held)
                    };
                    for sym in &initial_symbols {
                        if send_symbol_subscriptions(&mut write, sym, true)
                            .await
                            .is_ok()
                        {
                            subscribed.insert(sym.clone());
                        }
                    }

                    let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
                    ping_interval.tick().await;
                    let mut last_activity_at = tokio::time::Instant::now();

                    while !stop.load(Ordering::Relaxed) {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(WsMessage::Text(txt))) => {
                                        last_activity_at = tokio::time::Instant::now();
                                        let now_ms = Utc::now().timestamp_millis();
                                        let mut guard = self.state.write().expect("hunter lock");
                                        let _ = guard.apply_ws_message(txt.as_str(), now_ms);
                                    }
                                    Some(Ok(WsMessage::Ping(p))) => {
                                        last_activity_at = tokio::time::Instant::now();
                                        let _ = write.send(WsMessage::Pong(p)).await;
                                    }
                                    Some(Ok(WsMessage::Pong(_))) => {
                                        last_activity_at = tokio::time::Instant::now();
                                        let mut guard = self.state.write().expect("hunter lock");
                                        guard.health.last_pong_ms = Some(Utc::now().timestamp_millis());
                                    }
                                    Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => {
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            _ = ping_interval.tick() => {
                                if last_activity_at.elapsed() > Duration::from_secs(45) {
                                    tracing::warn!("HUNTER WS heartbeat timeout (>45s); reconnecting");
                                    break;
                                }
                                let ping_txt = serde_json::json!({"method":"ping"}).to_string();
                                if write.send(WsMessage::Text(ping_txt.into())).await.is_err() {
                                    break;
                                }
                                let desired: BTreeSet<String> = {
                                    let wl = watchlist.read().expect("watchlist lock").clone();
                                    let held = held_positions.read().expect("held lock").clone();
                                    self.resolve_bootstrap_universe(&wl, &held).into_iter().collect()
                                };
                                for sym in desired.difference(&subscribed).cloned().collect::<Vec<_>>() {
                                    if send_symbol_subscriptions(&mut write, &sym, true).await.is_ok() {
                                        subscribed.insert(sym);
                                    }
                                }
                                for sym in subscribed.difference(&desired).cloned().collect::<Vec<_>>() {
                                    if send_symbol_subscriptions(&mut write, &sym, false).await.is_ok() {
                                        subscribed.remove(&sym);
                                    }
                                }
                            }
                        }
                    }

                    let mut guard = self.state.write().expect("hunter lock");
                    guard.health.connected = false;
                    guard.health.reconnect_count = guard.health.reconnect_count.saturating_add(1);
                }
                Err(e) => {
                    tracing::warn!(error=%e, backoff_s, "HUNTER WebSocket connect failed; falling back to REST");
                    let mut guard = self.state.write().expect("hunter lock");
                    guard.health.connected = false;
                    guard.health.fallback_rest_active = true;
                    guard.health.reconnect_count = guard.health.reconnect_count.saturating_add(1);
                }
            }

            if stop.load(Ordering::Relaxed) {
                break;
            }
            // Conservative REST fallback while reconnecting.
            let _ = self.refresh_rest_snapshots(&http, &universe).await;
            let sleep_until = tokio::time::Instant::now() + Duration::from_secs(backoff_s);
            while tokio::time::Instant::now() < sleep_until && !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
            backoff_s = (backoff_s * 2).min(30);
        }
        Ok(())
    }
}

async fn send_symbol_subscriptions<S>(write: &mut S, symbol: &str, subscribe: bool) -> Result<()>
where
    S: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let method = if subscribe {
        "subscribe"
    } else {
        "unsubscribe"
    };
    let subs = [
        serde_json::json!({ "type": "l2Book", "coin": symbol }),
        serde_json::json!({ "type": "trades", "coin": symbol }),
        serde_json::json!({ "type": "candle", "coin": symbol, "interval": "1m" }),
        serde_json::json!({ "type": "activeAssetCtx", "coin": symbol }),
    ];
    for sub in subs {
        let msg = serde_json::json!({ "method": method, "subscription": sub }).to_string();
        write.send(WsMessage::Text(msg.into())).await?;
    }
    Ok(())
}

async fn post_info_with_backoff(
    http: &reqwest::Client,
    info_url: &str,
    body: serde_json::Value,
    state: &Arc<RwLock<HunterMarketState>>,
) -> Result<Vec<u8>> {
    let mut backoff_ms = 250u64;
    let mut last_err = None;
    for attempt in 0..4 {
        match http.post(info_url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await?;
                ensure!(
                    bytes.len() <= MAX_INFO_PAYLOAD_BYTES,
                    "Hyperliquid /info payload exceeded 4 MiB bound"
                );
                return Ok(bytes.to_vec());
            }
            Ok(resp) => {
                let code = resp.status();
                if code.as_u16() == 429 || code.is_server_error() {
                    if let Ok(mut g) = state.write() {
                        g.health.rate_limit_backoffs =
                            g.health.rate_limit_backoffs.saturating_add(1);
                    }
                }
                last_err = Some(anyhow!("HTTP {code}"));
            }
            Err(e) => last_err = Some(e.into()),
        }
        if attempt < 3 {
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms *= 2;
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("REST snapshot failed")))
}

fn parse_json_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| v.as_f64())
        .filter(|f| f.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn l2_book_computes_true_depth_and_spread_and_never_invents_derived_metrics() {
        let mut st = HunterMarketState::new(30_000);
        let l2 = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "BTC",
                "time": 1_700_000_050_000_i64,
                "levels": [
                    [{"px":"65000.0","sz":"1.5","n":2},{"px":"64990.0","sz":"2.0","n":3}],
                    [{"px":"65010.0","sz":"1.2","n":1},{"px":"65020.0","sz":"3.0","n":4}]
                ]
            }
        })
        .to_string();
        assert_eq!(
            st.apply_ws_message(&l2, 1_700_000_050_100).unwrap(),
            WsEventOutcome::L2Book
        );

        let snap = st
            .observed_market_snapshot("BTC", 1_700_000_055_000)
            .expect("snapshot exists");
        assert_eq!(snap.best_bid, Some(65000.0));
        assert_eq!(snap.best_ask, Some(65010.0));
        assert!((snap.spread_bps.unwrap() - 1.5383).abs() < 0.01);
        // bid_depth = 65000*1.5 + 64990*2.0 = 227480; ask_depth = 65010*1.2 + 65020*3.0 = 273072
        assert!((snap.depth_usd.unwrap() - 227_480.0).abs() < 1e-3);
        assert_eq!(snap.exchange_ts_ms, Some(1_700_000_050_000));
        assert_eq!(snap.received_at_ms, 1_700_000_050_100);
        // No completed candles yet -> derived metrics stay None (NOT 0.55 / 1.0!)
        assert!(snap.directionality.is_none());
        assert!(snap.coverage_fraction.is_none());
        assert!(snap.volatility_pct.is_none());
    }

    #[test]
    fn duplicate_tids_and_out_of_order_trades_and_incomplete_candles_handled_strictly() {
        let mut st = HunterMarketState::new(30_000);
        let t1 = serde_json::json!({
            "channel": "trades",
            "data": [
                {"coin":"ETH","side":"B","px":"3200","sz":"1.0","time":1_700_000_010_000_i64,"tid":101,"hash":"0xaa"},
                {"coin":"ETH","side":"A","px":"3199","sz":"0.5","time":1_700_000_005_000_i64,"tid":102,"hash":"0xbb"},
                {"coin":"ETH","side":"B","px":"3200","sz":"1.0","time":1_700_000_010_000_i64,"tid":101,"hash":"0xaa"}
            ]
        }).to_string();
        assert_eq!(
            st.apply_ws_message(&t1, 1_700_000_010_050).unwrap(),
            WsEventOutcome::Trades(2)
        );
        assert_eq!(st.trade_count("ETH"), 2);
        assert_eq!(st.health.duplicate_trades_dropped, 1);
        assert_eq!(st.health.out_of_order_events, 1);

        // Open candle whose close time T > received_at_ms must NOT pollute completed bars
        let open_candle = serde_json::json!({
            "channel": "candle",
            "data": {"s":"ETH","t":1_700_000_040_000_i64,"T":1_700_000_099_999_i64,"o":"3200","h":"3210","l":"3198","c":"3208","v":"40.0"}
        }).to_string();
        st.apply_ws_message(&open_candle, 1_700_000_050_000)
            .unwrap();
        assert_eq!(st.candles["ETH"].completed.len(), 0);
        assert!(st.candles["ETH"].open_candle.is_some());

        // Completed candle whose close time T <= received_at_ms IS admitted
        let closed_candle = serde_json::json!({
            "channel": "candle",
            "data": {"s":"ETH","t":1_700_000_040_000_i64,"T":1_700_000_099_999_i64,"o":"3200","h":"3212","l":"3198","c":"3211","v":"75.0"}
        }).to_string();
        st.apply_ws_message(&closed_candle, 1_700_000_100_000)
            .unwrap();
        assert_eq!(st.candles["ETH"].completed.len(), 1);
    }

    #[test]
    fn stale_detection_and_portfolio_held_universe_union_work() {
        let mut st = HunterMarketState::new(5_000);
        let l2 = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "SOL",
                "time": 1_000_000_i64,
                "levels": [[{"px":"140.0","sz":"10","n":1}],[{"px":"140.2","sz":"10","n":1}]]
            }
        })
        .to_string();
        st.apply_ws_message(&l2, 1_000_050).unwrap();
        let watched = HunterMarketState::monitor_universe(
            &["BTC".into()],
            &BTreeSet::from(["SOL".to_string()]),
        );
        assert_eq!(watched, vec!["BTC".to_string(), "SOL".to_string()]);
        st.refresh_health_diagnostics(&watched, 1_010_000);
        assert_eq!(st.health.stale_symbols, vec!["SOL".to_string()]);
        assert_eq!(st.health.missing_data_symbols, vec!["BTC".to_string()]);
    }

    #[tokio::test]
    async fn loopback_websocket_reconnects_and_resubscribes_with_live_l2_and_trades() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let rest_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rest_port = rest_listener.local_addr().unwrap().port();
        let rest_handle = tokio::spawn(async move {
            let body = r#"[{"universe":[]},[]]"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            while let Ok((mut sock, _)) = rest_listener.accept().await {
                let mut req_buf = [0u8; 2048];
                let _ = sock.read(&mut req_buf).await;
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        let server_handle = tokio::spawn(async move {
            // Connection 1: push l2Book, then close immediately to force reconnect.
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let _ = ws
                    .send(WsMessage::Text(
                        serde_json::json!({
                            "channel":"l2Book",
                            "data":{
                                "coin":"BTC",
                                "time":1_700_000_100_000_i64,
                                "levels":[[{"px":"64000","sz":"2","n":1}],[{"px":"64010","sz":"2","n":1}]]
                            }
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
                let _ = ws.close(None).await;
            }
            // Connection 2: wait for subscribe messages and send trade.
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                for _ in 0..4 {
                    if let Some(Ok(WsMessage::Text(sub))) = ws.next().await {
                        assert!(sub.contains("\"method\":\"subscribe\""));
                    }
                }
                let _ = ws
                    .send(WsMessage::Text(
                        serde_json::json!({
                            "channel":"trades",
                            "data":[{"coin":"BTC","side":"B","px":"64005","sz":"0.5","time":1_700_000_102_000_i64,"tid":777}]
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        });

        let client = HunterStreamClient::new(
            format!("ws://127.0.0.1:{port}"),
            format!("http://127.0.0.1:{rest_port}/info"),
            30_000,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let held = Arc::new(RwLock::new(BTreeSet::new()));
        let wl = Arc::new(RwLock::new(vec!["BTC".to_string()]));
        let state = client.state.clone();

        let task =
            tokio::spawn(async move { client.run_until_stopped(stop_clone, held, wl).await });

        for _ in 0..160 {
            if state.read().unwrap().trade_count("BTC") >= 1
                && state.read().unwrap().health.reconnect_count >= 1
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        stop.store(true, Ordering::Relaxed);
        let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        server_handle.abort();
        rest_handle.abort();

        let g = state.read().unwrap();
        assert!(g.l2_books.contains_key("BTC"));
        assert_eq!(g.trade_count("BTC"), 1);
        assert!(g.health.reconnect_count >= 1);
    }

    #[test]
    fn fresh_l2_book_never_masks_stale_mark_or_funding_and_history_is_not_days() {
        let mut st = HunterMarketState::new(30_000);
        // Meta & funding arrive at t = 1_000_000_000.
        let meta = r#"[{"universe":[{"name":"BTC","szDecimals":5}]},[{"markPx":"65000","oraclePx":"65000","funding":"0.0001","openInterest":"100","dayNtlVlm":"50000000"}]]"#;
        st.apply_meta_and_ctxs_json(meta.as_bytes(), 1_000_000_000)
            .unwrap();

        // Ingest 12 closed 1m candles through t = 1_000_050_000.
        for k in 0..12i64 {
            let open_ms = 999_300_000 + k * 60_000;
            let close_ms = open_ms + 59_999;
            let bar = serde_json::json!({
                "channel": "candle",
                "data": {"s":"BTC","t":open_ms,"T":close_ms,"o":"64900","h":"65100","l":"64850","c":"65000","v":"100.0"}
            })
            .to_string();
            st.apply_ws_message(&bar, 1_000_050_000).unwrap();
        }

        // Fresh L2 book arrives at t = 1_000_050_000 (40s later, after stale_after_ms=30s).
        let l2 = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "BTC",
                "time": 1_000_050_000_i64,
                "levels": [[{"px":"64995","sz":"5","n":1}],[{"px":"65005","sz":"5","n":1}]]
            }
        })
        .to_string();
        st.apply_ws_message(&l2, 1_000_050_000).unwrap();

        let snap = st
            .observed_market_snapshot("BTC", 1_000_055_000)
            .expect("snapshot exists");
        assert!(!snap.freshness.l2_depth.is_stale);
        assert!(snap.freshness.mark.is_stale);
        assert!(snap.freshness.funding.is_stale);
        assert!(!snap.freshness.mark_is_fresh());
        assert!(snap.is_stale);
        assert_eq!(snap.observed_history_minutes, 12);
        assert!(snap.instrument_listing_age_days.is_none());

        // An activeAssetCtx with markPx only (no funding field) refreshes mark without faking funding freshness.
        let ctx_no_funding = serde_json::json!({
            "channel": "activeAssetCtx",
            "data": {"coin":"BTC","ctx":{"markPx":"65020","openInterest":"100","dayNtlVlm":"50000000"}}
        })
        .to_string();
        st.apply_ws_message(&ctx_no_funding, 1_000_052_000).unwrap();
        let snap2 = st
            .observed_market_snapshot("BTC", 1_000_055_000)
            .expect("snapshot exists");
        assert!(snap2.freshness.mark_is_fresh());
        assert!(snap2.freshness.funding.is_stale);
        assert!(snap2.is_stale);
    }

    #[test]
    fn empty_watchlist_bootstraps_liquid_symbols_without_dropping_streams() {
        let client = HunterStreamClient::new("ws://127.0.0.1:1", "http://127.0.0.1:1/info", 30_000);
        // Empty watchlist with empty meta falls back cleanly to core liquid majors.
        assert_eq!(
            client.resolve_bootstrap_universe(&[], &BTreeSet::new()),
            vec!["BTC".to_string(), "ETH".to_string(), "SOL".to_string()]
        );

        // Once meta_ctx is discovered, empty watchlist discovers top liquid markets dynamically.
        let meta = r#"[
            {"universe":[{"name":"AAA","szDecimals":2},{"name":"BBB","szDecimals":2}]},
            [
                {"markPx":"10","funding":"0.0001","openInterest":"200000","dayNtlVlm":"5000000","impactPxs":["9.999","10.001"]},
                {"markPx":"20","funding":"0.0001","openInterest":"500000","dayNtlVlm":"50000000","impactPxs":["19.998","20.002"]}
            ]
        ]"#;
        client
            .state
            .write()
            .unwrap()
            .apply_meta_and_ctxs_json(meta.as_bytes(), 1_700_000_000_000)
            .unwrap();
        let discovered = client.resolve_bootstrap_universe(&[], &BTreeSet::new());
        assert!(discovered.contains(&"BBB".to_string()));
        assert!(!discovered.is_empty());
    }

    #[test]
    fn field_freshness_rejects_stale_and_future_exchange_timestamps() {
        let now = 1_700_000_060_000i64;
        let stale_after_ms = 15_000i64;

        let fresh = FieldFreshness::evaluate(Some(now), Some(now - 2_000), now, stale_after_ms);
        assert!(!fresh.is_stale);

        let stale_exchange = FieldFreshness::evaluate(
            Some(now),
            Some(now - stale_after_ms - 1),
            now,
            stale_after_ms,
        );
        assert!(stale_exchange.is_stale);

        let future_exchange =
            FieldFreshness::evaluate(Some(now), Some(now + 5_001), now, stale_after_ms);
        assert!(future_exchange.is_stale);
    }
}

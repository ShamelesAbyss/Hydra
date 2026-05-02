//! Shared domain types: candles, signals, order intents, regime.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side { Long, Short }

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self { Side::Long => "LONG", Side::Short => "SHORT" }
    }
    pub fn is_buy(&self) -> bool { matches!(self, Side::Long) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VenueKind { Hyperliquid }

impl VenueKind {
    pub fn as_str(&self) -> &'static str {
        match self { VenueKind::Hyperliquid => "hyperliquid" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub ts: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Regime { Trending, Choppy, Panic, LowLiquidity, VolSpike, NoTrade }

impl Regime {
    pub fn allows_trading(&self) -> bool {
        !matches!(self, Regime::Panic | Regime::NoTrade)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub venue: String,
    pub symbol: String,
    pub mid: f64,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub spread_bps: Option<f64>,
    pub last_update: DateTime<Utc>,
    pub candles: Vec<Candle>,
}

impl MarketSnapshot {
    pub fn is_fresh(&self, max_age_secs: i64) -> bool {
        (Utc::now() - self.last_update).num_seconds() <= max_age_secs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub strategy: String,
    pub venue: String,
    pub symbol: String,
    pub side: Side,
    pub confidence: f64,      // 0..1
    pub expected_edge_bps: f64,
    pub stop_price: f64,
    pub take_profit: f64,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderIntent {
    pub id: String,           // internal uuid
    pub venue: String,
    pub symbol: String,
    pub side: Side,
    pub size: f64,            // in base units (coin qty)
    pub limit_price: Option<f64>,  // None = market
    pub reduce_only: bool,
    pub stop_price: Option<f64>,
    pub take_profit: Option<f64>,
    pub source_signal: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub intent_id: String,
    pub accepted: bool,
    pub venue_order_id: Option<String>,
    pub filled_size: f64,
    pub avg_fill_price: Option<f64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    pub id: i64,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub stop_price: Option<f64>,
    pub take_profit: Option<f64>,
    pub opened_at: DateTime<Utc>,
    pub strategy: Option<String>,
    pub trailing_high: Option<f64>,
}

//! Shared, read-only dashboard state.
//!
//! The engine writes into this struct at the end of every tick; the TUI task
//! reads from it at ~4 fps. All fields are best-effort snapshots — the TUI
//! never reaches into the engine or storage directly and never holds a lock
//! across an await point. Writes are microsecond-scale; if contention ever
//! becomes an issue we switch to `try_write` and skip the update.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

/// Max entries kept in each rolling ring buffer. Chosen so the whole state
/// is well under 100 KB even with every buffer full.
pub const FEED_CAP: usize = 200;
pub const SIGNAL_CAP: usize = 40;
pub const FILL_CAP: usize = 40;
pub const ERROR_CAP: usize = 20;
pub const EQUITY_HISTORY_CAP: usize = 240;
pub const PRICE_HISTORY_CAP: usize = 60;

#[derive(Debug, Clone)]
pub struct MarketRow {
    pub symbol: String,
    pub mid: f64,
    pub spread_bps: Option<f64>,
    pub atr_pct: f64,
    pub regime: String,
    /// v1.5.0: multi-timeframe regime view as "1m/5m/15m" (e.g. "Tr/Tr/Ch").
    pub regime_view: Option<String>,
    /// v1.5.0: per-symbol adaptive size multiplier (mult, sample_count).
    /// `None` when learning is disabled or no samples yet.
    pub adaptive_mult: Option<(f64, i64)>,
    pub price_history: VecDeque<f64>,
    pub last_update: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PositionRow {
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl_usd: f64,
    pub stop_price: Option<f64>,
    pub take_profit: Option<f64>,
    pub opened_at: DateTime<Utc>,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub ts: DateTime<Utc>,
    pub strategy: String,
    pub symbol: String,
    pub side: String,
    pub edge_bps: f64,
    pub outcome: String, // "emitted" | "blocked: ..." | "filled"
}

#[derive(Debug, Clone)]
pub struct FillRow {
    pub ts: DateTime<Utc>,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub price: f64,
    pub kind: String, // "open" | "close_tp" | "close_stop" | "close_time"
    pub realized_usd: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ErrorRow {
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct FeedLine {
    pub ts: DateTime<Utc>,
    pub level: String, // "INFO" | "WARN" | "ERROR" | "DEBUG"
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct RiskView {
    pub default_risk_pct: f64,
    pub max_daily_loss_pct: f64,
    pub max_weekly_loss_pct: f64,
    pub max_consecutive_losses: u32,
    pub max_open_exposure_pct: f64,
    pub max_notional_per_market_usd: f64,
    pub risk_mult: f64,
    pub consec_losses: u32,
    pub open_exposure_usd: f64,
    pub daily_realized_usd: f64,
    pub weekly_realized_usd: f64,
}

#[derive(Debug, Clone, Default)]
pub struct DashState {
    // --- session identity ---
    pub session_id: i64,
    pub started_at: Option<DateTime<Utc>>,
    pub live_trading: bool,
    pub venues: Vec<String>,
    pub symbols: Vec<String>,
    pub version: String,

    // --- liveness ---
    pub tick: u64,
    pub last_heartbeat: Option<DateTime<Utc>>,

    // --- equity ---
    pub equity_usd: f64,
    pub equity_history: VecDeque<(DateTime<Utc>, f64)>,

    // --- pnl ---
    pub session_realized_usd: f64,
    pub session_trades: i64,

    // --- markets / positions ---
    pub markets: Vec<MarketRow>,
    pub positions: Vec<PositionRow>,

    // --- risk snapshot ---
    pub risk: RiskView,

    // --- ring buffers ---
    pub signals: VecDeque<SignalRow>,
    pub fills: VecDeque<FillRow>,
    pub errors: VecDeque<ErrorRow>,
    pub feed: VecDeque<FeedLine>,
}

impl DashState {
    pub fn push_equity(&mut self, eq: f64) {
        if !eq.is_finite() || eq <= 0.0 {
            return;
        }
        self.equity_usd = eq;
        self.equity_history.push_back((Utc::now(), eq));
        while self.equity_history.len() > EQUITY_HISTORY_CAP {
            self.equity_history.pop_front();
        }
    }

    pub fn upsert_market(&mut self, row: MarketRow) {
        if let Some(existing) = self.markets.iter_mut().find(|m| m.symbol == row.symbol) {
            // preserve the existing price-history ring; append the latest mid
            existing.mid = row.mid;
            existing.spread_bps = row.spread_bps;
            existing.atr_pct = row.atr_pct;
            existing.regime = row.regime;
            existing.regime_view = row.regime_view;
            existing.adaptive_mult = row.adaptive_mult;
            existing.last_update = row.last_update;
            existing.price_history.push_back(row.mid);
            while existing.price_history.len() > PRICE_HISTORY_CAP {
                existing.price_history.pop_front();
            }
        } else {
            let mut r = row;
            r.price_history.push_back(r.mid);
            self.markets.push(r);
        }
    }

    pub fn set_positions(&mut self, positions: Vec<PositionRow>) {
        self.positions = positions;
    }

    pub fn push_signal(&mut self, row: SignalRow) {
        self.signals.push_back(row);
        while self.signals.len() > SIGNAL_CAP {
            self.signals.pop_front();
        }
    }
    pub fn push_fill(&mut self, row: FillRow) {
        self.fills.push_back(row);
        while self.fills.len() > FILL_CAP {
            self.fills.pop_front();
        }
    }
    pub fn push_error(&mut self, row: ErrorRow) {
        self.errors.push_back(row);
        while self.errors.len() > ERROR_CAP {
            self.errors.pop_front();
        }
    }
    pub fn push_feed(&mut self, row: FeedLine) {
        self.feed.push_back(row);
        while self.feed.len() > FEED_CAP {
            self.feed.pop_front();
        }
    }
}

pub type SharedDash = Arc<RwLock<DashState>>;

pub fn shared() -> SharedDash {
    Arc::new(RwLock::new(DashState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    }))
}

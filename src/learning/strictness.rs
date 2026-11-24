//! v1.6.0 Adaptive Strictness — three coordinated overrides that prevent
//! the bot from starving in dead/drifting markets while preserving every
//! safety gate.
//!
//! 1. **ATR-aware scaling** — when ATR% is crushed (sleeping market), the
//!    `min_net_edge_bps` gate is loosened a few bps and the trend-filter
//!    deadband widened. When ATR is hot, the gates tighten back. Pure
//!    function of current snapshot.
//!
//! 2. **Drought detector** — counts consecutive trend-filter blocks per
//!    symbol over a rolling window. After N blocks with zero fills the
//!    trend filter is auto-bypassed for that symbol for M hours. The
//!    bot earns its way back into "trend-respecting" mode the moment a
//!    fill clears, or after the bypass window expires.
//!
//! 3. **ε-greedy exploration** — every Nth signal that ONLY fails the
//!    trend filter is allowed through anyway, downsized to 0.5× normal.
//!    Pure data-collection trades, logged distinctly so the user sees them.
//!
//! All three feed into a single `StrictnessAdjustment` consumed by the
//! risk engine. Bounded, configurable, fail-safe — every override has a
//! clamp and the `enabled` master switch returns identity.

use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, VecDeque};

use crate::config::LearningCfg;

/// Per-signal adjustments. The risk engine reads these AFTER the v1.5
/// `SignalDecision` and applies them on top.
#[derive(Debug, Clone, Default)]
pub struct StrictnessAdjustment {
    /// Add this to `min_net_edge_bps` (negative = looser, positive = tighter).
    pub edge_bps_delta: f64,
    /// Override the trend-filter deadband (None = use config default).
    pub trend_band_pct_override: Option<f64>,
    /// If true, the trend filter is bypassed entirely for this signal
    /// (drought detector active for this symbol).
    pub bypass_trend_filter: bool,
    /// If Some, this is an exploration trade — risk engine should multiply
    /// the final size by this factor (typically 0.5×) and log the fill
    /// with an "explored:" prefix.
    pub explore_size_factor: Option<f64>,
    /// Human-readable summary for the cockpit feed.
    pub summary: String,
}

/// Runtime state held by the engine. Tracks recent trend-filter blocks
/// per symbol and current bypass windows. Reset on every fill.
#[derive(Debug, Default)]
pub struct DroughtState {
    /// Per-symbol rolling list of trend-filter block timestamps.
    blocks: HashMap<String, VecDeque<DateTime<Utc>>>,
    /// Per-symbol bypass-until timestamps.
    bypass_until: HashMap<String, DateTime<Utc>>,
    /// Monotonic counter — every Nth eligible signal is explored.
    explore_counter: u64,
}

impl DroughtState {
    /// Record a trend-filter block. Auto-prunes entries older than the
    /// drought window so the count is always rolling.
    pub fn record_trend_block(&mut self, symbol: &str, window_hours: i64) {
        let now = Utc::now();
        let cutoff = now - Duration::hours(window_hours.max(1));
        let entry = self.blocks.entry(symbol.to_string()).or_default();
        entry.push_back(now);
        while entry.front().map_or(false, |t| *t < cutoff) {
            entry.pop_front();
        }
    }

    /// Number of trend-filter blocks for `symbol` within the rolling window.
    /// Auto-prunes stale entries on every read.
    pub fn block_count(&mut self, symbol: &str, window_hours: i64) -> usize {
        let cutoff = Utc::now() - Duration::hours(window_hours.max(1));
        let entry = self.blocks.entry(symbol.to_string()).or_default();
        while entry.front().map_or(false, |t| *t < cutoff) {
            entry.pop_front();
        }
        entry.len()
    }

    /// Mark `symbol` as in-drought; the trend filter will be bypassed
    /// for it until `bypass_hours` from now. Idempotent — calling again
    /// extends the bypass window.
    pub fn engage_bypass(&mut self, symbol: &str, bypass_hours: i64) {
        let until = Utc::now() + Duration::hours(bypass_hours.max(1));
        self.bypass_until.insert(symbol.to_string(), until);
    }

    /// Is `symbol` currently in a drought-bypass window?
    pub fn is_bypassed(&self, symbol: &str) -> bool {
        match self.bypass_until.get(symbol) {
            Some(t) => *t > Utc::now(),
            None => false,
        }
    }

    /// Clear all drought state for `symbol` — called on a fill so the bot
    /// re-engages the trend filter immediately after a successful trade.
    pub fn clear_for(&mut self, symbol: &str) {
        self.blocks.remove(symbol);
        self.bypass_until.remove(symbol);
    }

    /// Returns true if this is the Nth eligible signal (so it should be
    /// the explored one). Increments the counter regardless.
    pub fn should_explore(&mut self, one_in_n: u64) -> bool {
        self.explore_counter = self.explore_counter.wrapping_add(1);
        let n = one_in_n.max(2);
        self.explore_counter % n == 0
    }
}

/// Compute ATR-aware adjustments. Pure function — same ATR + same config
/// always returns the same result. Output is bounded.
pub fn atr_aware(atr_pct: f64, cfg: &LearningCfg) -> (f64, Option<f64>) {
    if !cfg.strictness_enabled || !cfg.atr_aware_scaling || !atr_pct.is_finite() {
        return (0.0, None);
    }
    // Buckets calibrated against the user's overnight log:
    //   ATR ≈ 0.05–0.10 = "sleeping" (loosen the most)
    //   ATR ≈ 0.10–0.20 = "calm"     (loosen modestly)
    //   ATR > 0.50      = "hot"      (tighten slightly)
    if atr_pct < 0.10 {
        (-5.0, Some(1.0)) // Dead market: -5bps gate, 1.0% band
    } else if atr_pct < 0.20 {
        (-3.0, Some(0.8)) // Calm market: -3bps gate, 0.8% band
    } else if atr_pct > 0.50 {
        (2.0, None) // Hot market: +2bps gate, default band
    } else {
        (0.0, None) // Normal market: identity
    }
}

/// Should the trend filter be bypassed for this symbol right now?
/// Returns true only when:
///   - drought detection is enabled, AND
///   - the symbol has been blocked ≥ N times in the rolling window, AND
///   - we're currently inside an engaged bypass window (or we should engage one now)
pub fn drought_check(state: &mut DroughtState, symbol: &str, cfg: &LearningCfg) -> bool {
    if !cfg.strictness_enabled || !cfg.drought_detector {
        return false;
    }
    if state.is_bypassed(symbol) {
        return true;
    }
    let blocks = state.block_count(symbol, cfg.drought_window_hours);
    if blocks >= cfg.drought_block_threshold {
        state.engage_bypass(symbol, cfg.drought_bypass_hours);
        return true;
    }
    false
}

/// Compose the full strictness adjustment for a candidate signal.
/// Called by the engine BEFORE risk evaluation.
pub fn compose(
    state: &mut DroughtState,
    symbol: &str,
    atr_pct: f64,
    cfg: &LearningCfg,
) -> StrictnessAdjustment {
    if !cfg.strictness_enabled {
        return StrictnessAdjustment::default();
    }
    let (edge_delta, band_override) = atr_aware(atr_pct, cfg);
    let bypass = drought_check(state, symbol, cfg);
    let mut summary_parts: Vec<String> = Vec::new();
    if edge_delta != 0.0 {
        summary_parts.push(format!("atr-aware {:+.0}bps", edge_delta));
    }
    if let Some(b) = band_override {
        summary_parts.push(format!("band→{:.1}%", b));
    }
    if bypass {
        summary_parts.push("trend-bypass (drought)".into());
    }
    StrictnessAdjustment {
        edge_bps_delta: edge_delta,
        trend_band_pct_override: band_override,
        bypass_trend_filter: bypass,
        explore_size_factor: None, // set later if signal *only* fails trend filter
        summary: if summary_parts.is_empty() {
            "strictness: identity".into()
        } else {
            format!("strictness: {}", summary_parts.join(" · "))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_default() -> LearningCfg {
        LearningCfg::default()
    }

    #[test]
    fn identity_when_strictness_disabled() {
        let mut cfg = cfg_default();
        cfg.strictness_enabled = false;
        let (delta, band) = atr_aware(0.05, &cfg);
        assert_eq!(delta, 0.0);
        assert!(band.is_none());
    }

    #[test]
    fn dead_market_loosens_gates() {
        let cfg = cfg_default();
        let (delta, band) = atr_aware(0.05, &cfg);
        assert_eq!(delta, -5.0);
        assert_eq!(band, Some(1.0));
    }

    #[test]
    fn calm_market_loosens_modestly() {
        let cfg = cfg_default();
        let (delta, band) = atr_aware(0.15, &cfg);
        assert_eq!(delta, -3.0);
        assert_eq!(band, Some(0.8));
    }

    #[test]
    fn normal_market_is_identity() {
        let cfg = cfg_default();
        let (delta, band) = atr_aware(0.30, &cfg);
        assert_eq!(delta, 0.0);
        assert!(band.is_none());
    }

    #[test]
    fn hot_market_tightens() {
        let cfg = cfg_default();
        let (delta, band) = atr_aware(0.80, &cfg);
        assert_eq!(delta, 2.0);
        assert!(band.is_none());
    }

    #[test]
    fn drought_engages_after_threshold_blocks() {
        let mut state = DroughtState::default();
        let cfg = cfg_default();
        // Below threshold → no bypass.
        for _ in 0..(cfg.drought_block_threshold - 1) {
            state.record_trend_block("BTC", cfg.drought_window_hours);
        }
        assert!(!drought_check(&mut state, "BTC", &cfg));
        // One more block → triggers bypass.
        state.record_trend_block("BTC", cfg.drought_window_hours);
        assert!(
            drought_check(&mut state, "BTC", &cfg),
            "should engage bypass at threshold"
        );
        // Subsequent calls within the window remain bypassed.
        assert!(drought_check(&mut state, "BTC", &cfg));
    }

    #[test]
    fn drought_isolates_per_symbol() {
        let mut state = DroughtState::default();
        let cfg = cfg_default();
        for _ in 0..cfg.drought_block_threshold {
            state.record_trend_block("BTC", cfg.drought_window_hours);
        }
        assert!(drought_check(&mut state, "BTC", &cfg));
        assert!(
            !drought_check(&mut state, "ETH", &cfg),
            "ETH should not be bypassed because BTC was"
        );
    }

    #[test]
    fn fill_clears_drought_state() {
        let mut state = DroughtState::default();
        let cfg = cfg_default();
        for _ in 0..cfg.drought_block_threshold {
            state.record_trend_block("BTC", cfg.drought_window_hours);
        }
        assert!(drought_check(&mut state, "BTC", &cfg));
        state.clear_for("BTC");
        assert!(!state.is_bypassed("BTC"), "fill should clear bypass window");
        assert_eq!(
            state.block_count("BTC", cfg.drought_window_hours),
            0,
            "fill should clear block history"
        );
    }

    #[test]
    fn explore_counter_fires_one_in_n() {
        let mut state = DroughtState::default();
        let n = 4;
        let mut explored = 0;
        for _ in 0..16 {
            if state.should_explore(n) {
                explored += 1;
            }
        }
        // 16 calls / 4 = 4 explorations
        assert_eq!(explored, 4);
    }

    #[test]
    fn explore_counter_min_n_is_two() {
        // Even when caller passes 0 or 1, we never make EVERY signal an
        // exploration — that would defeat the purpose.
        let mut state = DroughtState::default();
        let mut explored = 0;
        for _ in 0..10 {
            if state.should_explore(0) {
                explored += 1;
            }
        }
        assert!(explored < 10, "should_explore must clamp n ≥ 2");
    }

    #[test]
    fn compose_summary_describes_active_overrides() {
        let mut state = DroughtState::default();
        let cfg = cfg_default();
        let adj = compose(&mut state, "BTC", 0.05, &cfg); // dead market
        assert!(adj.summary.contains("atr-aware"));
        assert!(adj.summary.contains("band"));
        assert_eq!(adj.edge_bps_delta, -5.0);
    }

    #[test]
    fn compose_returns_identity_when_disabled() {
        let mut state = DroughtState::default();
        let mut cfg = cfg_default();
        cfg.strictness_enabled = false;
        let adj = compose(&mut state, "BTC", 0.05, &cfg);
        assert_eq!(adj.edge_bps_delta, 0.0);
        assert!(adj.trend_band_pct_override.is_none());
        assert!(!adj.bypass_trend_filter);
        assert!(adj.summary.contains("identity") || adj.summary.is_empty());
    }
}

//! Mean-reversion strategy: Bollinger band touches + RSI extremes,
//! only when regime is NOT trending.

use chrono::Utc;

use crate::config::{AppConfig, MeanReversionCfg};
use crate::core::{MarketSnapshot, Regime, Side, Signal};
use crate::strategies::Strategy;
use crate::utils::math;

pub struct MeanReversionStrategy { cfg: MeanReversionCfg }
impl MeanReversionStrategy { pub fn new(cfg: MeanReversionCfg) -> Self { Self { cfg } } }

impl Strategy for MeanReversionStrategy {
    fn name(&self) -> &'static str { "mean_reversion" }

    fn evaluate(&self, snap: &MarketSnapshot, regime: Regime, app: &AppConfig) -> Option<Signal> {
        if matches!(regime, Regime::Trending | Regime::Panic | Regime::NoTrade) { return None; }
        let c = &snap.candles;
        if c.len() < self.cfg.bb_period + 5 { return None; }
        let closes: Vec<f64> = c.iter().map(|k| k.close).collect();
        let (mid_bb, up, lo) = math::bollinger(&closes, self.cfg.bb_period, self.cfg.bb_std);
        let last = closes[closes.len() - 1];
        let rsi_now = *math::rsi_series(&closes, 14).last().unwrap_or(&50.0);

        let side = if last <= lo && rsi_now < self.cfg.rsi_floor {
            Side::Long
        } else if last >= up && rsi_now > self.cfg.rsi_ceiling {
            Side::Short
        } else { return None; };

        let atr = math::atr(c, 14);
        let stop_mult = app.strategies.position_management.stop_loss_atr_mult;
        let min_stop_bps = app.strategies.position_management.min_stop_bps;
        // v1.6.1: noise-floor stop. Prevents bid/ask chop on low-ATR markets.
        let stop_dist = math::stop_distance_floored(
            atr, stop_mult, last, snap.spread_bps, min_stop_bps,
        );
        let (stop, tp) = match side {
            Side::Long  => (last - stop_dist, mid_bb),
            Side::Short => (last + stop_dist, mid_bb),
        };
        let edge_bps = ((tp - last).abs() / last * 10_000.0) - snap.spread_bps.unwrap_or(2.0) - 6.0;
        // v1.6.2: strategy-level edge floor REMOVED. Was a redundant
        // double-screen with the risk engine's `min_net_edge_bps`. Strategies
        // should emit any geometrically-valid signal; risk decides on edge.
        // Negative-edge signals still skipped (math doesn't make sense).
        if edge_bps <= 0.0 { return None; }

        Some(Signal {
            strategy: "mean_reversion".into(),
            venue: snap.venue.clone(),
            symbol: snap.symbol.clone(),
            side,
            confidence: 0.45,
            expected_edge_bps: edge_bps,
            stop_price: stop,
            take_profit: tp,
            reason: format!("BB({},{:.1}) touch + RSI={:.1}", self.cfg.bb_period, self.cfg.bb_std, rsi_now),
            created_at: Utc::now(),
        })
    }
}

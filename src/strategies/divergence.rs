//! Divergence / convergence strategy: detects RSI divergence vs price,
//! requires an expected edge after cost assumption.

use chrono::Utc;

use crate::config::{AppConfig, DivergenceCfg};
use crate::core::{MarketSnapshot, Regime, Side, Signal};
use crate::strategies::Strategy;
use crate::utils::math;

pub struct DivergenceStrategy { cfg: DivergenceCfg }

impl DivergenceStrategy {
    pub fn new(cfg: DivergenceCfg) -> Self { Self { cfg } }
}

impl Strategy for DivergenceStrategy {
    fn name(&self) -> &'static str { "divergence" }

    fn evaluate(&self, snap: &MarketSnapshot, regime: Regime, app: &AppConfig) -> Option<Signal> {
        if matches!(regime, Regime::Panic | Regime::NoTrade) { return None; }
        let candles = &snap.candles;
        if candles.len() < self.cfg.rsi_period * 3 { return None; }
        let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
        let rsi = math::rsi_series(&closes, self.cfg.rsi_period);
        let n = closes.len();
        if rsi.len() < 10 { return None; }

        // Bullish divergence: price made lower low over last 5 vs prior 10
        //                    while RSI made higher low.
        let recent_lo_idx = math::argmin(&closes[n.saturating_sub(5)..]);
        let prior_lo_idx  = math::argmin(&closes[n.saturating_sub(20)..n.saturating_sub(5)]);
        let price_recent = closes[n - 5 + recent_lo_idx];
        let price_prior  = closes[n - 20 + prior_lo_idx];
        let rsi_recent   = rsi[n - 5 + recent_lo_idx];
        let rsi_prior    = rsi[n - 20 + prior_lo_idx];

        let bullish_div = price_recent < price_prior && (rsi_recent - rsi_prior) > self.cfg.rsi_threshold;
        let bearish_div = {
            let recent_hi_idx = math::argmax(&closes[n.saturating_sub(5)..]);
            let prior_hi_idx  = math::argmax(&closes[n.saturating_sub(20)..n.saturating_sub(5)]);
            let p_r = closes[n - 5 + recent_hi_idx];
            let p_p = closes[n - 20 + prior_hi_idx];
            let r_r = rsi[n - 5 + recent_hi_idx];
            let r_p = rsi[n - 20 + prior_hi_idx];
            p_r > p_p && (r_p - r_r) > self.cfg.rsi_threshold
        };

        let side = if bullish_div { Side::Long } else if bearish_div { Side::Short } else { return None };
        let atr = math::atr(candles, 14);
        if atr <= 0.0 { return None; }
        let mid = snap.mid;
        let stop_mult = app.strategies.position_management.stop_loss_atr_mult;
        let tp_mult   = app.strategies.position_management.take_profit_atr_mult;
        let min_stop_bps = app.strategies.position_management.min_stop_bps;
        // v1.6.1: noise-floor stop; TP scales with stop so R:R is preserved.
        let stop_dist = math::stop_distance_floored(
            atr, stop_mult, mid, snap.spread_bps, min_stop_bps,
        );
        let tp_dist = stop_dist * (tp_mult / stop_mult.max(1e-9));
        let (stop, tp) = match side {
            Side::Long  => (mid - stop_dist, mid + tp_dist),
            Side::Short => (mid + stop_dist, mid - tp_dist),
        };

        // Expected edge: reward/risk * some prior * minus fees/spread assumption.
        let reward = (tp - mid).abs() / mid * 10_000.0;
        let risk   = (mid - stop).abs() / mid * 10_000.0;
        let edge_bps = (reward - risk * 0.6) - snap.spread_bps.unwrap_or(2.0) - 6.0; // 6bps taker
        // v1.6.2: strategy-level edge floor (`self.cfg.min_edge_bps`) REMOVED.
        // Risk engine's `min_net_edge_bps` is now the single source of truth.
        // Negative-edge signals still skipped (math doesn't make sense).
        if edge_bps <= 0.0 { return None; }

        Some(Signal {
            strategy: "divergence".into(),
            venue: snap.venue.clone(),
            symbol: snap.symbol.clone(),
            side,
            confidence: 0.55_f64.min(0.3 + edge_bps / 40.0),
            expected_edge_bps: edge_bps,
            stop_price: stop,
            take_profit: tp,
            reason: format!("RSI divergence; atr={:.4}, edge={:.1}bps", atr, edge_bps),
            created_at: Utc::now(),
        })
    }
}

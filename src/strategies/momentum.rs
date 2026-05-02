//! Momentum / breakout strategy: EMA crossover + prior-range breakout,
//! requires a minimum ATR ratio to avoid choppy conditions.

use chrono::Utc;

use crate::config::{AppConfig, MomentumCfg};
use crate::core::{MarketSnapshot, Regime, Side, Signal};
use crate::strategies::Strategy;
use crate::utils::math;

pub struct MomentumStrategy { cfg: MomentumCfg }

impl MomentumStrategy {
    pub fn new(cfg: MomentumCfg) -> Self { Self { cfg } }
}

impl Strategy for MomentumStrategy {
    fn name(&self) -> &'static str { "momentum" }

    fn evaluate(&self, snap: &MarketSnapshot, regime: Regime, app: &AppConfig) -> Option<Signal> {
        if !matches!(regime, Regime::Trending | Regime::VolSpike) { return None; }
        let c = &snap.candles;
        if c.len() < (self.cfg.slow_ema + self.cfg.breakout_lookback + 5) { return None; }
        let closes: Vec<f64> = c.iter().map(|k| k.close).collect();
        let fast = math::ema(&closes, self.cfg.fast_ema);
        let slow = math::ema(&closes, self.cfg.slow_ema);
        let n = closes.len();
        let fast_now = *fast.last()?;
        let slow_now = *slow.last()?;
        let fast_prev = fast[n.saturating_sub(2)];
        let slow_prev = slow[n.saturating_sub(2)];

        let atr = math::atr(c, self.cfg.atr_period);
        let atr_pct = if snap.mid > 0.0 { atr / snap.mid * 100.0 } else { 0.0 };
        if atr_pct < self.cfg.min_atr_pct { return None; }

        let recent_high = closes[n.saturating_sub(self.cfg.breakout_lookback)..n.saturating_sub(1)]
            .iter().cloned().fold(f64::MIN, f64::max);
        let recent_low = closes[n.saturating_sub(self.cfg.breakout_lookback)..n.saturating_sub(1)]
            .iter().cloned().fold(f64::MAX, f64::min);
        let last = closes[n - 1];

        let bull = fast_now > slow_now && fast_prev <= slow_prev && last > recent_high;
        let bear = fast_now < slow_now && fast_prev >= slow_prev && last < recent_low;
        let side = if bull { Side::Long } else if bear { Side::Short } else { return None; };

        let stop_mult = app.strategies.position_management.stop_loss_atr_mult;
        let tp_mult   = app.strategies.position_management.take_profit_atr_mult;
        let min_stop_bps = app.strategies.position_management.min_stop_bps;
        // v1.6.1: noise-floor stop + symmetric TP scaled by the same ratio
        // so R:R targets remain stable when the floor activates.
        let stop_dist = crate::utils::math::stop_distance_floored(
            atr, stop_mult, last, snap.spread_bps, min_stop_bps,
        );
        let tp_dist = stop_dist * (tp_mult / stop_mult.max(1e-9));
        let (stop, tp) = match side {
            Side::Long  => (last - stop_dist, last + tp_dist),
            Side::Short => (last + stop_dist, last - tp_dist),
        };
        let edge_bps = ((tp - last).abs() / last * 10_000.0) * 0.5
                     - snap.spread_bps.unwrap_or(2.0) - 6.0;
        // v1.6.2: strategy-level edge floor REMOVED. Risk engine's
        // `min_net_edge_bps` is now the single source of truth.
        if edge_bps <= 0.0 { return None; }

        Some(Signal {
            strategy: "momentum".into(),
            venue: snap.venue.clone(),
            symbol: snap.symbol.clone(),
            side,
            confidence: (0.35 + atr_pct * 0.4).min(0.8),
            expected_edge_bps: edge_bps,
            stop_price: stop,
            take_profit: tp,
            reason: format!(
                "EMA{}/{} crossover + {}bar {} breakout, atr%={:.2}",
                self.cfg.fast_ema, self.cfg.slow_ema,
                self.cfg.breakout_lookback, if bull {"high"} else {"low"}, atr_pct
            ),
            created_at: Utc::now(),
        })
    }
}

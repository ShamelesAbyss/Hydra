//! Market regime classifier.

use crate::config::RegimeCfg;
use crate::core::{Candle, Regime};
use crate::utils::math;

pub fn classify_regime(candles: &[Candle], cfg: &RegimeCfg) -> Regime {
    if candles.len() < cfg.trend_ema_period + 30 { return Regime::NoTrade; }

    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let ema_long = math::ema(&closes, cfg.trend_ema_period);
    let last = closes[closes.len()-1];
    let ema_last = *ema_long.last().unwrap_or(&last);

    // ATR% over mid
    let atr = math::atr(candles, 14);
    let atr_pct = if last > 0.0 { atr / last * 100.0 } else { 0.0 };

    // Rolling mean ATR over prior 60 candles to detect a volatility spike.
    let atr_series = math::atr_series(candles, 14);
    let n = atr_series.len();
    let prior_mean_atr = if n > 70 {
        let window = &atr_series[n.saturating_sub(70)..n.saturating_sub(10)];
        let m = window.iter().sum::<f64>() / window.len() as f64;
        m
    } else { atr };
    let vol_mult = if prior_mean_atr > 0.0 { atr / prior_mean_atr } else { 1.0 };

    if atr_pct >= cfg.panic_atr_threshold {
        return Regime::Panic;
    }
    if vol_mult >= cfg.vol_spike_mult {
        return Regime::VolSpike;
    }
    if atr_pct <= cfg.chop_atr_threshold {
        return Regime::Choppy;
    }
    // Trend check: persistent side vs EMA
    let above = closes.iter().rev().take(20).filter(|&&c| c > ema_last).count();
    let below = closes.iter().rev().take(20).filter(|&&c| c < ema_last).count();
    if above >= 16 || below >= 16 {
        return Regime::Trending;
    }
    Regime::Choppy
}

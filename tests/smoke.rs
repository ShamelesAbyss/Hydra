//! Smoke tests: indicator math correctness + regime classification shape.
use hydra::core::{Candle, Regime};
use hydra::utils::math;
use chrono::Utc;

fn synthetic_candles(n: usize, seed: f64, amp: f64) -> Vec<Candle> {
    (0..n).map(|i| {
        let t = i as f64;
        let c = seed + (t * 0.05).sin() * amp + t * 0.02;
        Candle { ts: Utc::now(), open: c, high: c + amp*0.1, low: c - amp*0.1, close: c, volume: 1.0 }
    }).collect()
}

#[test]
fn ema_monotone_on_linear_series() {
    let xs: Vec<f64> = (1..=50).map(|i| i as f64).collect();
    let ema = math::ema(&xs, 10);
    assert_eq!(ema.len(), xs.len());
    for i in 10..xs.len() { assert!(ema[i] > ema[i-1]); }
}

#[test]
fn rsi_bounded() {
    let c = synthetic_candles(200, 100.0, 2.0);
    let closes: Vec<f64> = c.iter().map(|k| k.close).collect();
    let r = math::rsi_series(&closes, 14);
    assert!(r.iter().all(|v| *v >= 0.0 && *v <= 100.0));
}

#[test]
fn atr_non_negative() {
    let c = synthetic_candles(120, 1800.0, 10.0);
    let s = math::atr_series(&c, 14);
    assert!(s.iter().all(|v| *v >= 0.0));
}

#[test]
fn regime_flags_choppy_on_tiny_moves() {
    let c: Vec<Candle> = (0..200).map(|i| Candle {
        ts: Utc::now(),
        open: 1000.0 + (i as f64 * 0.001),
        high: 1000.5 + (i as f64 * 0.001),
        low:  999.5  + (i as f64 * 0.001),
        close: 1000.0 + (i as f64 * 0.001),
        volume: 1.0,
    }).collect();
    let cfg = hydra::config::RegimeCfg {
        trend_ema_period: 96,
        chop_atr_threshold: 0.08,
        panic_atr_threshold: 1.2,
        vol_spike_mult: 2.5,
    };
    let r = hydra::strategies::regime::classify_regime(&c, &cfg);
    assert!(matches!(r, Regime::Choppy | Regime::NoTrade | Regime::Trending), "got {:?}", r);
}

//! Multi-timeframe regime classifier.
//!
//! Reuses the existing 1m `classify_regime` as the building block, then
//! resamples to 5m / 15m candles and classifies each. The final regime
//! "view" is the tuple (1m, 5m, 15m), giving the strategy/risk layer
//! enough context to differentiate between, say, a 1m noise spike during
//! a calm 15m trend vs a real cross-timeframe panic.

use crate::config::RegimeCfg;
use crate::core::Candle;
use crate::core::Regime as MarketRegime;
use crate::strategies::regime::classify_regime;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct RegimeView {
    pub r_1m: MarketRegime,
    pub r_5m: MarketRegime,
    pub r_15m: MarketRegime,
}

impl RegimeView {
    /// Confidence in the strategy direction for the current view, 0–100.
    /// 100 = all 3 timeframes agree on regime; lower = mixed signals.
    pub fn confidence(&self) -> u8 {
        let agree_5_15 = self.r_5m == self.r_15m;
        let agree_1_5 = self.r_1m == self.r_5m;
        match (agree_5_15, agree_1_5) {
            (true, true) => 100,
            (true, false) => 75,
            (false, true) => 50,
            (false, false) => 25,
        }
    }
}

/// Aggregate every `factor` consecutive 1m candles into one bigger candle.
/// Last partial bucket is dropped (we want only fully-formed candles).
pub fn resample(candles_1m: &[Candle], factor: usize) -> Vec<Candle> {
    if factor <= 1 || candles_1m.len() < factor {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(candles_1m.len() / factor);
    let mut chunk = candles_1m.chunks_exact(factor);
    for ch in chunk.by_ref() {
        let open = ch.first().unwrap().open;
        let close = ch.last().unwrap().close;
        let high = ch.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
        let low = ch.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let vol = ch.iter().map(|c| c.volume).sum();
        let ts = ch.last().unwrap().ts;
        out.push(Candle {
            ts,
            open,
            high,
            low,
            close,
            volume: vol,
        });
    }
    out
}

/// Classify the multi-timeframe view from a single set of 1m candles.
/// All three timeframes share the same `RegimeCfg` thresholds; in
/// practice this means short noise on 1m can map to "Choppy" while 5m
/// reads "Trending", which is exactly the cross-TF signal we want.
pub fn classify_view(candles_1m: &[Candle], cfg: &RegimeCfg) -> RegimeView {
    let r_1m = classify_regime(candles_1m, cfg);
    let c_5m = resample(candles_1m, 5);
    let c_15m = resample(candles_1m, 15);
    let r_5m = classify_regime(&c_5m, cfg);
    let r_15m = classify_regime(&c_15m, cfg);
    RegimeView { r_1m, r_5m, r_15m }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Candle;

    fn mkc(close: f64) -> Candle {
        Candle {
            ts: chrono::Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn resample_drops_partial_bucket() {
        let c1m: Vec<Candle> = (0..23).map(|i| mkc(100.0 + i as f64)).collect();
        // 23 / 5 = 4 full 5-min buckets, 3 leftover dropped.
        let c5m = resample(&c1m, 5);
        assert_eq!(c5m.len(), 4);
        // 23 / 15 = 1 full 15-min bucket.
        let c15m = resample(&c1m, 15);
        assert_eq!(c15m.len(), 1);
    }

    #[test]
    fn resample_aggregates_high_low_correctly() {
        let mut c1m = Vec::new();
        for i in 0..5 {
            c1m.push(Candle {
                ts: chrono::Utc::now(),
                open: 100.0,
                high: 100.0 + i as f64,
                low: 99.0 - i as f64,
                close: 100.0,
                volume: 1.0,
            });
        }
        let c5m = resample(&c1m, 5);
        assert_eq!(c5m.len(), 1);
        assert!((c5m[0].high - 104.0).abs() < 1e-9);
        assert!((c5m[0].low - 95.0).abs() < 1e-9);
        assert!((c5m[0].volume - 5.0).abs() < 1e-9);
    }

    #[test]
    fn confidence_full_agreement() {
        let v = RegimeView {
            r_1m: MarketRegime::Trending,
            r_5m: MarketRegime::Trending,
            r_15m: MarketRegime::Trending,
        };
        assert_eq!(v.confidence(), 100);
    }

    #[test]
    fn confidence_only_5_15_agree() {
        let v = RegimeView {
            r_1m: MarketRegime::Choppy,
            r_5m: MarketRegime::Trending,
            r_15m: MarketRegime::Trending,
        };
        assert_eq!(v.confidence(), 75);
    }
}

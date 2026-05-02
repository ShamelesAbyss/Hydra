//! Rolling expectancy: per-symbol, per-strategy, per-(regime × symbol × strategy).
//!
//! "Expectancy" here = average realized R per trade over the rolling window.
//! Above 0 means the bucket has been winning, below 0 means losing. We only
//! act on samples ≥ `MIN_SAMPLES`; smaller samples → confidence reduces and
//! we revert toward the neutral 1.0× multiplier.

use anyhow::Result;
use rusqlite::params;

use crate::storage::Storage;

/// Below this many samples, we don't trust the bucket's expectancy enough
/// to deviate from neutral (1.0× multiplier, baseline edge threshold).
pub const MIN_SAMPLES: i64 = 8;

/// Rolling window: only the most recent N closes contribute.
pub const ROLLING_WINDOW: i64 = 50;

#[derive(Debug, Clone, Copy)]
pub struct Expectancy {
    pub samples: i64,
    /// Average realized R per trade over the window.
    pub avg_r: f64,
    /// Net realized P&L (USD) over the window.
    pub net_pnl: f64,
    /// Win rate over the window, 0.0 – 1.0.
    pub win_rate: f64,
    /// Confidence in the estimate, 0.0 – 1.0. Pure-Bayesian-flavoured:
    /// `samples / (samples + MIN_SAMPLES)` so 8 samples = 0.5 confidence,
    /// 50 samples ≈ 0.86, etc. Used to attenuate adjustments.
    pub confidence: f64,
}

impl Default for Expectancy {
    fn default() -> Self {
        Self { samples: 0, avg_r: 0.0, net_pnl: 0.0, win_rate: 0.0, confidence: 0.0 }
    }
}

impl Expectancy {
    /// Translate this expectancy into a multiplier in [0.5, 1.5]. Samples
    /// below MIN_SAMPLES are pulled towards 1.0 by the confidence factor.
    /// The mapping is monotonic: more positive R → larger multiplier.
    pub fn to_multiplier(&self) -> f64 {
        if self.samples < MIN_SAMPLES { return 1.0; }
        // Map avg_r to a delta in [-0.5, 0.5]:
        //   avg_r = +1R → delta = +0.5  (max 1.5×)
        //   avg_r = -1R → delta = -0.5  (min 0.5×)
        let raw_delta = self.avg_r.clamp(-1.0, 1.0) * 0.5;
        let attenuated = raw_delta * self.confidence;
        (1.0 + attenuated).clamp(0.5, 1.5)
    }

    pub fn from_rows(rows: Vec<(f64, f64, f64)>) -> Self {
        // rows = (realized_r, realized_pnl, r_init). Filter out malformed.
        let valid: Vec<(f64, f64)> = rows.into_iter()
            .filter(|(r, p, ri)| r.is_finite() && p.is_finite() && *ri > 0.0)
            .map(|(r, p, _)| (r, p))
            .collect();
        let samples = valid.len() as i64;
        if samples == 0 { return Self::default(); }
        let avg_r = valid.iter().map(|(r, _)| *r).sum::<f64>() / samples as f64;
        let net_pnl = valid.iter().map(|(_, p)| *p).sum();
        let wins = valid.iter().filter(|(_, p)| *p > 0.0).count() as f64;
        let win_rate = wins / samples as f64;
        let confidence = samples as f64 / (samples as f64 + MIN_SAMPLES as f64);
        Self { samples, avg_r, net_pnl, win_rate, confidence }
    }
}

/// Pull rolling expectancy for a symbol over the last `ROLLING_WINDOW` closes.
pub fn for_symbol(storage: &Storage, symbol: &str) -> Result<Expectancy> {
    let rows: Vec<(f64,f64,f64)> = storage.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT realized_r, realized_pnl, r_init
             FROM trade_context WHERE symbol = ?1
             ORDER BY id DESC LIMIT ?2"
        ).ok()?;
        let rows: Vec<(f64,f64,f64)> = stmt
            .query_map(params![symbol, ROLLING_WINDOW], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .ok()?
            .filter_map(|r| r.ok()).collect();
        Some(rows)
    }).unwrap_or_default();
    Ok(Expectancy::from_rows(rows))
}

pub fn for_strategy(storage: &Storage, strategy: &str) -> Result<Expectancy> {
    let rows: Vec<(f64,f64,f64)> = storage.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT realized_r, realized_pnl, r_init
             FROM trade_context WHERE strategy = ?1
             ORDER BY id DESC LIMIT ?2"
        ).ok()?;
        let rows: Vec<(f64,f64,f64)> = stmt
            .query_map(params![strategy, ROLLING_WINDOW], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .ok()?
            .filter_map(|r| r.ok()).collect();
        Some(rows)
    }).unwrap_or_default();
    Ok(Expectancy::from_rows(rows))
}

/// Per-(regime × symbol × strategy) expectancy. The Regime Brain.
pub fn for_regime_combo(
    storage: &Storage,
    regime_5m: &str,
    symbol: &str,
    strategy: &str,
) -> Result<Expectancy> {
    let rows: Vec<(f64,f64,f64)> = storage.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT realized_r, realized_pnl, r_init
             FROM trade_context
             WHERE regime_5m = ?1 AND symbol = ?2 AND strategy = ?3
             ORDER BY id DESC LIMIT ?4"
        ).ok()?;
        let rows: Vec<(f64,f64,f64)> = stmt
            .query_map(params![regime_5m, symbol, strategy, ROLLING_WINDOW],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .ok()?
            .filter_map(|r| r.ok()).collect();
        Some(rows)
    }).unwrap_or_default();
    Ok(Expectancy::from_rows(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_sample_returns_neutral_multiplier() {
        let e = Expectancy::from_rows(vec![(1.0, 1.0, 1.0); 3]);
        assert!(e.samples < MIN_SAMPLES);
        assert_eq!(e.to_multiplier(), 1.0);
    }

    #[test]
    fn winning_bucket_with_full_sample_lifts_multiplier() {
        // 50 trades all winning at +0.5R → avg_r = 0.5, confidence ~0.86,
        // delta = 0.5 * 0.5 * 0.86 = 0.215, mult = 1.215
        let rows: Vec<(f64,f64,f64)> = (0..50).map(|_| (0.5, 1.0, 1.0)).collect();
        let e = Expectancy::from_rows(rows);
        let m = e.to_multiplier();
        assert!(m > 1.1 && m < 1.3, "got {}", m);
    }

    #[test]
    fn losing_bucket_with_full_sample_lowers_multiplier() {
        let rows: Vec<(f64,f64,f64)> = (0..50).map(|_| (-0.5, -1.0, 1.0)).collect();
        let e = Expectancy::from_rows(rows);
        let m = e.to_multiplier();
        assert!(m < 0.9 && m > 0.7, "got {}", m);
    }

    #[test]
    fn extreme_winning_bucket_clamps_at_1_5() {
        // 50× +5R closes shouldn't blow past the cap.
        let rows: Vec<(f64,f64,f64)> = (0..50).map(|_| (5.0, 10.0, 1.0)).collect();
        let e = Expectancy::from_rows(rows);
        assert!(e.to_multiplier() <= 1.5);
    }

    #[test]
    fn extreme_losing_bucket_clamps_at_0_5() {
        let rows: Vec<(f64,f64,f64)> = (0..50).map(|_| (-5.0, -10.0, 1.0)).collect();
        let e = Expectancy::from_rows(rows);
        assert!(e.to_multiplier() >= 0.5);
    }

    #[test]
    fn malformed_rows_filtered_out() {
        let rows = vec![
            (1.0, 1.0, 1.0),
            (f64::NAN, 1.0, 1.0),
            (1.0, f64::INFINITY, 1.0),
            (1.0, 1.0, 0.0),       // r_init = 0 → invalid
            (1.0, 1.0, -1.0),      // negative r_init → invalid
        ];
        let e = Expectancy::from_rows(rows);
        assert_eq!(e.samples, 1);
    }
}

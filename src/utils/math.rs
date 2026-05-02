//! Numerical indicators used by strategies.

use crate::core::Candle;

/// v1.6.1: Compute a noise-floor-aware stop distance in PRICE units.
/// Returns the larger of:
///   - `atr_mult × atr` (the strategy's preferred ATR-based stop)
///   - `4 × spread_bps × entry / 10_000` (so bid/ask noise can't take us out)
///   - `min_stop_bps × entry / 10_000` (absolute floor: 25bps default)
///
/// Applied in all three strategies AND any code that constructs a stop.
/// The floor was added in v1.6.1 after a 16h log analysis showed sub-3-min
/// stop-outs on HYPE were caused by ATR (0.06%) being smaller than the
/// typical bid/ask noise band — random tick walks were taking us out
/// before the trade could develop.
pub fn stop_distance_floored(
    atr: f64,
    atr_mult: f64,
    entry: f64,
    spread_bps: Option<f64>,
    min_stop_bps: f64,
) -> f64 {
    let atr_dist = (atr * atr_mult).max(0.0);
    let spread_floor = spread_bps.unwrap_or(2.0).max(0.0) * 4.0 * entry / 10_000.0;
    let abs_floor = min_stop_bps.max(0.0) * entry / 10_000.0;
    atr_dist.max(spread_floor).max(abs_floor)
}

pub fn argmin(xs: &[f64]) -> usize {
    let mut best = 0; let mut bv = f64::INFINITY;
    for (i, &v) in xs.iter().enumerate() { if v < bv { bv = v; best = i; } }
    best
}
pub fn argmax(xs: &[f64]) -> usize {
    let mut best = 0; let mut bv = f64::NEG_INFINITY;
    for (i, &v) in xs.iter().enumerate() { if v > bv { bv = v; best = i; } }
    best
}

/// Exponential moving average. Returns series of same length; first n-1 values are seed SMA.
pub fn ema(xs: &[f64], period: usize) -> Vec<f64> {
    if xs.is_empty() { return vec![]; }
    let period = period.max(1);
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut out = Vec::with_capacity(xs.len());
    let seed_len = period.min(xs.len());
    let seed: f64 = xs[..seed_len].iter().sum::<f64>() / seed_len as f64;
    for (i, &x) in xs.iter().enumerate() {
        let v = if i == 0 { seed } else { alpha * x + (1.0 - alpha) * out[i-1] };
        out.push(v);
    }
    out
}

/// Classic Wilder RSI.
pub fn rsi_series(xs: &[f64], period: usize) -> Vec<f64> {
    let n = xs.len();
    let mut out = vec![50.0; n];
    if n < period + 1 { return out; }
    let mut gain = 0.0; let mut loss = 0.0;
    for i in 1..=period {
        let d = xs[i] - xs[i-1];
        if d >= 0.0 { gain += d; } else { loss += -d; }
    }
    gain /= period as f64;
    loss /= period as f64;
    let rs = if loss == 0.0 { 100.0 } else { gain / loss };
    out[period] = 100.0 - 100.0 / (1.0 + rs);
    for i in (period+1)..n {
        let d = xs[i] - xs[i-1];
        let g = d.max(0.0);
        let l = (-d).max(0.0);
        gain = (gain * (period as f64 - 1.0) + g) / period as f64;
        loss = (loss * (period as f64 - 1.0) + l) / period as f64;
        let rs = if loss == 0.0 { 100.0 } else { gain / loss };
        out[i] = 100.0 - 100.0 / (1.0 + rs);
    }
    out
}

/// ATR over the last `period` candles (single scalar).
pub fn atr(candles: &[Candle], period: usize) -> f64 {
    let series = atr_series(candles, period);
    *series.last().unwrap_or(&0.0)
}

/// ATR series (Wilder smoothing).
pub fn atr_series(candles: &[Candle], period: usize) -> Vec<f64> {
    let n = candles.len();
    let mut out = vec![0.0; n];
    if n < 2 { return out; }
    let tr = |i: usize| -> f64 {
        let c = &candles[i];
        let p = &candles[i-1];
        (c.high - c.low).max((c.high - p.close).abs()).max((c.low - p.close).abs())
    };
    let period = period.max(1);
    if n <= period { return out; }
    let first: f64 = (1..=period).map(tr).sum::<f64>() / period as f64;
    out[period] = first;
    for i in (period+1)..n {
        let t = tr(i);
        out[i] = (out[i-1] * (period as f64 - 1.0) + t) / period as f64;
    }
    out
}

/// Bollinger Bands: (middle, upper, lower). Uses population stdev.
pub fn bollinger(xs: &[f64], period: usize, std_mult: f64) -> (f64, f64, f64) {
    let n = xs.len();
    if n < period { return (xs.last().copied().unwrap_or(0.0), 0.0, 0.0); }
    let window = &xs[n-period..];
    let mean: f64 = window.iter().sum::<f64>() / period as f64;
    let var: f64 = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / period as f64;
    let sd = var.sqrt();
    (mean, mean + std_mult * sd, mean - std_mult * sd)
}

#[cfg(test)]
mod tests {
    use super::stop_distance_floored;

    /// On a HYPE-like low-ATR token: ATR = 0.025 (≈ 0.06% of $41).
    /// Pure ATR × 1.8 = $0.045 (≈11bps). With a 25bps floor on $41 = $0.10.
    /// Floor must win.
    #[test]
    fn stop_floor_protects_low_atr_market() {
        let dist = stop_distance_floored(0.025, 1.8, 41.0, Some(2.0), 25.0);
        let bps = dist / 41.0 * 10_000.0;
        assert!(bps >= 25.0, "expected ≥25bps floor, got {:.1}bps", bps);
        assert!((dist - 0.1025).abs() < 1e-6,
            "floor should give 25bps × $41 = $0.1025, got {}", dist);
    }

    /// On a BTC-like high-ATR market: ATR × 1.8 way above the floor.
    /// ATR-based stop wins.
    #[test]
    fn stop_floor_lets_high_atr_through() {
        // ATR = 200 on $80,000 BTC → 25bps. 1.8 × 200 = 360 (45bps).
        let dist = stop_distance_floored(200.0, 1.8, 80_000.0, Some(1.0), 25.0);
        assert!((dist - 360.0).abs() < 1e-6,
            "expected ATR×mult = 360, got {}", dist);
    }

    /// Wide spread on otherwise-quiet market: spread × 4 floor wins.
    #[test]
    fn stop_floor_handles_wide_spread() {
        // ATR = 0.001 (negligible), spread = 50bps wide.
        let dist = stop_distance_floored(0.001, 1.8, 100.0, Some(50.0), 25.0);
        // 4 × 50bps × 100 = 200bps × 100 / 10000 = $2.00
        assert!((dist - 2.0).abs() < 1e-6,
            "expected 4×spread = $2.00, got {}", dist);
    }

    /// Disabling the floor (min_stop_bps = 0) reverts to ATR-or-spread.
    #[test]
    fn stop_floor_disabled() {
        let dist = stop_distance_floored(0.025, 1.8, 41.0, Some(1.0), 0.0);
        // ATR×1.8 = 0.045; spread×4 = 4bps × 41 / 10000 × 100 = 0.0164. ATR wins.
        assert!((dist - 0.045).abs() < 1e-6, "got {}", dist);
    }
}

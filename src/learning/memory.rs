//! Memory Bot — feature vectors + k-NN similarity search over `trade_context`.
//!
//! When a new signal fires, we extract its feature vector, pull recent
//! `trade_context` rows that match the (symbol, strategy, side) tuple, and
//! rank them by L2 distance in normalized feature space. The top-k
//! "similar past trades" inform a SimilarityVerdict: their realized P&L
//! and win rate become inputs to the AdaptiveContext.
//!
//! Only (symbol, strategy, side) tuples filter the candidate pool — we
//! never compare an ETH LONG to a BTC SHORT. Within a tuple we still
//! differentiate by ATR%, regime, hour, day-of-week, and edge_bps.

use anyhow::Result;
use rusqlite::params;

use crate::storage::Storage;

pub const K: usize = 20;

/// Feature vector at signal time. All values are normalized to a comparable
/// scale before distance computation (ATR% is in [0, 5+], hour is [0,23], etc.).
#[derive(Debug, Clone)]
pub struct FeatureVec {
    pub atr_pct: f64,
    pub spread_bps: f64,
    pub edge_bps: f64,
    pub regime_5m_id: u8, // see regime_to_id
    pub hour_utc: u8,
    pub day_of_week: u8,
}

/// Stable mapping from regime name → small int. Keeps the feature space
/// compact and makes similarity distance well-defined.
pub fn regime_to_id(regime: &str) -> u8 {
    match regime {
        "Trending" => 1,
        "VolSpike" => 2,
        "Choppy" => 3,
        "Panic" => 4,
        "LowLiquidity" => 5,
        _ => 0, // NoTrade / unknown
    }
}

/// L2 distance in normalized feature space. Lower = more similar.
fn distance(a: &FeatureVec, b: &FeatureVec) -> f64 {
    let datr = (a.atr_pct - b.atr_pct) / 1.0; // ATR% scale ~ [0,5]
    let dspr = (a.spread_bps - b.spread_bps) / 10.0; // spread scale
    let dedg = (a.edge_bps - b.edge_bps) / 20.0; // edge scale
    let drgm = if a.regime_5m_id == b.regime_5m_id {
        0.0
    } else {
        1.0
    };
    // Hour distance is circular (23 vs 0 should be close).
    let dh_raw = (a.hour_utc as f64 - b.hour_utc as f64).abs();
    let dh = dh_raw.min(24.0 - dh_raw) / 12.0;
    let ddow = if a.day_of_week == b.day_of_week {
        0.0
    } else {
        1.0
    };
    (datr * datr + dspr * dspr + dedg * dedg + drgm + dh * dh + ddow).sqrt()
}

#[derive(Debug, Clone)]
pub struct SimilarityVerdict {
    pub matches: usize,
    pub avg_realized_r: f64,
    pub net_pnl: f64,
    pub win_rate: f64,
}

impl Default for SimilarityVerdict {
    fn default() -> Self {
        Self {
            matches: 0,
            avg_realized_r: 0.0,
            net_pnl: 0.0,
            win_rate: 0.0,
        }
    }
}

impl SimilarityVerdict {
    /// Should the bot REJECT this signal because past similar trades were
    /// significant losers? Conservative threshold parameterised so the user
    /// can tune via `[learning]` in `strategies.toml` without recompiling.
    pub fn is_blocking_with(&self, min_matches: usize, max_winrate: f64) -> bool {
        self.matches >= min_matches && self.win_rate < max_winrate && self.net_pnl < 0.0
    }

    /// Default thresholds (10 matches, <30% win-rate, net loss). Kept for
    /// callers that don't have config access (tests / introspection).
    pub fn is_blocking(&self) -> bool {
        self.is_blocking_with(10, 0.30)
    }

    pub fn summary(&self) -> String {
        if self.matches == 0 {
            "memory: no prior similar setups".into()
        } else {
            format!(
                "memory: {} matches, win_rate {:.0}%, avg_r {:+.2}, net ${:+.2}",
                self.matches,
                self.win_rate * 100.0,
                self.avg_realized_r,
                self.net_pnl,
            )
        }
    }
}

/// k-NN over `trade_context` filtered to matching (symbol, strategy, side).
/// Pulls up to 500 recent candidates from SQL then ranks in-memory by L2
/// distance — well under 1ms even at full row counts on commodity hardware.
pub fn similar_trades(
    storage: &Storage,
    symbol: &str,
    strategy: &str,
    side: &str,
    feat: &FeatureVec,
) -> Result<SimilarityVerdict> {
    type Row = (f64, Option<f64>, f64, String, i64, i64, f64, f64);
    let rows: Vec<Row> = storage.with_conn(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT atr_pct, spread_bps, edge_bps_predicted, regime_5m,
                    hour_utc, day_of_week, realized_r, realized_pnl
             FROM trade_context
             WHERE symbol = ?1 AND strategy = ?2 AND side = ?3
             ORDER BY id DESC LIMIT 500",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![symbol, strategy, side], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .map(|m| m.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    });
    if rows.is_empty() {
        return Ok(SimilarityVerdict::default());
    }

    // Score every candidate
    let mut scored: Vec<(f64, f64, f64)> = rows
        .iter()
        .map(|row| {
            let cf = FeatureVec {
                atr_pct: row.0,
                spread_bps: row.1.unwrap_or(2.0),
                edge_bps: row.2,
                regime_5m_id: regime_to_id(&row.3),
                hour_utc: row.4 as u8,
                day_of_week: row.5 as u8,
            };
            (distance(feat, &cf), row.6, row.7)
        })
        .collect();
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let top: Vec<_> = scored.into_iter().take(K).collect();
    let n = top.len();
    if n == 0 {
        return Ok(SimilarityVerdict::default());
    }

    let avg_r = top.iter().map(|(_, r, _)| r).sum::<f64>() / n as f64;
    let net = top.iter().map(|(_, _, p)| p).sum();
    let wins = top.iter().filter(|(_, _, p)| *p > 0.0).count() as f64;
    Ok(SimilarityVerdict {
        matches: n,
        avg_realized_r: avg_r,
        net_pnl: net,
        win_rate: wins / n as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fv(atr: f64, edge: f64, regime: u8, hour: u8) -> FeatureVec {
        FeatureVec {
            atr_pct: atr,
            spread_bps: 2.0,
            edge_bps: edge,
            regime_5m_id: regime,
            hour_utc: hour,
            day_of_week: 1,
        }
    }

    #[test]
    fn identical_vectors_have_zero_distance() {
        let a = fv(0.5, 15.0, 1, 12);
        assert_eq!(distance(&a, &a), 0.0);
    }

    #[test]
    fn different_regime_increases_distance() {
        let a = fv(0.5, 15.0, 1, 12);
        let b = fv(0.5, 15.0, 3, 12);
        assert!(distance(&a, &b) > 0.5);
    }

    #[test]
    fn hour_distance_is_circular() {
        let a = fv(0.5, 15.0, 1, 1);
        let b = fv(0.5, 15.0, 1, 23);
        let c = fv(0.5, 15.0, 1, 13);
        // 1↔23 should be closer than 1↔13 (distance 2 vs 12).
        assert!(distance(&a, &b) < distance(&a, &c));
    }

    #[test]
    fn verdict_blocks_only_on_clear_history_of_loss() {
        let v = SimilarityVerdict {
            matches: 15,
            win_rate: 0.20,
            net_pnl: -3.0,
            avg_realized_r: -0.5,
        };
        assert!(v.is_blocking());

        let v2 = SimilarityVerdict {
            matches: 5,
            win_rate: 0.0,
            net_pnl: -3.0,
            avg_realized_r: -0.5,
        };
        assert!(!v2.is_blocking(), "small sample shouldn't block");

        let v3 = SimilarityVerdict {
            matches: 15,
            win_rate: 0.40,
            net_pnl: -1.0,
            avg_realized_r: -0.1,
        };
        assert!(
            !v3.is_blocking(),
            "marginal loss with 40% wr shouldn't block"
        );
    }
}

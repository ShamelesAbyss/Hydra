//! v1.5.0 integration smoke test: open an in-memory storage, write fake
//! `trade_context` rows, build an `AdaptiveContext`, and verify the
//! decision math is bounded and responds to the data we plant.

use hydra::config::LearningCfg;
use hydra::core::Regime;
use hydra::learning::memory::{regime_to_id, FeatureVec};
use hydra::learning::regime::RegimeView;
use hydra::learning::{AdaptiveContext, SignalDecisionKind};
use hydra::storage::Storage;

/// Insert N `trade_context` rows for a given (symbol, strategy, side) bucket
/// where every trade has the given realized R + PnL outcome. Used by the
/// tests to simulate "this bucket has been winning/losing".
fn seed(
    storage: &Storage,
    symbol: &str,
    strategy: &str,
    side: &str,
    regime_5m: &str,
    atr_pct: f64,
    edge_bps: f64,
    realized_r: f64,
    realized_pnl: f64,
    n: usize,
) {
    storage.with_conn(|conn| {
        for _ in 0..n {
            conn.execute(
                "INSERT INTO trade_context (
                    symbol, strategy, side,
                    regime_1m, regime_5m, regime_15m,
                    atr_pct, spread_bps, edge_bps_predicted,
                    hour_utc, day_of_week,
                    entry_price, exit_price, r_init,
                    realized_pnl, realized_r, duration_secs, exit_reason,
                    opened_at, closed_at
                ) VALUES (
                    ?1, ?2, ?3,
                    ?4, ?4, ?4,
                    ?5, 2.0, ?6,
                    12, 1,
                    100.0, 101.0, 1.0,
                    ?7, ?8, 60, 'tp',
                    ?9, ?9
                )",
                rusqlite::params![
                    symbol,
                    strategy,
                    side,
                    regime_5m,
                    atr_pct,
                    edge_bps,
                    realized_pnl,
                    realized_r,
                    chrono::Utc::now().to_rfc3339(),
                ],
            )
            .unwrap();
        }
    });
}

fn fresh_storage() -> Storage {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = Storage::open(tmp.path()).unwrap();
    storage.migrate().unwrap();
    // Keep tempfile alive for the test by leaking. Tests are short-lived.
    Box::leak(Box::new(tmp));
    storage
}

fn neutral_view() -> RegimeView {
    RegimeView {
        r_1m: Regime::Trending,
        r_5m: Regime::Trending,
        r_15m: Regime::Trending,
    }
}

fn neutral_feat() -> FeatureVec {
    FeatureVec {
        atr_pct: 0.5,
        spread_bps: 2.0,
        edge_bps: 15.0,
        regime_5m_id: regime_to_id("Trending"),
        hour_utc: 12,
        day_of_week: 1,
    }
}

#[test]
fn empty_storage_returns_neutral_decision() {
    let storage = fresh_storage();
    let cfg = LearningCfg::default();
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    assert_eq!(dec.kind, SignalDecisionKind::Allow);
    assert!(
        (dec.size_mult - 1.0).abs() < 1e-9,
        "neutral mult expected, got {}",
        dec.size_mult
    );
    assert!(
        dec.edge_threshold_override.is_none(),
        "no expectancy yet → no edge override"
    );
}

#[test]
fn winning_history_lifts_size_multiplier() {
    let storage = fresh_storage();
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, 0.5, 1.0, 50,
    );
    let cfg = LearningCfg::default();
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    assert_eq!(dec.kind, SignalDecisionKind::Allow);
    assert!(
        dec.size_mult > 1.0,
        "winning history should lift mult, got {}",
        dec.size_mult
    );
    assert!(
        dec.size_mult <= 1.5,
        "mult must clamp at 1.5, got {}",
        dec.size_mult
    );
}

#[test]
fn losing_history_drops_size_multiplier() {
    let storage = fresh_storage();
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, -0.5, -1.0, 50,
    );
    let cfg = LearningCfg::default();
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    // With 50× -0.5R losses (and net pnl very negative), the memory verdict
    // will hard-block before we even compute the multiplier.
    match &dec.kind {
        SignalDecisionKind::Block(_) => {} // expected
        SignalDecisionKind::Allow => {
            assert!(
                dec.size_mult < 1.0,
                "losing history should drop mult, got {}",
                dec.size_mult
            );
            assert!(
                dec.size_mult >= 0.5,
                "mult must clamp at 0.5, got {}",
                dec.size_mult
            );
        }
    }
}

#[test]
fn memory_block_disabled_via_config_lets_signal_through() {
    let storage = fresh_storage();
    // Plant clear losing history that would normally trigger memory-block.
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, -0.8, -3.0, 30,
    );
    let mut cfg = LearningCfg::default();
    cfg.memory_block_enabled = false;
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    assert_eq!(
        dec.kind,
        SignalDecisionKind::Allow,
        "with memory_block_enabled=false, signal must always proceed"
    );
    assert!(dec.size_mult >= 0.5 && dec.size_mult <= 1.5);
}

#[test]
fn learning_disabled_returns_pure_identity() {
    let storage = fresh_storage();
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, 0.8, 2.0, 50,
    );
    let mut cfg = LearningCfg::default();
    cfg.enabled = false;
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    assert_eq!(dec.kind, SignalDecisionKind::Allow);
    assert_eq!(dec.size_mult, 1.0);
    assert!(dec.edge_threshold_override.is_none());
}

#[test]
fn winning_strategy_loosens_edge_threshold() {
    let storage = fresh_storage();
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, 0.6, 1.5, 50,
    );
    let cfg = LearningCfg::default();
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let dec = ctx
        .for_signal("BTC", "momentum", "LONG", neutral_view(), &neutral_feat())
        .unwrap();
    let delta = dec
        .edge_threshold_override
        .expect("with 50 winning samples, expect a delta");
    assert!(
        delta < 0.0,
        "winning streak should LOOSEN edge gate (negative delta), got {}",
        delta
    );
    assert!(delta > -10.0, "delta must be bounded, got {}", delta);
}

#[test]
fn snapshot_returns_per_symbol_multipliers() {
    let storage = fresh_storage();
    seed(
        &storage, "BTC", "momentum", "LONG", "Trending", 0.5, 15.0, 0.5, 1.0, 50,
    );
    seed(
        &storage, "ETH", "momentum", "LONG", "Trending", 0.5, 15.0, -0.3, -0.5, 50,
    );
    let cfg = LearningCfg::default();
    let ctx = AdaptiveContext::new(&storage, &cfg);
    let snap = ctx
        .snapshot_for_symbols(&["BTC".into(), "ETH".into()], &["momentum"])
        .unwrap();
    assert_eq!(snap.sym_mults.len(), 2);
    let btc = snap.sym_mults.iter().find(|x| x.0 == "BTC").unwrap();
    let eth = snap.sym_mults.iter().find(|x| x.0 == "ETH").unwrap();
    assert!(btc.1 > 1.0, "BTC mult should be >1.0, got {}", btc.1);
    assert!(eth.1 < 1.0, "ETH mult should be <1.0, got {}", eth.1);
}

#[test]
fn record_trade_context_stores_entry_time_hour_not_close_time() {
    // Regression test for the v1.5.0 audit bug fix: hour_utc / day_of_week
    // in `trade_context` MUST reflect the OPEN time, not the close time.
    // Memory Bot keys on (hour, dow) at signal time — using close-time
    // would silently break similarity matching for any trade lasting > 1h.
    let storage = fresh_storage();
    // Synthetic open-time ≠ now: 14:30 UTC on Wednesday (day_of_week=2 with
    // 0=Monday convention).
    let opened_at = chrono::DateTime::parse_from_rfc3339("2026-04-22T14:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let view = RegimeView {
        r_1m: Regime::Trending,
        r_5m: Regime::Trending,
        r_15m: Regime::Trending,
    };
    hydra::learning::context::record_trade_context(
        &storage,
        "BTC",
        "momentum",
        "LONG",
        view,
        0.5,
        Some(2.0),
        15.0,
        100.0,
        102.0,
        1.0,
        2.0,
        60,
        "tp",
        opened_at,
    )
    .unwrap();
    // Read back and verify hour_utc == 14, day_of_week == 2 (Wed).
    let (hour, dow): (i64, i64) = storage.with_conn(|conn| {
        conn.query_row(
            "SELECT hour_utc, day_of_week FROM trade_context WHERE symbol='BTC'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    });
    assert_eq!(
        hour, 14,
        "stored hour must be entry-time UTC hour, not close-time"
    );
    assert_eq!(dow, 2, "stored day_of_week must be 0-indexed Mon=0 (Wed=2)");
}

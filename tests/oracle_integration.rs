use chrono::{TimeZone, Utc};
use hydra::{
    config::AppConfig,
    core::{Candle, MarketSnapshot, Regime, Side, Signal},
    oracle::{replay_with_strategies, ReplayOptions},
    strategies::Strategy,
};
use std::sync::{Arc, Mutex};

struct Once {
    seen: Arc<Mutex<Vec<i64>>>,
}
impl Strategy for Once {
    fn name(&self) -> &'static str {
        "offline_once"
    }
    fn evaluate(&self, s: &MarketSnapshot, _: Regime, _: &AppConfig) -> Option<Signal> {
        let mut seen = self.seen.lock().unwrap();
        seen.push(s.last_update.timestamp_millis());
        assert!(s.candles.iter().all(|c| c.ts <= s.last_update));
        if seen.len() != 1 {
            return None;
        }
        Some(Signal {
            strategy: self.name().into(),
            venue: s.venue.clone(),
            symbol: s.symbol.clone(),
            side: Side::Long,
            confidence: 1.0,
            expected_edge_bps: 100.0,
            stop_price: 90.0,
            take_profit: 120.0,
            reason: "offline test".into(),
            created_at: Utc::now(),
        })
    }
}
fn cfg() -> AppConfig {
    let mut c = AppConfig::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap();
    c.strategies.global.regime_required = false;
    c
}
fn bars() -> Vec<Candle> {
    (0..5)
        .map(|i| Candle {
            ts: Utc
                .timestamp_millis_opt(1_700_000_000_000 + i * 60_000)
                .unwrap(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
        })
        .collect()
}
fn options() -> ReplayOptions {
    ReplayOptions {
        warmup_bars: 1,
        fee_bps: 10.0,
        slippage_bps: 0.0,
        ..Default::default()
    }
}
fn strategies() -> Vec<Box<dyn Strategy>> {
    vec![Box::new(Once {
        seen: Arc::new(Mutex::new(vec![])),
    })]
}

#[test]
fn next_bar_fill_and_forced_exit_charge_both_fees_reproducibly() {
    let b = bars();
    let o = options();
    let one = replay_with_strategies(&cfg(), &b, &o, &strategies()).unwrap();
    let two = replay_with_strategies(&cfg(), &b, &o, &strategies()).unwrap();
    assert_eq!(
        serde_json::to_string(&one).unwrap(),
        serde_json::to_string(&two).unwrap()
    );
    let t = &one.trades[0];
    assert_eq!(t.signal_time_ms, b[1].ts.timestamp_millis());
    assert_eq!(t.entry_time_ms, b[2].ts.timestamp_millis());
    assert_eq!(t.exit_reason, "end_of_data");
    assert!(t.fees > 0.0);
    assert!((one.final_equity - (o.initial_equity - t.fees)).abs() < 1e-9);
}
#[test]
fn stop_precedes_target_when_both_are_touched() {
    let mut b = bars();
    b[2].high = 125.0;
    b[2].low = 85.0;
    let report = replay_with_strategies(&cfg(), &b, &options(), &strategies()).unwrap();
    assert_eq!(report.trades[0].exit_reason, "stop");
    assert_eq!(report.trades[0].exit_price, 90.0);
}
#[test]
fn adverse_gap_exits_at_the_open_and_slippage_reduces_result() {
    let mut b = bars();
    b[3].open = 80.0;
    b[3].low = 79.0;
    b[3].close = 80.0;
    b[3].high = 81.0;
    let mut o = options();
    o.slippage_bps = 10.0;
    let report = replay_with_strategies(&cfg(), &b, &o, &strategies()).unwrap();
    assert_eq!(report.trades[0].exit_reason, "stop");
    assert!(report.trades[0].exit_price < 80.0);
}
#[test]
fn invalid_inputs_and_resource_limits_fail_before_evaluation() {
    let mut b = bars();
    b[2].close = f64::NAN;
    assert!(replay_with_strategies(&cfg(), &b, &options(), &strategies()).is_err());
    let mut b = bars();
    b[2].ts = b[1].ts;
    assert!(replay_with_strategies(&cfg(), &b, &options(), &strategies()).is_err());
    let mut o = options();
    o.max_bars = 2;
    assert!(replay_with_strategies(&cfg(), &bars(), &o, &strategies()).is_err());
}

#[test]
fn entry_gap_through_the_planned_stop_rejects_the_entry_without_fees() {
    let mut b = bars();
    b[2].open = 80.0;
    b[2].close = 80.0;
    b[2].high = 81.0;
    b[2].low = 79.0;
    let report = replay_with_strategies(&cfg(), &b, &options(), &strategies()).unwrap();
    assert!(report.trades.is_empty());
    assert_eq!(report.rejected_entries, 1);
    assert_eq!(report.total_fees, 0.0);
    assert_eq!(report.final_equity, report.initial_equity);
}

#[test]
fn unleveraged_entry_budget_includes_the_entry_fee() {
    let mut o = options();
    o.risk_fraction = 0.1;
    o.max_exposure_fraction = 1.0;
    let report = replay_with_strategies(&cfg(), &bars(), &o, &strategies()).unwrap();
    let trade = &report.trades[0];
    let notional = trade.quantity * trade.entry_price;
    let entry_fee = notional * o.fee_bps / 10_000.0;
    assert!(notional + entry_fee <= o.initial_equity + 1e-9);
}

struct ShortOnce(Once);
impl Strategy for ShortOnce {
    fn name(&self) -> &'static str {
        "offline_short_once"
    }
    fn evaluate(
        &self,
        snapshot: &MarketSnapshot,
        regime: Regime,
        cfg: &AppConfig,
    ) -> Option<Signal> {
        let mut signal = self.0.evaluate(snapshot, regime, cfg)?;
        signal.side = Side::Short;
        signal.stop_price = 110.0;
        signal.take_profit = 80.0;
        Some(signal)
    }
}

#[test]
fn short_position_uses_the_same_conservative_stop_priority() {
    let mut b = bars();
    b[2].high = 115.0;
    b[2].low = 75.0;
    let source: Vec<Box<dyn Strategy>> = vec![Box::new(ShortOnce(Once {
        seen: Arc::new(Mutex::new(vec![])),
    }))];
    let report = replay_with_strategies(&cfg(), &b, &options(), &source).unwrap();
    assert_eq!(report.trades[0].exit_reason, "stop");
    assert_eq!(report.trades[0].exit_price, 110.0);
    assert!(report.trades[0].gross_pnl < 0.0);
}

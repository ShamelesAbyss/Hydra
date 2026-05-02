//! Smoke test: ensure the Cockpit TUI renderer can draw a full frame into
//! a test backend without panicking, both with empty state and with a
//! realistic populated state. Exercises every panel.

use chrono::Utc;
use hydra::tui::render;
use hydra::tui::state::{
    DashState, ErrorRow, FeedLine, FillRow, MarketRow, PositionRow, RiskView, SignalRow,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn make_populated_state() -> DashState {
    let mut d = DashState {
        version: "1.3.0".into(),
        session_id: 47,
        started_at: Some(Utc::now() - chrono::Duration::minutes(34)),
        live_trading: true,
        venues: vec!["hyperliquid".into()],
        symbols: vec!["ETH".into(), "BTC".into()],
        tick: 1_234,
        last_heartbeat: Some(Utc::now()),
        equity_usd: 94.12,
        session_realized_usd: 2.87,
        session_trades: 14,
        risk: RiskView {
            default_risk_pct: 0.5,
            max_daily_loss_pct: 3.0,
            max_weekly_loss_pct: 8.0,
            max_consecutive_losses: 4,
            max_open_exposure_pct: 40.0,
            max_notional_per_market_usd: 200.0,
            risk_mult: 1.18,
            consec_losses: 1,
            open_exposure_usd: 42.71,
            daily_realized_usd: 2.87,
            weekly_realized_usd: 5.14,
        },
        ..Default::default()
    };
    for _ in 0..30 { d.push_equity(92.0); }
    for _ in 0..30 { d.push_equity(94.12); }
    d.upsert_market(MarketRow {
        symbol: "ETH".into(), mid: 2034.51, spread_bps: Some(1.2),
        atr_pct: 0.24, regime: "Trending".into(),
        regime_view: Some("Tr/Tr/Ch".into()),
        adaptive_mult: Some((1.05, 12)),
        price_history: std::collections::VecDeque::new(),
        last_update: Utc::now(),
    });
    d.upsert_market(MarketRow {
        symbol: "BTC".into(), mid: 63_102.20, spread_bps: Some(0.8),
        atr_pct: 0.18, regime: "Choppy".into(),
        regime_view: Some("Ch/Ch/Tr".into()),
        adaptive_mult: None,
        price_history: std::collections::VecDeque::new(),
        last_update: Utc::now(),
    });
    d.set_positions(vec![PositionRow {
        venue: "hyperliquid".into(), symbol: "ETH".into(), side: "LONG".into(),
        size: 0.021, entry_price: 2031.0, mark_price: 2034.51,
        unrealized_pnl_usd: 0.074, stop_price: Some(2020.0), take_profit: Some(2055.0),
        opened_at: Utc::now() - chrono::Duration::minutes(12),
        strategy: Some("momentum".into()),
    }]);
    d.push_signal(SignalRow {
        ts: Utc::now(), strategy: "momentum".into(), symbol: "ETH".into(),
        side: "LONG".into(), edge_bps: 12.0, outcome: "filled".into(),
    });
    d.push_signal(SignalRow {
        ts: Utc::now(), strategy: "divergence".into(), symbol: "BTC".into(),
        side: "SHORT".into(), edge_bps: 9.0, outcome: "blocked: spread too wide".into(),
    });
    d.push_fill(FillRow {
        ts: Utc::now(), symbol: "ETH".into(), side: "LONG".into(), size: 0.021,
        price: 2034.35, kind: "open".into(), realized_usd: None,
    });
    d.push_fill(FillRow {
        ts: Utc::now(), symbol: "ETH".into(), side: "LONG".into(), size: 0.021,
        price: 2055.1, kind: "close_tp".into(), realized_usd: Some(0.47),
    });
    d.push_error(ErrorRow {
        ts: Utc::now(), kind: "snapshot".into(),
        message: "timeout on l2Book, retry 1/4".into(),
    });
    d.push_feed(FeedLine {
        ts: Utc::now(), level: "INFO".into(), target: "engine".into(),
        message: "signal momentum ETH LONG edge=12bps regime=trending".into(),
    });
    d.push_feed(FeedLine {
        ts: Utc::now(), level: "WARN".into(), target: "adapter".into(),
        message: "hyperliquid info retry (200ms backoff)".into(),
    });
    d
}

#[test]
fn renders_empty_state_without_panic() {
    let backend = TestBackend::new(160, 50);
    let mut t = Terminal::new(backend).unwrap();
    let state = DashState { version: "1.3.0".into(), ..Default::default() };
    t.draw(|f| render::render(f, &state)).expect("render empty");
}

#[test]
fn renders_populated_state_with_all_panels() {
    let backend = TestBackend::new(160, 50);
    let mut t = Terminal::new(backend).unwrap();
    let state = make_populated_state();
    t.draw(|f| render::render(f, &state)).expect("render populated");
}

#[test]
fn renders_at_minimum_viable_terminal_size() {
    // 100x32 is on the lower end of "useful" terminal sizes.
    let backend = TestBackend::new(100, 32);
    let mut t = Terminal::new(backend).unwrap();
    let state = make_populated_state();
    t.draw(|f| render::render(f, &state)).expect("render small");
}

#[test]
fn ring_buffers_cap_correctly() {
    let mut d = DashState::default();
    for i in 0..500 {
        d.push_feed(FeedLine {
            ts: Utc::now(), level: "INFO".into(), target: "t".into(),
            message: format!("m{i}"),
        });
    }
    assert!(d.feed.len() <= hydra::tui::state::FEED_CAP);
    assert_eq!(d.feed.back().unwrap().message, "m499");
}

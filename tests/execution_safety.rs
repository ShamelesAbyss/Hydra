//! Offline fault injection: these tests never construct a real exchange adapter.
use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::Utc;
use hydra::{
    adapters::{VenueAdapter, VenuePosition},
    config::AppConfig,
    core::{MarketSnapshot, OpenPosition, OrderIntent, OrderResult, Side, VenueKind},
    execution::{authorized_live, ExecutionEngine},
    portfolio::PortfolioManager,
    state::StateManager,
    storage::Storage,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
enum Reply {
    Reject,
    Timeout,
    TimeoutEth,
    Fill(f64),
    MissingPrice,
    WrongId,
}
struct Fake {
    reply: Reply,
    remote: Option<f64>,
    calls: Arc<AtomicUsize>,
    db: Storage,
}
#[async_trait]
impl VenueAdapter for Fake {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }
    async fn ping(&self) -> Result<()> {
        Ok(())
    }
    async fn snapshot(&self, _: &str, _: usize) -> Result<MarketSnapshot> {
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            mid: 100.0,
            bid: Some(99.0),
            ask: Some(101.0),
            spread_bps: Some(1.0),
            l2_depth_usd: None,
            funding_bps_1h: None,
            last_update: Utc::now(),
            observed_exchange_ts_ms: None,
            candles: vec![],
            microstructure: None,
        })
    }
    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        let size = self
            .remote
            .ok_or_else(|| anyhow::anyhow!("account offline"))?;
        Ok(if size == 0.0 {
            vec![]
        } else {
            vec![VenuePosition {
                symbol: "ETH".into(),
                size,
                entry_price: 100.0,
                unrealized_pnl: 0.0,
                leverage: 1.0,
            }]
        })
    }
    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        // Submission must be observable as durable before the network call begins.
        let count: i64 =
            self.db.with_conn(|c| {
                c.query_row(
            "SELECT COUNT(*) FROM execution_intents WHERE intent_id=?1 AND state='submitted'",
            [&intent.id], |r| r.get(0))
            })?;
        assert_eq!(count, 1);
        self.calls.fetch_add(1, Ordering::SeqCst);
        if matches!(self.reply, Reply::Timeout)
            || (matches!(self.reply, Reply::TimeoutEth) && intent.symbol == "ETH")
        {
            bail!("injected timeout after send")
        }
        Ok(OrderResult {
            intent_id: if matches!(self.reply, Reply::WrongId) {
                "wrong".into()
            } else {
                intent.id.clone()
            },
            accepted: !matches!(self.reply, Reply::Reject),
            venue_order_id: Some("42".into()),
            filled_size: match self.reply {
                Reply::Fill(q) => q,
                Reply::TimeoutEth => intent.size,
                Reply::MissingPrice | Reply::WrongId => 1.0,
                _ => 0.0,
            },
            avg_fill_price: if matches!(self.reply, Reply::MissingPrice) {
                None
            } else {
                Some(110.0)
            },
            message: "offline fixture".into(),
        })
    }
    async fn close_position(&self, _: &str, _: f64, _: bool) -> Result<OrderResult> {
        panic!("close must use the journaled submit path")
    }
    async fn account_equity_usd(&self) -> Result<f64> {
        Ok(1000.0)
    }
    async fn account_exposure_evidence(
        &self,
    ) -> Result<hydra::adapters::account_evidence::AccountExposureEvidence> {
        use hydra::adapters::account_evidence::{AccountExposureEvidence, PositionEvidence};
        use rust_decimal::Decimal;
        let quantity = self
            .remote
            .ok_or_else(|| anyhow::anyhow!("offline account unavailable"))?;
        let size = Decimal::from_str_exact(&quantity.to_string())?;
        let now = Utc::now().timestamp_millis();
        Ok(AccountExposureEvidence {
            account: "0x1111111111111111111111111111111111111111".into(),
            venue: "hyperliquid".into(),
            environment: "mainnet".into(),
            perp_dex: String::new(),
            account_mode: "disabled".into(),
            acquired_started_ms: now,
            acquired_finished_ms: now,
            exchange_time_ms: now,
            snapshot: hydra::aegis::AccountSnapshot {
                revision: now,
                observed_at_ms: now,
                equity: rust_decimal_macros::dec!(1000),
                exposure_by_market: if size.is_zero() {
                    Default::default()
                } else {
                    std::collections::BTreeMap::from([(
                        "ETH".into(),
                        size.abs() * rust_decimal_macros::dec!(100),
                    )])
                },
            },
            positions: if size.is_zero() {
                vec![]
            } else {
                vec![PositionEvidence {
                    symbol: "ETH".into(),
                    signed_size: size,
                    entry_price: rust_decimal_macros::dec!(100),
                    notional: size.abs() * rust_decimal_macros::dec!(100),
                }]
            },
            open_orders: vec![],
        })
    }
    async fn risk_snapshot(&self) -> Result<hydra::aegis::AccountSnapshot> {
        Ok(hydra::aegis::AccountSnapshot {
            revision: Utc::now().timestamp_millis(),
            observed_at_ms: Utc::now().timestamp_millis(),
            equity: rust_decimal_macros::dec!(1000),
            exposure_by_market: Default::default(),
        })
    }
}

#[tokio::test]
async fn failed_exit_on_one_instrument_does_not_starve_other_protective_exits() {
    let db = db();
    position(&db);
    let mut second = db.list_open_positions().unwrap()[0].clone();
    second.id = 0;
    second.symbol = "BTC".into();
    db.insert_open_position(&second).unwrap();
    let venue = fake(&db, Reply::TimeoutEth, Some(2.0));
    let calls = venue.calls.clone();
    let venues: Vec<Box<dyn VenueAdapter>> = vec![Box::new(venue)];
    let mut execution = ExecutionEngine::new(db.clone(), true);
    let mut portfolio = PortfolioManager::new(db.clone());
    let snapshots: Vec<_> = ["ETH", "BTC"]
        .iter()
        .map(|symbol| MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: (*symbol).into(),
            mid: 80.0,
            bid: None,
            ask: None,
            spread_bps: None,
            l2_depth_usd: None,
            funding_bps_1h: None,
            last_update: Utc::now(),
            observed_exchange_ts_ms: None,
            candles: vec![],
            microstructure: None,
        })
        .collect();
    assert!(portfolio
        .manage_open_positions(&snapshots, &cfg(), &mut execution, &venues)
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let remaining = db.list_open_positions().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].symbol, "ETH");
}

fn db() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db
}
fn intent(reduce: bool) -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Long,
        size: 2.0,
        limit_price: None,
        reduce_only: reduce,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: Utc::now(),
    }
}
fn fake(db: &Storage, reply: Reply, remote: Option<f64>) -> Fake {
    Fake {
        db: db.clone(),
        reply,
        remote,
        calls: Arc::new(AtomicUsize::new(0)),
    }
}
fn position(db: &Storage) {
    db.insert_open_position(&OpenPosition {
        id: 0,
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: "LONG".into(),
        size: 2.0,
        entry_price: 100.0,
        stop_price: Some(90.0),
        take_profit: Some(110.0),
        opened_at: Utc::now(),
        strategy: Some("momentum".into()),
        trailing_high: Some(100.0),
    })
    .unwrap();
}
fn cfg() -> AppConfig {
    AppConfig::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap()
}

#[test]
fn live_flag_cannot_override_configuration() {
    assert!(!authorized_live(false, false).unwrap());
    assert!(authorized_live(false, true).is_err());
    assert!(authorized_live(true, false).unwrap());
    assert!(authorized_live(true, true).unwrap());
}

#[tokio::test]
async fn dry_run_never_calls_exchange_or_journals_execution() {
    let db = db();
    let venue = fake(&db, Reply::Timeout, None);
    let mut execution = ExecutionEngine::new(db.clone(), false);
    assert!(
        !execution
            .submit(&intent(false), &venue)
            .await
            .unwrap()
            .accepted
    );
    assert!(
        !execution
            .close(&venue, "ETH", 1.0, false)
            .await
            .unwrap()
            .accepted
    );
    assert_eq!(venue.calls.load(Ordering::SeqCst), 0);
    db.ensure_execution_resolved().unwrap();
}

#[tokio::test]
async fn timeout_blocks_retries_and_new_exposure_after_restart() {
    let db = db();
    let venue = fake(&db, Reply::Timeout, None);
    let mut execution = ExecutionEngine::new(db.clone(), true);
    let order = intent(false);
    assert!(execution.submit(&order, &venue).await.is_err());
    let mut restarted = ExecutionEngine::new(db.clone(), true);
    assert!(restarted.submit(&order, &venue).await.is_err());
    assert!(restarted.submit(&intent(false), &venue).await.is_err());
    assert!(restarted.close(&venue, "ETH", 1.0, false).await.is_err());
    assert_eq!(venue.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rejected_intent_cannot_be_submitted_twice() {
    let db = db();
    let venue = fake(&db, Reply::Reject, Some(2.0));
    let mut execution = ExecutionEngine::new(db.clone(), true);
    let order = intent(false);
    assert!(!execution.submit(&order, &venue).await.unwrap().accepted);
    assert!(execution.submit(&order, &venue).await.is_err());
    db.ensure_execution_resolved().unwrap();
    assert_eq!(venue.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn malformed_acknowledgments_remain_unresolved() {
    for reply in [
        Reply::MissingPrice,
        Reply::WrongId,
        Reply::Fill(f64::NAN),
        Reply::Fill(3.0),
    ] {
        let db = db();
        let venue = fake(&db, reply, Some(2.0));
        let mut execution = ExecutionEngine::new(db.clone(), true);
        assert!(execution.submit(&intent(false), &venue).await.is_err());
        assert!(db.ensure_execution_resolved().is_err());
    }
}

async fn flatten_case(reply: Reply, remote: Option<f64>, live: bool) -> (Storage, Result<()>) {
    let db = db();
    position(&db);
    let venue = fake(&db, reply, remote);
    let mut execution = ExecutionEngine::new(db.clone(), live);
    let mut portfolio = PortfolioManager::new(db.clone());
    let result = portfolio
        .flatten_all(&cfg(), &mut execution, &[Box::new(venue)])
        .await;
    (db, result)
}

#[tokio::test]
async fn rejected_failed_and_dry_run_closes_preserve_positions() {
    for (reply, live) in [
        (Reply::Reject, true),
        (Reply::Timeout, true),
        (Reply::Fill(2.0), false),
    ] {
        let (db, result) = flatten_case(reply, Some(0.0), live).await;
        assert!(result.is_err());
        assert_eq!(db.list_open_positions().unwrap()[0].size, 2.0);
        assert_eq!(
            db.sum_realized_since(Utc::now() - chrono::Duration::days(1))
                .unwrap(),
            0.0
        );
    }
}

#[tokio::test]
async fn partial_close_persists_only_confirmed_residual() {
    let (db, result) = flatten_case(Reply::Fill(0.75), Some(1.25), true).await;
    result.unwrap();
    assert_eq!(db.list_open_positions().unwrap()[0].size, 1.25);
    assert!(db.ensure_execution_resolved().is_err()); // net accounting outstanding
    let (gross, net): (f64, Option<f64>) = db
        .with_conn(|c| {
            c.query_row(
                "SELECT gross_pnl,net_pnl FROM confirmed_reductions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(gross, 7.5);
    assert_eq!(net, None);
    assert_eq!(db.get_compound_state().unwrap().realized_profit, 0.0);
}

#[tokio::test]
async fn full_close_needs_matching_exchange_position_and_never_invents_net_pnl() {
    let (db, result) = flatten_case(Reply::Fill(2.0), Some(0.0), true).await;
    result.unwrap();
    assert!(db.list_open_positions().unwrap().is_empty());
    let count: i64 = db
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM closed_trades", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn stale_or_unavailable_remote_state_never_marks_closed() {
    for remote in [None, Some(2.0), Some(-1.0), Some(f64::NAN)] {
        let (db, result) = flatten_case(Reply::Fill(2.0), remote, true).await;
        assert!(result.is_err());
        assert_eq!(db.list_open_positions().unwrap().len(), 1);
        assert!(db.ensure_execution_resolved().is_err());
    }
}

#[tokio::test]
async fn shutdown_reduction_updates_confirmed_quantity() {
    let db = db();
    position(&db);
    let venue = fake(&db, Reply::Fill(1.0), Some(1.0));
    let mut execution = ExecutionEngine::new(db.clone(), true);
    PortfolioManager::new(db.clone())
        .reduce_all(&cfg(), &mut execution, &[Box::new(venue)])
        .await
        .unwrap();
    assert_eq!(db.list_open_positions().unwrap()[0].size, 1.0);
}

#[tokio::test]
async fn missing_remote_position_is_not_a_zero_pnl_close() {
    let db = db();
    position(&db);
    let venue = fake(&db, Reply::Reject, Some(0.0));
    assert!(StateManager::new(db.clone())
        .reconcile_with_venues(&cfg(), &[Box::new(venue)])
        .await
        .is_err());
    assert_eq!(db.list_open_positions().unwrap().len(), 1);
}

#[tokio::test]
async fn emergency_disabled_preserves_local_state_without_network() {
    let db = db();
    position(&db);
    let mut cfg = cfg();
    cfg.live_trading = false;
    assert!(StateManager::new(db.clone())
        .emergency_flatten(&cfg)
        .await
        .is_err());
    assert_eq!(db.list_open_positions().unwrap().len(), 1);
}

#[test]
fn corrupt_position_row_and_database_failure_propagate() {
    let db = db();
    position(&db);
    db.with_conn(|c| c.execute("UPDATE positions SET opened_at='corrupt'", []))
        .unwrap();
    assert!(db.list_open_positions().is_err());
    assert!(PortfolioManager::new(db.clone()).exposure("ETH").is_err());
    db.with_conn(|c| c.execute("DROP TABLE positions", []))
        .unwrap();
    assert!(PortfolioManager::new(db).exposure("ETH").is_err());
}

#[test]
fn migrations_are_additive_idempotent_and_journal_survives_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    position(&db);
    let order = intent(false);
    db.reserve_intent(&order).unwrap();
    db.migrate().unwrap();
    drop(db);
    let reopened = Storage::open(path).unwrap();
    reopened.migrate().unwrap();
    assert_eq!(reopened.list_open_positions().unwrap().len(), 1);
    assert!(reopened.reserve_intent(&order).is_err());
    assert!(reopened.ensure_execution_resolved().is_err());
}

#[tokio::test]
async fn database_write_failure_rolls_back_reduction_and_blocks_a_second_close() {
    let db = db();
    position(&db);
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_position_update BEFORE UPDATE ON positions BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;")).unwrap();
    let venue = fake(&db, Reply::Fill(2.0), Some(0.0));
    let calls = venue.calls.clone();
    let venues: Vec<Box<dyn VenueAdapter>> = vec![Box::new(venue)];
    let mut execution = ExecutionEngine::new(db.clone(), true);
    let mut portfolio = PortfolioManager::new(db.clone());
    assert!(portfolio
        .flatten_all(&cfg(), &mut execution, &venues)
        .await
        .is_err());
    assert_eq!(db.list_open_positions().unwrap()[0].size, 2.0);
    let count: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM confirmed_reductions", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(count, 0);
    assert!(portfolio
        .flatten_all(&cfg(), &mut execution, &venues)
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn configured_independent_authority_reserves_exposure_before_network_write() {
    use rust_decimal_macros::dec;
    let db = db();
    db.bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let venue = fake(&db, Reply::Reject, Some(0.0));
    let authority = hydra::aegis::Aegis::new(
        db.clone(),
        "0x1111111111111111111111111111111111111111",
        hydra::aegis::RiskLimits {
            max_total_exposure_ratio: dec!(0.15),
            max_market_notional: dec!(1000),
            max_leverage: dec!(1),
            max_drawdown_ratio: dec!(0.2),
            max_snapshot_age_ms: 30_000,
        },
    )
    .unwrap();
    let mut execution = ExecutionEngine::new(db.clone(), true)
        .with_authority(authority.clone(), 30.0)
        .unwrap();
    let mut first = intent(false);
    first.size = 1.0;
    assert!(!execution.submit(&first, &venue).await.unwrap().accepted);
    assert_eq!(authority.status().unwrap().active_reservations, 1);
    let mut second = intent(false);
    second.size = 1.0;
    assert!(execution.submit(&second, &venue).await.is_err());
    assert_eq!(venue.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_unknown_remote_position_blocks_entries_before_submission() {
    use rust_decimal_macros::dec;
    let db = db();
    db.bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let venue = fake(&db, Reply::Fill(2.0), Some(2.0));
    let authority = hydra::aegis::Aegis::new(
        db.clone(),
        "0x1111111111111111111111111111111111111111",
        hydra::aegis::RiskLimits {
            max_total_exposure_ratio: dec!(1),
            max_market_notional: dec!(1000),
            max_leverage: dec!(1),
            max_drawdown_ratio: dec!(0.2),
            max_snapshot_age_ms: 30_000,
        },
    )
    .unwrap();
    let mut execution = ExecutionEngine::new(db, true)
        .with_authority(authority, 30.0)
        .unwrap();
    assert!(execution.submit(&intent(false), &venue).await.is_err());
    assert_eq!(venue.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn journal_write_failure_rolls_back_entry_reservation_before_any_send() {
    use rust_decimal_macros::dec;
    let db = db();
    db.bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let authority = hydra::aegis::Aegis::new(
        db.clone(),
        "0x1111111111111111111111111111111111111111",
        hydra::aegis::RiskLimits {
            max_total_exposure_ratio: dec!(1),
            max_market_notional: dec!(1000),
            max_leverage: dec!(1),
            max_drawdown_ratio: dec!(0.2),
            max_snapshot_age_ms: 30_000,
        },
    )
    .unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_entry_journal BEFORE INSERT ON execution_intents BEGIN SELECT RAISE(ABORT,'offline journal failure'); END;")).unwrap();
    let venue = fake(&db, Reply::Fill(1.0), Some(0.0));
    let mut engine = ExecutionEngine::new(db.clone(), true)
        .with_authority(authority.clone(), 30.0)
        .unwrap();
    assert!(engine.submit(&intent(false), &venue).await.is_err());
    assert_eq!(venue.calls.load(Ordering::SeqCst), 0);
    assert_eq!(authority.status().unwrap().active_reservations, 0);
    let journals: i64 = db
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM execution_intents", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(journals, 0);
}

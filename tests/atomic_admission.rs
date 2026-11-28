//! Atomic risk/journal admission; all account facts are offline fixtures.
use chrono::{TimeZone, Utc};
use hydra::{
    adapters::account_evidence::AccountExposureEvidence,
    aegis::{AccountSnapshot, Aegis, RiskDecision, RiskLimits},
    core::{OpenPosition, OrderIntent, OrderResult, Side},
    storage::Storage,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::BTreeMap;
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const BASE: i64 = 1_700_000_000_000;
fn database() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    db
}
fn authority(db: &Storage) -> Aegis {
    Aegis::new(
        db.clone(),
        ACCOUNT,
        RiskLimits {
            max_total_exposure_ratio: dec!(1),
            max_market_notional: dec!(1000),
            max_leverage: dec!(1),
            max_drawdown_ratio: dec!(0.2),
            max_snapshot_age_ms: 30_000,
        },
    )
    .unwrap()
}
fn evidence() -> AccountExposureEvidence {
    AccountExposureEvidence {
        account: ACCOUNT.into(),
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        account_mode: "disabled".into(),
        acquired_started_ms: BASE,
        acquired_finished_ms: BASE + 2,
        exchange_time_ms: BASE + 1,
        snapshot: AccountSnapshot {
            revision: BASE,
            observed_at_ms: BASE,
            equity: dec!(1000),
            exposure_by_market: BTreeMap::new(),
        },
        positions: vec![],
        open_orders: vec![],
        withdrawable_collateral: None,
        total_margin_used: None,
    }
}
fn intent() -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Long,
        size: 1.0,
        limit_price: Some(100.0),
        reduce_only: false,
        stop_price: Some(98.0),
        take_profit: Some(110.0),
        source_signal: None,
        created_at: Utc.timestamp_millis_opt(BASE).unwrap(),
    }
}
fn counts(db: &Storage) -> (i64, i64) {
    db.with_conn(|c|c.query_row("SELECT (SELECT COUNT(*) FROM aegis_reservations),(SELECT COUNT(*) FROM execution_intents)",[],|r|Ok((r.get(0)?,r.get(1)?)))).unwrap()
}
fn admit(aegis: &Aegis, order: &OrderIntent) -> anyhow::Result<RiskDecision> {
    aegis.reserve_journaled_entry(order, dec!(100), Decimal::ONE, &evidence(), BASE + 10)
}
#[test]
fn journal_failure_rolls_back_reservation_and_snapshot_before_retry() {
    let db = database();
    let aegis = authority(&db);
    let order = intent();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_journal BEFORE INSERT ON execution_intents BEGIN SELECT RAISE(ABORT,'offline journal failure'); END;")).unwrap();
    assert!(admit(&aegis, &order).is_err());
    assert_eq!(counts(&db), (0, 0));
    assert_eq!(aegis.status().unwrap().latest_snapshot_revision, None);
    assert_eq!(aegis.status().unwrap().high_water_equity, Decimal::ZERO);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_journal"))
        .unwrap();
    assert!(matches!(
        admit(&aegis, &order).unwrap(),
        RiskDecision::Approved(_)
    ));
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn reservation_failure_never_commits_a_submitted_intent() {
    let db = database();
    let aegis = authority(&db);
    let order = intent();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_reservation BEFORE INSERT ON aegis_reservations BEGIN SELECT RAISE(ABORT,'offline reservation failure'); END;")).unwrap();
    assert!(admit(&aegis, &order).is_err());
    assert_eq!(counts(&db), (0, 0));
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_reservation"))
        .unwrap();
    assert!(matches!(
        admit(&aegis, &order).unwrap(),
        RiskDecision::Approved(_)
    ));
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn blocked_drawdown_commits_breaker_but_no_order_or_reservation() {
    let db = database();
    let aegis = authority(&db);
    let original = evidence();
    aegis
        .apply_reconciled_snapshot(&original.snapshot, &[], "offline-initial", BASE + 2)
        .unwrap();
    let mut lower = original;
    lower.acquired_started_ms += 100;
    lower.acquired_finished_ms += 100;
    lower.exchange_time_ms += 100;
    lower.snapshot.revision += 100;
    lower.snapshot.observed_at_ms += 100;
    lower.snapshot.equity = dec!(700);
    let result = aegis
        .reserve_journaled_entry(&intent(), dec!(100), Decimal::ONE, &lower, BASE + 110)
        .unwrap();
    assert!(matches!(
        result,
        RiskDecision::Blocked {
            breaker_latched: true,
            ..
        }
    ));
    assert_eq!(counts(&db), (0, 0));
    assert!(aegis.status().unwrap().breaker_reason.is_some());
}
#[test]
fn invalid_scope_prices_and_understated_notional_leave_no_partial_facts() {
    let db = database();
    let aegis = authority(&db);
    for mutation in 0..5 {
        let mut order = intent();
        match mutation {
            0 => order.limit_price = None,
            1 => order.size = f64::NAN,
            2 => order.take_profit = Some(f64::NAN),
            3 => order.reduce_only = true,
            _ => order.venue = "unknown".into(),
        };
        assert!(admit(&aegis, &order).is_err());
        assert_eq!(counts(&db), (0, 0));
    }
    assert!(aegis
        .reserve_journaled_entry(&intent(), dec!(99), Decimal::ONE, &evidence(), BASE + 10)
        .is_err());
    let mut other = evidence();
    other.account = "0x2222222222222222222222222222222222222222".into();
    assert!(aegis
        .reserve_journaled_entry(&intent(), dec!(100), Decimal::ONE, &other, BASE + 10)
        .is_err());
    assert_eq!(counts(&db), (0, 0));
}
#[test]
fn unresolved_journal_blocks_new_admission_inside_the_transaction() {
    let db = database();
    let aegis = authority(&db);
    let mut old = intent();
    old.symbol = "BTC".into();
    db.reserve_intent(&old).unwrap();
    assert!(admit(&aegis, &intent()).is_err());
    assert_eq!(counts(&db), (0, 1));
    db.finish_intent(
        &old.id,
        "rejected",
        Some(&OrderResult {
            intent_id: old.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "offline".into(),
        }),
    )
    .unwrap();
    assert!(matches!(
        admit(&aegis, &intent()).unwrap(),
        RiskDecision::Approved(_)
    ));
    assert_eq!(counts(&db), (1, 2));
}
#[test]
fn unknown_financial_settlement_still_blocks_atomic_admission() {
    let db = database();
    let aegis = authority(&db);
    let position = OpenPosition {
        id: 0,
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: "LONG".into(),
        size: 1.0,
        entry_price: 100.0,
        stop_price: Some(98.0),
        take_profit: Some(110.0),
        opened_at: Utc::now(),
        strategy: None,
        trailing_high: Some(100.0),
    };
    let id = db.insert_open_position(&position).unwrap();
    let mut position = position;
    position.id = id;
    let mut close = intent();
    close.reduce_only = true;
    close.side = Side::Short;
    db.reserve_intent(&close).unwrap();
    let receipt = OrderResult {
        intent_id: close.id.clone(),
        accepted: true,
        venue_order_id: Some("42".into()),
        filled_size: 1.0,
        avg_fill_price: Some(110.0),
        message: "offline".into(),
    };
    db.finish_intent(&close.id, "filled", Some(&receipt))
        .unwrap();
    db.apply_confirmed_reduction(&position, &receipt, 0.0, "offline")
        .unwrap();
    assert!(admit(&aegis, &intent()).is_err());
    assert_eq!(counts(&db), (0, 1));
    assert!(db.ensure_execution_resolved().is_err());
}
#[test]
fn separate_connections_cannot_admit_two_unresolved_orders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let first = authority(&db);
    let other = Storage::open(&path).unwrap();
    let second = authority(&other);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [first, second]
        .into_iter()
        .map(|aegis| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                admit(&aegis, &intent())
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Ok(RiskDecision::Approved(_))))
            .count(),
        1
    );
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn admission_pair_survives_restart_without_becoming_retriable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restart.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let aegis = authority(&db);
    let order = intent();
    assert!(matches!(
        admit(&aegis, &order).unwrap(),
        RiskDecision::Approved(_)
    ));
    drop(aegis);
    drop(db);
    let db = Storage::open(path).unwrap();
    let aegis = authority(&db);
    assert_eq!(counts(&db), (1, 1));
    assert!(db.ensure_execution_resolved().is_err());
    assert!(admit(&aegis, &order).is_err());
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn authority_binding_requires_the_same_protected_storage_handle() {
    let db = database();
    let other = database();
    let aegis = authority(&other);
    assert!(hydra::execution::ExecutionEngine::new(db.clone(), true)
        .with_authority(aegis, 30.0)
        .is_err());
    assert!(hydra::execution::ExecutionEngine::new(db.clone(), true)
        .with_authority(authority(&db), 30.0)
        .is_ok());
}

#[test]
fn reused_terminal_journal_uuid_cannot_leave_an_orphan_reservation() {
    let db = database();
    let aegis = authority(&db);
    let order = intent();
    db.reserve_intent(&order).unwrap();
    db.finish_intent(
        &order.id,
        "rejected",
        Some(&OrderResult {
            intent_id: order.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "offline".into(),
        }),
    )
    .unwrap();
    assert!(admit(&aegis, &order).is_err());
    assert_eq!(counts(&db), (0, 1));
    assert_eq!(aegis.status().unwrap().latest_snapshot_revision, None);
}
#[test]
fn any_same_market_pending_order_blocks_atomic_admission() {
    use hydra::adapters::account_evidence::OpenOrderEvidence;
    for reduce_only in [false, true] {
        let db = database();
        let aegis = authority(&db);
        let mut proof = evidence();
        proof.open_orders.push(OpenOrderEvidence {
            oid: 42,
            symbol: "ETH".into(),
            size: dec!(1),
            limit_price: dec!(100),
            mark_price: dec!(100),
            reduce_only,
            order_type: "Limit".into(),
            is_trigger: false,
        });
        if !reduce_only {
            proof
                .snapshot
                .exposure_by_market
                .insert("ETH".into(), dec!(100));
        }
        assert!(aegis
            .reserve_journaled_entry(&intent(), dec!(100), Decimal::ONE, &proof, BASE + 10)
            .is_err());
        assert_eq!(counts(&db), (0, 0));
    }
}

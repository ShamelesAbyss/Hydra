//! Decimal receipt facts and exact entry projections; every input is offline.
use chrono::Utc;
use hydra::{
    adapters::account_evidence::{AccountExposureEvidence, PositionEvidence},
    aegis::{AccountSnapshot, Aegis, RiskDecision, RiskLimits},
    core::{OpenPosition, OrderIntent, OrderResult, Side},
    execution_facts::{self, PreciseFill},
    position_context,
    storage::Storage,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::BTreeMap;
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const EXACT_PRICE: &str = "100.000000000000000001";
fn database() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    db
}
fn intent(symbol: &str) -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: symbol.into(),
        side: Side::Long,
        size: 2.0,
        limit_price: Some(101.0),
        reduce_only: false,
        stop_price: Some(98.0),
        take_profit: Some(110.0),
        source_signal: None,
        created_at: Utc::now(),
    }
}
fn facts() -> PreciseFill {
    PreciseFill {
        quantity: "2.0000".into(),
        average_price: EXACT_PRICE.into(),
        order_id: "42".into(),
    }
}
fn result(order: &OrderIntent, facts: &PreciseFill) -> OrderResult {
    OrderResult {
        intent_id: order.id.clone(),
        accepted: true,
        venue_order_id: Some(facts.order_id.clone()),
        filled_size: facts.quantity.parse().unwrap(),
        avg_fill_price: Some(facts.average_price.parse().unwrap()),
        message: "offline typed receipt".into(),
    }
}
fn stage(db: &Storage, order: &OrderIntent, precise: bool) -> OrderResult {
    db.reserve_intent(order).unwrap();
    let facts = facts();
    let receipt = result(order, &facts);
    execution_facts::record_receipt(db, "filled", &receipt, precise.then_some(&facts)).unwrap();
    receipt
}
fn position(order: &OrderIntent, receipt: &OrderResult) -> OpenPosition {
    OpenPosition {
        id: 0,
        venue: order.venue.clone(),
        symbol: order.symbol.clone(),
        side: order.side.as_str().into(),
        size: receipt.filled_size,
        entry_price: receipt.avg_fill_price.unwrap(),
        stop_price: order.stop_price,
        take_profit: order.take_profit,
        opened_at: Utc::now(),
        strategy: None,
        trailing_high: receipt.avg_fill_price,
    }
}
fn open(db: &Storage, order: &OrderIntent, receipt: &OrderResult) -> OpenPosition {
    let mut p = position(order, receipt);
    p.id =
        position_context::insert_position_with_context(db, &p, &order.id, Some(2.0), None).unwrap();
    db.finish_intent(&order.id, "applied", Some(receipt))
        .unwrap();
    p
}
fn state(db: &Storage, id: &str) -> String {
    db.with_conn(|c| {
        c.query_row(
            "SELECT state FROM execution_intents WHERE intent_id=?1",
            [id],
            |r| r.get(0),
        )
    })
    .unwrap()
}
fn count(db: &Storage) -> i64 {
    db.with_conn(|c| {
        c.query_row("SELECT COUNT(*) FROM execution_fill_facts", [], |r| {
            r.get(0)
        })
    })
    .unwrap()
}
fn guard(db: &Storage) -> Aegis {
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
fn evidence(price: Decimal, quantity: Decimal) -> AccountExposureEvidence {
    let now = Utc::now().timestamp_millis();
    AccountExposureEvidence {
        account: ACCOUNT.into(),
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        account_mode: "disabled".into(),
        acquired_started_ms: now,
        acquired_finished_ms: now,
        exchange_time_ms: now,
        snapshot: AccountSnapshot {
            revision: now,
            observed_at_ms: now,
            equity: dec!(1000),
            exposure_by_market: BTreeMap::from([("ETH".into(), dec!(220))]),
        },
        positions: vec![PositionEvidence {
            symbol: "ETH".into(),
            signed_size: quantity,
            entry_price: price,
            notional: dec!(220),
        }],
        open_orders: vec![],
    }
}
#[test]
fn receipt_and_entry_precision_survive_restart_without_float_rounding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("precise.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let order = intent("ETH");
    let receipt = stage(&db, &order, true);
    let p = open(&db, &order, &receipt);
    drop(db);
    let db = Storage::open(path).unwrap();
    db.migrate().unwrap();
    let (qty,price,current):(String,String,String)=db.with_conn(|c|c.query_row("SELECT r.quantity,r.average_price,p.current_quantity FROM execution_fill_facts r JOIN position_precision_state p ON p.entry_intent_id=r.intent_id WHERE p.position_id=?1",[p.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))).unwrap();
    assert_eq!(qty, "2.0000");
    assert_eq!(current, qty);
    assert_eq!(price, EXACT_PRICE);
    assert_eq!(db.list_open_positions().unwrap()[0].entry_price, 100.0);
    let proof = evidence(Decimal::from_str_exact(EXACT_PRICE).unwrap(), dec!(2));
    let mut next = intent("BTC");
    next.size = 1.0;
    assert!(matches!(
        guard(&db)
            .reserve_journaled_entry(
                &next,
                dec!(101),
                Decimal::ONE,
                &proof,
                Utc::now().timestamp_millis()
            )
            .unwrap(),
        RiskDecision::Approved(_)
    ));
}
#[test]
fn precision_fact_write_failure_retains_submitted_journal_without_partial_facts() {
    let db = database();
    let order = intent("ETH");
    db.reserve_intent(&order).unwrap();
    let facts = facts();
    let receipt = result(&order, &facts);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_facts BEFORE INSERT ON execution_fill_facts BEGIN SELECT RAISE(ABORT,'offline facts failure'); END;")).unwrap();
    assert!(execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).is_err());
    assert_eq!(state(&db, &order.id), "submitted");
    assert_eq!(count(&db), 0);
    assert!(db.ensure_execution_resolved().is_err());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_facts"))
        .unwrap();
    execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).unwrap();
    assert_eq!(count(&db), 1);
    assert!(execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).is_err());
    assert_eq!(count(&db), 1);
}
#[test]
fn journal_status_failure_rolls_back_new_exact_facts() {
    let db = database();
    let order = intent("ETH");
    db.reserve_intent(&order).unwrap();
    let facts = facts();
    let receipt = result(&order, &facts);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_receipt BEFORE UPDATE ON execution_intents BEGIN SELECT RAISE(ABORT,'offline status failure'); END;")).unwrap();
    assert!(execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).is_err());
    assert_eq!(count(&db), 0);
    assert_eq!(state(&db, &order.id), "submitted");
}
#[test]
fn exact_overfill_cannot_hide_inside_identical_float_values() {
    let db = database();
    let order = intent("ETH");
    db.reserve_intent(&order).unwrap();
    let mut facts = facts();
    facts.quantity = "2.00000000000000000001".into();
    let receipt = result(&order, &facts);
    assert_eq!(receipt.filled_size, order.size);
    assert!(execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).is_err());
    assert_eq!(count(&db), 0);
    assert_eq!(state(&db, &order.id), "submitted");
}
#[test]
fn missing_precision_is_legacy_unknown_never_reconstructed_from_floats() {
    let db = database();
    let order = intent("ETH");
    let receipt = stage(&db, &order, false);
    open(&db, &order, &receipt);
    assert_eq!(count(&db), 0);
    let projected: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM position_precision_state", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(projected, 0);
    let aegis = guard(&db);
    let mut next = intent("BTC");
    next.size = 1.0;
    let proof = evidence(Decimal::from_str_exact(EXACT_PRICE).unwrap(), dec!(2));
    assert!(aegis
        .reserve_journaled_entry(
            &next,
            dec!(101),
            Decimal::ONE,
            &proof,
            Utc::now().timestamp_millis()
        )
        .is_err());
    let representable = evidence(dec!(100), dec!(2));
    assert!(matches!(
        aegis
            .reserve_journaled_entry(
                &next,
                dec!(101),
                Decimal::ONE,
                &representable,
                Utc::now().timestamp_millis()
            )
            .unwrap(),
        RiskDecision::Approved(_)
    ));
    assert_eq!(count(&db), 0);
}
#[test]
fn position_precision_write_failure_rolls_back_position_and_entry_metadata() {
    let db = database();
    let order = intent("ETH");
    let receipt = stage(&db, &order, true);
    let p = position(&order, &receipt);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_projection BEFORE INSERT ON position_precision_state BEGIN SELECT RAISE(ABORT,'offline projection failure'); END;")).unwrap();
    assert!(
        position_context::insert_position_with_context(&db, &p, &order.id, Some(2.0), None)
            .is_err()
    );
    assert!(db.list_open_positions().unwrap().is_empty());
    assert_eq!(count(&db), 1);
    assert_eq!(state(&db, &order.id), "filled");
    let metadata: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM position_entry_state", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(metadata, 0);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_projection"))
        .unwrap();
    assert!(
        position_context::insert_position_with_context(&db, &p, &order.id, Some(2.0), None).is_ok()
    );
}
fn close(db: &Storage, p: &OpenPosition) -> (OrderResult, f64) {
    let mut order = intent("ETH");
    order.reduce_only = true;
    order.side = Side::Short;
    order.size = 1.0;
    db.reserve_intent(&order).unwrap();
    let precise = PreciseFill {
        quantity: "1.0000".into(),
        average_price: "110.00".into(),
        order_id: "43".into(),
    };
    let receipt = result(&order, &precise);
    execution_facts::record_receipt(db, "filled", &receipt, Some(&precise)).unwrap();
    (receipt, p.size - 1.0)
}
#[test]
fn legacy_reduction_marks_current_precision_unknown_and_retains_original_receipts() {
    let db = database();
    let order = intent("ETH");
    let receipt = stage(&db, &order, true);
    let p = open(&db, &order, &receipt);
    let (closed, remaining) = close(&db, &p);
    db.apply_confirmed_reduction(&p, &closed, remaining, "offline")
        .unwrap();
    let quantity: Option<String> = db
        .with_conn(|c| {
            c.query_row(
                "SELECT current_quantity FROM position_precision_state WHERE position_id=?1",
                [p.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(quantity, None);
    assert_eq!(count(&db), 2);
    assert!(db.ensure_execution_resolved().is_err());
    let aegis = guard(&db);
    let mut next = intent("BTC");
    next.size = 1.0;
    let proof = evidence(Decimal::from_str_exact(EXACT_PRICE).unwrap(), dec!(1));
    let error = aegis
        .reserve_journaled_entry(
            &next,
            dec!(101),
            Decimal::ONE,
            &proof,
            Utc::now().timestamp_millis(),
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("exact residual quantity remains unknown"));
}
#[test]
fn precision_invalidation_failure_rolls_back_residual_and_reduction_application() {
    let db = database();
    let order = intent("ETH");
    let receipt = stage(&db, &order, true);
    let p = open(&db, &order, &receipt);
    let (closed, remaining) = close(&db, &p);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_invalidation BEFORE UPDATE ON position_precision_state BEGIN SELECT RAISE(ABORT,'offline invalidation failure'); END;")).unwrap();
    assert!(db
        .apply_confirmed_reduction(&p, &closed, remaining, "offline")
        .is_err());
    assert_eq!(db.list_open_positions().unwrap()[0].size, 2.0);
    assert_eq!(state(&db, &closed.intent_id), "filled");
    let reductions: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM confirmed_reductions", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(reductions, 0);
    let quantity: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT current_quantity FROM position_precision_state WHERE position_id=?1",
                [p.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(quantity, "2.0000");
}
#[test]
fn inconsistent_status_and_cross_account_precision_cannot_be_attached_to_positions() {
    let db = database();
    let order = intent("ETH");
    db.reserve_intent(&order).unwrap();
    let facts = facts();
    let receipt = result(&order, &facts);
    assert!(execution_facts::record_receipt(&db, "rejected", &receipt, None).is_err());
    assert_eq!(state(&db, &order.id), "submitted");
    execution_facts::record_receipt(&db, "filled", &receipt, Some(&facts)).unwrap();
    db.with_conn(|c|c.execute("UPDATE execution_fill_facts SET account='0x2222222222222222222222222222222222222222' WHERE intent_id=?1",[&order.id])).unwrap();
    assert!(position_context::insert_position_with_context(
        &db,
        &position(&order, &receipt),
        &order.id,
        Some(2.0),
        None
    )
    .is_err());
    assert!(db.list_open_positions().unwrap().is_empty());
}

#[test]
fn nonfinite_optional_prices_cannot_turn_into_a_clean_rejection_via_json_null() {
    let db = database();
    let order = intent("ETH");
    db.reserve_intent(&order).unwrap();
    let receipt = OrderResult {
        intent_id: order.id.clone(),
        accepted: false,
        venue_order_id: None,
        filled_size: 0.0,
        avg_fill_price: Some(f64::NAN),
        message: "offline malformed rejection".into(),
    };
    assert!(execution_facts::record_receipt(&db, "rejected", &receipt, None).is_err());
    assert_eq!(state(&db, &order.id), "submitted");
    assert_eq!(count(&db), 0);
}
struct ReceiptVenue {
    db: Storage,
    quantity: &'static str,
    calls: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl hydra::adapters::VenueAdapter for ReceiptVenue {
    fn kind(&self) -> hydra::core::VenueKind {
        hydra::core::VenueKind::Hyperliquid
    }
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn snapshot(&self, _: &str, _: usize) -> anyhow::Result<hydra::core::MarketSnapshot> {
        panic!("no authority in transport-boundary fixture")
    }
    async fn open_positions(&self) -> anyhow::Result<Vec<hydra::adapters::VenuePosition>> {
        panic!("unexpected account read")
    }
    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        panic!("unexpected account read")
    }
    async fn close_position(&self, _: &str, _: f64, _: bool) -> anyhow::Result<OrderResult> {
        panic!("journal path required")
    }
    async fn submit_order(&self, _: &OrderIntent) -> anyhow::Result<OrderResult> {
        panic!("must preserve extended receipt evidence")
    }
    async fn submit_order_with_evidence(
        &self,
        order: &OrderIntent,
    ) -> anyhow::Result<hydra::adapters::OrderReceipt> {
        assert_eq!(state(&self.db, &order.id), "submitted");
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut precise = facts();
        precise.quantity = self.quantity.into();
        Ok(hydra::adapters::OrderReceipt {
            result: result(order, &precise),
            precise_fill: Some(precise),
        })
    }
}
#[tokio::test]
async fn controller_records_extended_precision_without_using_legacy_submission() {
    let db = database();
    let order = intent("ETH");
    let venue = ReceiptVenue {
        db: db.clone(),
        quantity: "2.0000",
        calls: 0.into(),
    };
    let mut engine = hydra::execution::ExecutionEngine::new(db.clone(), true);
    let receipt = engine.submit(&order, &venue).await.unwrap();
    assert_eq!(receipt.avg_fill_price, Some(100.0));
    assert_eq!(count(&db), 1);
    assert_eq!(state(&db, &order.id), "filled");
    let exact: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT average_price FROM execution_fill_facts WHERE intent_id=?1",
                [&order.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(exact, EXACT_PRICE);
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[tokio::test]
async fn controller_keeps_hidden_exact_overfill_unknown_instead_of_applying_it() {
    let db = database();
    let order = intent("ETH");
    let venue = ReceiptVenue {
        db: db.clone(),
        quantity: "2.00000000000000000001",
        calls: 0.into(),
    };
    let mut engine = hydra::execution::ExecutionEngine::new(db.clone(), true);
    assert!(engine.submit(&order, &venue).await.is_err());
    assert_eq!(state(&db, &order.id), "acknowledgment_unknown");
    assert_eq!(count(&db), 0);
    assert!(db.ensure_execution_resolved().is_err());
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[tokio::test]
async fn controller_does_not_retry_when_receipt_facts_cannot_be_committed() {
    let db = database();
    let order = intent("ETH");
    let venue = ReceiptVenue {
        db: db.clone(),
        quantity: "2.0000",
        calls: 0.into(),
    };
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_controller_facts BEFORE INSERT ON execution_fill_facts BEGIN SELECT RAISE(ABORT,'offline facts failure'); END;")).unwrap();
    let mut engine = hydra::execution::ExecutionEngine::new(db.clone(), true);
    assert!(engine.submit(&order, &venue).await.is_err());
    assert_eq!(state(&db, &order.id), "submitted");
    assert_eq!(count(&db), 0);
    assert!(db.ensure_execution_resolved().is_err());
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn contradictory_journal_state_does_not_make_native_facts_look_like_legacy_data() {
    let db = database();
    let order = intent("ETH");
    let receipt = stage(&db, &order, true);
    let rejected = OrderResult {
        intent_id: order.id.clone(),
        accepted: false,
        venue_order_id: None,
        filled_size: 0.0,
        avg_fill_price: None,
        message: "offline inconsistent journal".into(),
    };
    db.finish_intent(&order.id, "rejected", Some(&rejected))
        .unwrap();
    assert!(position_context::insert_position_with_context(
        &db,
        &position(&order, &receipt),
        &order.id,
        Some(2.0),
        None
    )
    .is_err());
    assert!(db.list_open_positions().unwrap().is_empty());
    assert_eq!(count(&db), 1);
}

fn residual_snapshot(quantity: Decimal) -> hydra::adapters::position_evidence::PositionSnapshot {
    let now = Utc::now().timestamp_millis();
    hydra::adapters::position_evidence::PositionSnapshot {
        account: ACCOUNT.into(),
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        acquired_started_ms: now,
        acquired_finished_ms: now,
        exchange_time_ms: now,
        positions: if quantity == Decimal::ZERO {
            vec![]
        } else {
            vec![hydra::adapters::position_evidence::ExactPosition {
                symbol: "ETH".into(),
                signed_quantity: quantity,
                entry_price: Decimal::from_str_exact(EXACT_PRICE).unwrap(),
            }]
        },
    }
}
fn residual_count(db: &Storage) -> i64 {
    db.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM execution_residual_evidence",
            [],
            |r| r.get(0),
        )
    })
    .unwrap()
}
fn exact_remaining(db: &Storage, id: i64) -> Option<String> {
    db.with_conn(|c| {
        c.query_row(
            "SELECT current_quantity FROM position_precision_state WHERE position_id=?1",
            [id],
            |r| r.get(0),
        )
    })
    .unwrap()
}
#[test]
fn exact_residual_survives_restart_and_keeps_financial_hold() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("residual.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let order = intent("ETH");
    let entry = stage(&db, &order, true);
    let p = open(&db, &order, &entry);
    let (res, remaining) = close(&db, &p);
    db.apply_confirmed_reduction_with_snapshot(
        &p,
        &res,
        remaining,
        "offline",
        Some(&residual_snapshot(dec!(1))),
    )
    .unwrap();
    assert_eq!(exact_remaining(&db, p.id).as_deref(), Some("1.0000"));
    assert_eq!(residual_count(&db), 1);
    assert_eq!(state(&db, &res.intent_id), "applied");
    assert!(db.ensure_execution_resolved().is_err());
    drop(db);
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    assert_eq!(exact_remaining(&db, p.id).as_deref(), Some("1.0000"));
    assert_eq!(residual_count(&db), 1);
}
#[test]
fn full_exact_reduction_requires_complete_zero_snapshot() {
    let db = database();
    let order = intent("ETH");
    let entry = stage(&db, &order, true);
    let p = open(&db, &order, &entry);
    let mut close_order = intent("ETH");
    close_order.reduce_only = true;
    close_order.side = Side::Short;
    db.reserve_intent(&close_order).unwrap();
    let mut f = facts();
    f.order_id = "43".into();
    let res = result(&close_order, &f);
    execution_facts::record_receipt(&db, "filled", &res, Some(&f)).unwrap();
    assert!(db
        .apply_confirmed_reduction_with_snapshot(
            &p,
            &res,
            0.0,
            "offline",
            Some(&residual_snapshot(dec!(1)))
        )
        .is_err());
    db.apply_confirmed_reduction_with_snapshot(
        &p,
        &res,
        0.0,
        "offline",
        Some(&residual_snapshot(dec!(0))),
    )
    .unwrap();
    assert!(db.list_open_positions().unwrap().is_empty());
    assert_eq!(residual_count(&db), 1);
    assert_eq!(exact_remaining(&db, p.id).as_deref(), Some("0.0000"));
    assert!(db.ensure_execution_resolved().is_err());
}
#[test]
fn contradictory_residual_proof_cannot_use_float_tolerance() {
    for variant in 0..7 {
        let db = database();
        let order = intent("ETH");
        let entry = stage(&db, &order, true);
        let p = open(&db, &order, &entry);
        let (res, remaining) = close(&db, &p);
        let mut proof = residual_snapshot(dec!(1));
        let recorded: String = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT updated_at FROM execution_intents WHERE intent_id=?1",
                    [&res.intent_id],
                    |r| r.get(0),
                )
            })
            .unwrap();
        let before_receipt = chrono::DateTime::parse_from_rfc3339(&recorded)
            .unwrap()
            .timestamp_millis()
            - 1;
        match variant {
            0 => proof.positions[0].signed_quantity = dec!(1.000000000000000001),
            1 => proof.positions[0].entry_price = dec!(100.000000000000000002),
            2 => proof.account = "0x2222222222222222222222222222222222222222".into(),
            3 => proof.positions[0].signed_quantity = dec!(-1),
            4 => proof.positions.clear(),
            5 => proof.acquired_started_ms = before_receipt,
            _ => proof.exchange_time_ms = before_receipt,
        };
        assert!(
            db.apply_confirmed_reduction_with_snapshot(
                &p,
                &res,
                remaining,
                "offline",
                Some(&proof)
            )
            .is_err(),
            "variant {variant}"
        );
        assert_eq!(db.list_open_positions().unwrap()[0].size, 2.0);
        assert_eq!(state(&db, &res.intent_id), "filled");
        assert_eq!(residual_count(&db), 0);
        assert_eq!(exact_remaining(&db, p.id).as_deref(), Some("2.0000"));
    }
}
#[test]
fn every_residual_persistence_failure_rolls_back_all_state() {
    for sql in ["CREATE TRIGGER fail_residual BEFORE INSERT ON execution_residual_evidence BEGIN SELECT RAISE(ABORT,'offline'); END;",
        "CREATE TRIGGER fail_residual BEFORE UPDATE ON position_precision_state BEGIN SELECT RAISE(ABORT,'offline'); END;",
        "CREATE TRIGGER fail_residual BEFORE UPDATE ON positions BEGIN SELECT RAISE(ABORT,'offline'); END;",
        "CREATE TRIGGER fail_residual BEFORE UPDATE ON execution_intents BEGIN SELECT RAISE(ABORT,'offline'); END;"] {
        let db=database();let order=intent("ETH");let entry=stage(&db,&order,true);let p=open(&db,&order,&entry);let(res,remaining)=close(&db,&p);
        db.with_conn(|c|c.execute_batch(sql)).unwrap();
        assert!(db.apply_confirmed_reduction_with_snapshot(&p,&res,remaining,"offline",Some(&residual_snapshot(dec!(1)))).is_err());
        assert_eq!(db.list_open_positions().unwrap()[0].size,2.0);assert_eq!(state(&db,&res.intent_id),"filled");assert_eq!(residual_count(&db),0);assert_eq!(exact_remaining(&db,p.id).as_deref(),Some("2.0000"));
        let n:i64=db.with_conn(|c|c.query_row("SELECT COUNT(*) FROM confirmed_reductions",[],|r|r.get(0))).unwrap();assert_eq!(n,0);
    }
}
#[test]
fn known_unknown_residual_is_not_reconstructed_from_a_new_snapshot() {
    let db = database();
    let order = intent("ETH");
    let entry = stage(&db, &order, true);
    let p = open(&db, &order, &entry);
    let (res, remaining) = close(&db, &p);
    db.with_conn(|c| {
        c.execute(
            "UPDATE position_precision_state SET current_quantity=NULL",
            [],
        )
    })
    .unwrap();
    db.apply_confirmed_reduction_with_snapshot(
        &p,
        &res,
        remaining,
        "offline",
        Some(&residual_snapshot(dec!(1))),
    )
    .unwrap();
    assert_eq!(exact_remaining(&db, p.id), None);
    assert_eq!(residual_count(&db), 0);
    assert!(db.ensure_execution_resolved().is_err());
}
#[test]
fn changed_entry_identity_prevents_application_even_without_exact_proof() {
    let db = database();
    let order = intent("ETH");
    let entry = stage(&db, &order, true);
    let p = open(&db, &order, &entry);
    let (res, remaining) = close(&db, &p);
    db.with_conn(|c| c.execute("UPDATE positions SET entry_price=101 WHERE id=?1", [p.id]))
        .unwrap();
    assert!(db
        .apply_confirmed_reduction(&p, &res, remaining, "offline")
        .is_err());
    assert_eq!(state(&db, &res.intent_id), "filled");
}

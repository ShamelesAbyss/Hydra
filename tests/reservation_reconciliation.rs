//! Offline account evidence and reservation release contracts. No network adapter.
use chrono::{TimeZone, Utc};
use hydra::{
    adapters::account_evidence::{
        self, AccountExposureEvidence, OpenOrderEvidence, PositionEvidence,
    },
    aegis::{AccountSnapshot, Aegis, RiskDecision, RiskLimits},
    core::{OpenPosition, OrderIntent, OrderResult, Side},
    position_context,
    storage::Storage,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use std::collections::BTreeMap;
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const BASE: i64 = 1_700_000_000_000;
fn database() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    db
}
fn guard(db: &Storage) -> Aegis {
    Aegis::new(
        db.clone(),
        ACCOUNT,
        RiskLimits {
            max_total_exposure_ratio: dec!(1),
            max_market_notional: dec!(500),
            max_leverage: dec!(1),
            max_drawdown_ratio: dec!(0.2),
            max_snapshot_age_ms: 30_000,
        },
    )
    .unwrap()
}
fn initial() -> AccountSnapshot {
    AccountSnapshot {
        revision: BASE - 100,
        observed_at_ms: BASE - 100,
        equity: dec!(1000),
        exposure_by_market: BTreeMap::new(),
    }
}
fn intent(reduce_only: bool) -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Long,
        size: 2.0,
        limit_price: Some(100.0),
        reduce_only,
        stop_price: Some(98.0),
        take_profit: None,
        source_signal: None,
        created_at: Utc.timestamp_millis_opt(BASE - 90).unwrap(),
    }
}
fn reserve(db: &Storage, aegis: &Aegis) -> OrderIntent {
    let intent = intent(false);
    assert!(matches!(
        aegis
            .reserve_entry(
                &intent.id,
                "ETH",
                dec!(200),
                dec!(1),
                &initial(),
                BASE - 100
            )
            .unwrap(),
        RiskDecision::Approved(_)
    ));
    db.reserve_intent(&intent).unwrap();
    intent
}
fn timestamp(db: &Storage, id: &str) {
    db.with_conn(|c| {
        c.execute(
            "UPDATE execution_intents SET updated_at=?1 WHERE intent_id=?2",
            rusqlite::params![
                Utc.timestamp_millis_opt(BASE - 50).unwrap().to_rfc3339(),
                id
            ],
        )
    })
    .unwrap();
}
fn reject(db: &Storage, aegis: &Aegis) -> String {
    let entry = reserve(db, aegis);
    db.finish_intent(
        &entry.id,
        "rejected",
        Some(&OrderResult {
            intent_id: entry.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "offline definite rejection".into(),
        }),
    )
    .unwrap();
    timestamp(db, &entry.id);
    entry.id
}
fn applied(db: &Storage, aegis: &Aegis) -> (String, i64) {
    let entry = reserve(db, aegis);
    let p = OpenPosition {
        id: 0,
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: "LONG".into(),
        size: 2.0,
        entry_price: 100.0,
        stop_price: Some(98.0),
        take_profit: None,
        opened_at: Utc.timestamp_millis_opt(BASE - 60).unwrap(),
        strategy: Some("momentum".into()),
        trailing_high: Some(100.0),
    };
    let pid =
        position_context::insert_position_with_context(db, &p, &entry.id, Some(2.0), None).unwrap();
    db.finish_intent(
        &entry.id,
        "applied",
        Some(&OrderResult {
            intent_id: entry.id.clone(),
            accepted: true,
            venue_order_id: Some("42".into()),
            filled_size: 2.0,
            avg_fill_price: Some(100.0),
            message: "offline known fill".into(),
        }),
    )
    .unwrap();
    timestamp(db, &entry.id);
    (entry.id, pid)
}
fn evidence(owned: bool) -> AccountExposureEvidence {
    AccountExposureEvidence {
        account: ACCOUNT.into(),
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        account_mode: "disabled".into(),
        acquired_started_ms: BASE + 100,
        acquired_finished_ms: BASE + 110,
        exchange_time_ms: BASE + 105,
        snapshot: AccountSnapshot {
            revision: BASE + 100,
            observed_at_ms: BASE + 100,
            equity: dec!(1000),
            exposure_by_market: if owned {
                BTreeMap::from([("ETH".into(), dec!(200))])
            } else {
                BTreeMap::new()
            },
        },
        positions: if owned {
            vec![PositionEvidence {
                symbol: "ETH".into(),
                signed_size: dec!(2),
                entry_price: dec!(100),
                notional: dec!(200),
            }]
        } else {
            vec![]
        },
        open_orders: vec![],
    }
}
#[test]
fn definite_zero_fill_rejection_releases_once_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("risk.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let aegis = guard(&db);
    let id = reject(&db, &aegis);
    let proof = evidence(false);
    assert!(aegis.has_terminal_reservations().unwrap());
    assert_eq!(
        aegis
            .reconcile_terminal_entries(&proof, BASE + 110)
            .unwrap(),
        vec![id]
    );
    assert!(aegis
        .reconcile_terminal_entries(&proof, BASE + 110)
        .unwrap()
        .is_empty());
    assert_eq!(aegis.status().unwrap().active_reservations, 0);
    drop(aegis);
    drop(db);
    let db = Storage::open(path).unwrap();
    let aegis = guard(&db);
    assert_eq!(aegis.status().unwrap().active_reservations, 0);
    assert!(!aegis.has_terminal_reservations().unwrap());
    let events: i64 = db
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM aegis_events WHERE kind='reservation_reconciled'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(events, 1);
}
#[test]
fn applied_position_replaces_reservation_without_double_counting_and_preserves_breaker() {
    let db = database();
    let aegis = guard(&db);
    let (id, _) = applied(&db, &aegis);
    aegis.trip("operator hold", BASE).unwrap();
    assert_eq!(
        aegis
            .reconcile_terminal_entries(&evidence(true), BASE + 110)
            .unwrap(),
        vec![id]
    );
    assert_eq!(aegis.status().unwrap().reserved_notional, Decimal::ZERO);
    assert_eq!(
        aegis.status().unwrap().breaker_reason.as_deref(),
        Some("operator hold")
    );
    let stored: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT snapshot_json FROM aegis_accounts WHERE account=?1",
                [ACCOUNT],
                |r| r.get(0),
            )
        })
        .unwrap();
    let stored: AccountSnapshot = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored.exposure_by_market["ETH"], dec!(200));
}
#[test]
fn unknown_unapplied_resting_and_orphan_reservations_remain_held() {
    for state in ["submitted", "acknowledgment_unknown", "resting", "filled"] {
        let db = database();
        let aegis = guard(&db);
        let entry = reserve(&db, &aegis);
        db.finish_intent(&entry.id, state, None).unwrap();
        assert!(!aegis.has_terminal_reservations().unwrap());
        assert!(aegis
            .reconcile_terminal_entries(&evidence(false), BASE + 110)
            .unwrap()
            .is_empty());
        assert_eq!(aegis.status().unwrap().active_reservations, 1);
        assert!(db.ensure_execution_resolved().is_err());
    }
    let db = database();
    let aegis = guard(&db);
    aegis
        .reserve_entry(
            &uuid::Uuid::new_v4().to_string(),
            "ETH",
            dec!(200),
            dec!(1),
            &initial(),
            BASE - 100,
        )
        .unwrap();
    assert!(aegis
        .reconcile_terminal_entries(&evidence(false), BASE + 110)
        .unwrap()
        .is_empty());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
}
#[test]
fn account_scope_freshness_causality_and_aggregate_omissions_cannot_release() {
    let db = database();
    let aegis = guard(&db);
    let _ = reject(&db, &aegis);
    let original = evidence(false);
    for variant in 0..8 {
        let mut bad = original.clone();
        match variant {
            0 => bad.account = OTHER.into(),
            1 => bad.environment = "replay".into(),
            2 => bad.perp_dex = "other".into(),
            3 => bad.account_mode = "unified".into(),
            4 => {
                bad.acquired_started_ms = BASE - 60;
                bad.snapshot.revision = BASE - 60;
                bad.snapshot.observed_at_ms = BASE - 60;
            }
            5 => bad.exchange_time_ms = BASE - 60,
            6 => bad.acquired_finished_ms = BASE + 50_000,
            _ => {
                bad.snapshot
                    .exposure_by_market
                    .insert("ETH".into(), dec!(1));
            }
        }
        assert!(
            aegis.reconcile_terminal_entries(&bad, BASE + 110).is_err(),
            "variant {variant}"
        );
        assert_eq!(aegis.status().unwrap().active_reservations, 1);
    }
    assert!(aegis
        .reconcile_terminal_entries(&original, BASE + 40_000)
        .is_err());
}
#[test]
fn current_order_or_missing_entry_metadata_cannot_release_an_applied_entry() {
    let db = database();
    let aegis = guard(&db);
    let (_id, _) = applied(&db, &aegis);
    let mut proof = evidence(true);
    proof.open_orders.push(OpenOrderEvidence {
        oid: 42,
        symbol: "ETH".into(),
        size: dec!(1),
        limit_price: dec!(100),
        mark_price: dec!(100),
        reduce_only: false,
        order_type: "Limit".into(),
        is_trigger: false,
    });
    proof
        .snapshot
        .exposure_by_market
        .insert("ETH".into(), dec!(300));
    assert!(aegis
        .reconcile_terminal_entries(&proof, BASE + 110)
        .unwrap()
        .is_empty());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
    db.with_conn(|c| c.execute("DELETE FROM position_entry_state", []))
        .unwrap();
    assert!(aegis
        .reconcile_terminal_entries(&proof, BASE + 110)
        .unwrap()
        .is_empty());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
}
#[test]
fn conflicting_receipt_and_database_write_failure_roll_back_entire_batch() {
    let db = database();
    let aegis = guard(&db);
    let first = reject(&db, &aegis);
    let second = reject(&db, &aegis);
    db.with_conn(|c|c.execute("UPDATE execution_intents SET result_json=json_set(result_json,'$.intent_id','wrong') WHERE intent_id=?1",[&second])).unwrap();
    assert!(aegis
        .reconcile_terminal_entries(&evidence(false), BASE + 110)
        .is_err());
    assert_eq!(aegis.status().unwrap().active_reservations, 2);
    db.with_conn(|c|c.execute("UPDATE execution_intents SET result_json=json_set(result_json,'$.intent_id',?1) WHERE intent_id=?1",[&second])).unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_snapshot BEFORE UPDATE ON aegis_accounts BEGIN SELECT RAISE(ABORT,'offline failure'); END;")).unwrap();
    assert!(aegis
        .reconcile_terminal_entries(&evidence(false), BASE + 110)
        .is_err());
    assert_eq!(aegis.status().unwrap().active_reservations, 2);
    let released: i64 = db
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM aegis_reservations WHERE intent_id=?1 AND state='reconciled'",
                [&first],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(released, 0);
}
#[test]
fn reduced_entry_reservation_and_financial_settlement_hold_survive_other_release() {
    let db = database();
    let aegis = guard(&db);
    let (_entry, pid) = applied(&db, &aegis);
    let mut close = intent(true);
    close.size = 1.0;
    close.side = Side::Short;
    db.reserve_intent(&close).unwrap();
    let receipt = OrderResult {
        intent_id: close.id.clone(),
        accepted: true,
        venue_order_id: Some("43".into()),
        filled_size: 1.0,
        avg_fill_price: Some(110.0),
        message: "offline close".into(),
    };
    db.finish_intent(&close.id, "filled", Some(&receipt))
        .unwrap();
    let p = db
        .list_open_positions()
        .unwrap()
        .into_iter()
        .find(|p| p.id == pid)
        .unwrap();
    db.apply_confirmed_reduction(&p, &receipt, 1.0, "offline")
        .unwrap();
    let rejected = reject(&db, &aegis);
    let mut proof = evidence(true);
    proof.positions[0].signed_size = dec!(1);
    proof.positions[0].notional = dec!(100);
    proof
        .snapshot
        .exposure_by_market
        .insert("ETH".into(), dec!(100));
    assert_eq!(
        aegis
            .reconcile_terminal_entries(&proof, BASE + 110)
            .unwrap(),
        vec![rejected]
    );
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
    assert!(db.ensure_execution_resolved().is_err());
    let unknown: i64 = db
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM confirmed_reductions WHERE net_pnl IS NULL",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(unknown, 1);
}
fn raw_state() -> serde_json::Value {
    json!({"assetPositions":[{"position":{"coin":"ETH","szi":"2","entryPx":"100","positionValue":"220"}}],"marginSummary":{"accountValue":"1000"},"time":BASE+5})
}
fn parse(
    state: &serde_json::Value,
    orders: &serde_json::Value,
) -> anyhow::Result<AccountExposureEvidence> {
    account_evidence::parse_hyperliquid(
        ACCOUNT,
        "disabled",
        &state.to_string(),
        &orders.to_string(),
        r#"{"ETH":"110"}"#,
        BASE,
        BASE + 10,
    )
}
#[test]
fn account_parser_derives_positions_and_pending_exposure_with_exact_decimals() {
    let orders = json!([{"oid":7,"coin":"ETH","sz":"1","limitPx":"120","reduceOnly":false,"orderType":"Limit","isTrigger":false}]);
    let proof = parse(&raw_state(), &orders).unwrap();
    assert_eq!(proof.positions[0].signed_size, dec!(2));
    assert_eq!(proof.snapshot.exposure_by_market["ETH"], dec!(340));
    assert_eq!(proof.exchange_time_ms, BASE + 5);
    let divergent_mark = account_evidence::parse_hyperliquid(
        ACCOUNT,
        "disabled",
        &raw_state().to_string(),
        "[]",
        r#"{"ETH":"5"}"#,
        BASE,
        BASE + 10,
    )
    .unwrap();
    assert_eq!(divergent_mark.snapshot.exposure_by_market["ETH"], dec!(220));

    let mut omitted = proof.clone();
    omitted
        .snapshot
        .exposure_by_market
        .insert("ETH".into(), dec!(220));
    assert!(omitted.validate(BASE + 10, 30_000).is_err());
}
#[test]
fn account_parser_rejects_precision_loss_duplicate_identity_and_unsupported_nested_orders() {
    let mut state = raw_state();
    state["assetPositions"][0]["position"]["szi"] = json!("0.00000000000000000000000000001");
    assert!(parse(&state, &json!([])).is_err());
    state = raw_state();
    let duplicated = state["assetPositions"][0].clone();
    state["assetPositions"]
        .as_array_mut()
        .unwrap()
        .push(duplicated);
    assert!(parse(&state, &json!([])).is_err());
    for orders in [
        json!([{"oid":7,"coin":"ETH","sz":"1","limitPx":"120","reduceOnly":false,"orderType":"Limit","isTrigger":false,"children":[{}]}]),
        json!([{"oid":7,"coin":"ETH","sz":"1","limitPx":"120","reduceOnly":false,"orderType":"Limit","isTrigger":false},{"oid":7,"coin":"ETH","sz":"1","limitPx":"120","reduceOnly":false,"orderType":"Limit","isTrigger":false}]),
    ] {
        assert!(parse(&raw_state(), &orders).is_err());
    }
    let mut future = raw_state();
    future["time"] = json!(BASE + 100);
    assert!(parse(&future, &json!([])).is_err());
}

struct EntryVenue {
    evidence: Option<AccountExposureEvidence>,
    calls: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl hydra::adapters::VenueAdapter for EntryVenue {
    fn kind(&self) -> hydra::core::VenueKind {
        hydra::core::VenueKind::Hyperliquid
    }
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn snapshot(
        &self,
        symbol: &str,
        _: usize,
    ) -> anyhow::Result<hydra::core::MarketSnapshot> {
        Ok(hydra::core::MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
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
    async fn open_positions(&self) -> anyhow::Result<Vec<hydra::adapters::VenuePosition>> {
        panic!("reconciled evidence must be reused instead of a second position read")
    }
    async fn risk_snapshot(&self) -> anyhow::Result<AccountSnapshot> {
        panic!("reconciled evidence must be reused instead of a second risk snapshot")
    }
    async fn account_exposure_evidence(&self) -> anyhow::Result<AccountExposureEvidence> {
        self.evidence
            .clone()
            .ok_or_else(|| anyhow::anyhow!("offline evidence unavailable"))
    }
    async fn submit_order(&self, intent: &OrderIntent) -> anyhow::Result<OrderResult> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(OrderResult {
            intent_id: intent.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "offline rejection".into(),
        })
    }
    async fn close_position(&self, _: &str, _: f64, _: bool) -> anyhow::Result<OrderResult> {
        panic!("journaled submit must be used")
    }
    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        Ok(1000.0)
    }
}
#[tokio::test]
async fn entry_reuses_scoped_snapshot_after_reconciliation() {
    let db = database();
    let aegis = guard(&db);
    let old = reject(&db, &aegis);
    let now = Utc::now().timestamp_millis();
    let mut proof = evidence(false);
    proof.acquired_started_ms = now - 5;
    proof.acquired_finished_ms = now - 1;
    proof.exchange_time_ms = now - 2;
    proof.snapshot.revision = now - 5;
    proof.snapshot.observed_at_ms = now - 5;
    let venue = EntryVenue {
        evidence: Some(proof),
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
    let mut engine = hydra::execution::ExecutionEngine::new(db.clone(), true)
        .with_authority(aegis.clone(), 30.0)
        .unwrap();
    assert!(
        !engine
            .submit(&intent(false), &venue)
            .await
            .unwrap()
            .accepted
    );
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let state: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT state FROM aegis_reservations WHERE intent_id=?1",
                [old],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "reconciled");
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
}
#[tokio::test]
async fn reconciliation_read_failure_blocks_entry_but_never_reduction_submission() {
    let db = database();
    let aegis = guard(&db);
    reject(&db, &aegis);
    let venue = EntryVenue {
        evidence: None,
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
    let mut engine = hydra::execution::ExecutionEngine::new(db, true)
        .with_authority(aegis, 30.0)
        .unwrap();
    assert!(engine.submit(&intent(false), &venue).await.is_err());
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        !engine
            .close(&venue, "ETH", 1.0, false)
            .await
            .unwrap()
            .accepted
    );
    assert_eq!(venue.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[test]
fn closed_positions_and_unbounded_parser_inputs_remain_ineligible() {
    let db = database();
    let aegis = guard(&db);
    let (_, pid) = applied(&db, &aegis);
    db.with_conn(|c| {
        c.execute(
            "UPDATE positions SET closed_at='offline' WHERE id=?1",
            [pid],
        )
    })
    .unwrap();
    assert!(aegis
        .reconcile_terminal_entries(&evidence(false), BASE + 110)
        .unwrap()
        .is_empty());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
    let oversized = " ".repeat(4 * 1024 * 1024 + 1);
    assert!(account_evidence::parse_hyperliquid(
        ACCOUNT,
        "disabled",
        &oversized,
        "[]",
        "{}",
        BASE,
        BASE + 10
    )
    .is_err());
    let mut too_many = raw_state();
    let raw = too_many["assetPositions"][0].clone();
    too_many["assetPositions"] = json!(vec![raw; 1025]);
    assert!(parse(&too_many, &json!([])).is_err());
}

#[test]
fn zero_quantity_cannot_hide_nonzero_or_unknown_position_value() {
    let mut state = raw_state();
    state["assetPositions"][0]["position"]["szi"] = json!("0");
    assert!(parse(&state, &json!([])).is_err());
    state["assetPositions"][0]["position"]
        .as_object_mut()
        .unwrap()
        .remove("positionValue");
    assert!(parse(&state, &json!([])).is_err());
    state["assetPositions"][0]["position"]["positionValue"] = json!("0");
    let empty = parse(&state, &json!([])).unwrap();
    assert!(empty.positions.is_empty());
    assert!(empty.snapshot.exposure_by_market.is_empty());
}

#[test]
fn batch_evidence_survives_newer_snapshots_without_duplicate_replay_records() {
    let db = database();
    let aegis = guard(&db);
    reject(&db, &aegis);
    let original = evidence(false);
    aegis
        .reconcile_terminal_entries(&original, BASE + 110)
        .unwrap();
    let mut newer = original.clone();
    newer.acquired_started_ms += 100;
    newer.acquired_finished_ms += 100;
    newer.exchange_time_ms += 100;
    newer.snapshot.revision += 100;
    newer.snapshot.observed_at_ms += 100;
    newer.snapshot.equity = dec!(1100);
    assert!(aegis
        .reconcile_terminal_entries(&newer, BASE + 210)
        .unwrap()
        .is_empty());
    let (count, json): (i64, String) = db
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*),evidence_json FROM aegis_reconciliations WHERE account=?1",
                [ACCOUNT],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(count, 1);
    let retained: AccountExposureEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(retained, original);
    assert_eq!(
        aegis.status().unwrap().latest_snapshot_revision,
        Some(newer.snapshot.revision)
    );
}

#[test]
fn evidence_archive_write_failure_rolls_back_release_snapshot_and_events() {
    let db = database();
    let aegis = guard(&db);
    reject(&db, &aegis);
    let before = aegis.status().unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_proof BEFORE INSERT ON aegis_reconciliations BEGIN SELECT RAISE(ABORT,'offline proof failure'); END;")).unwrap();
    let proof = evidence(false);
    assert!(aegis
        .reconcile_terminal_entries(&proof, BASE + 110)
        .is_err());
    let after = aegis.status().unwrap();
    assert_eq!(after.active_reservations, before.active_reservations);
    assert_eq!(
        after.latest_snapshot_revision,
        before.latest_snapshot_revision
    );
    let count: i64 = db
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM aegis_events WHERE kind='reservation_reconciled'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(count, 0);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_proof"))
        .unwrap();
    assert_eq!(
        aegis
            .reconcile_terminal_entries(&proof, BASE + 110)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn contingent_unknown_and_missing_entry_order_types_cannot_understate_risk() {
    let base = json!({"oid":7,"coin":"ETH","sz":"1","limitPx":"120","reduceOnly":false,"orderType":"Limit","isTrigger":false});
    assert!(parse(&raw_state(), &json!([base.clone()])).is_ok());
    for kind in ["Stop Market", "Stop Limit", "Unknown"] {
        let mut order = base.clone();
        order["orderType"] = json!(kind);
        assert!(parse(&raw_state(), &json!([order])).is_err());
    }
    let mut trigger = base.clone();
    trigger["isTrigger"] = json!(true);
    assert!(parse(&raw_state(), &json!([trigger])).is_err());
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("orderType");
    assert!(parse(&raw_state(), &json!([missing])).is_err());
    let mut protective = base;
    protective["isTrigger"] = json!(true);
    protective["orderType"] = json!("Stop Market");
    protective["reduceOnly"] = json!(true);
    let accepted = parse(&raw_state(), &json!([protective])).unwrap();
    assert_eq!(accepted.snapshot.exposure_by_market["ETH"], dec!(220));
}

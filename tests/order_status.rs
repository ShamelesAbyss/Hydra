//! Offline native lifecycle evidence; no account probes or signed requests.
use chrono::{TimeZone, Utc};
use hydra::{
    core::{OrderIntent, Side},
    order_status::{self, Observation, Outcome},
    storage::Storage,
};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const NOW: i64 = 1_800_000_000_000;
fn intent() -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Long,
        size: 2.0,
        limit_price: Some(101.0),
        reduce_only: false,
        stop_price: Some(98.0),
        take_profit: Some(110.0),
        source_signal: None,
        created_at: Utc.timestamp_millis_opt(NOW - 1000).unwrap(),
    }
}
fn response(i: &OrderIntent, status: &str) -> Value {
    json!({"status":"order","order":{"order":{"coin":"ETH","side":"B","limitPx":"100.000000000000000001","sz":"0.0000","origSz":"2.0000","oid":42,"timestamp":NOW-500,"cloid":order_status::client_order_id(&i.id).unwrap(),"reduceOnly":false,"isTrigger":false,"isPositionTpsl":false,"children":[],"orderType":"Limit","tif":"Ioc"},"status":status,"statusTimestamp":NOW-100}})
}
fn parse(i: &OrderIntent, body: &Value) -> anyhow::Result<Observation> {
    order_status::parse_hyperliquid(ACCOUNT, i, &body.to_string(), NOW, NOW)
}
fn database(i: &OrderIntent) -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    db.reserve_intent(i).unwrap();
    db
}
fn count(db: &Storage) -> i64 {
    db.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM execution_status_observations",
            [],
            |r| r.get(0),
        )
    })
    .unwrap()
}
fn held(db: &Storage, i: &OrderIntent) {
    assert!(db.ensure_execution_resolved().is_err());
    let state: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT state FROM execution_intents WHERE intent_id=?1",
                [&i.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "submitted");
    assert!(db.list_open_positions().unwrap().is_empty());
    let facts: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM execution_fill_facts", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(facts, 0);
}
#[test]
fn request_uses_canonical_client_identity_and_no_signing_fields() {
    let mut i = intent();
    i.id = "01234567-89ab-cdef-0123-456789abcdef".into();
    let body = order_status::request_body(ACCOUNT, &i).unwrap();
    assert_eq!(
        body,
        json!({"type":"orderStatus","user":ACCOUNT,"oid":"0x0123456789abcdef0123456789abcdef"})
    );
    for bad in ["01234567-89AB-CDEF-0123-456789ABCDEF", "not-a-uuid"] {
        i.id = bad.into();
        assert!(order_status::request_body(ACCOUNT, &i).is_err());
    }
    i = intent();
    i.symbol = "@107".into();
    assert!(order_status::request_body(ACCOUNT, &i).is_err());
}
#[test]
fn preserves_exact_selected_order_facts_without_fabricating_fill_price() {
    let i = intent();
    let obs = parse(&i, &response(&i, "canceled")).unwrap();
    let Outcome::Order(order) = obs.outcome else {
        panic!("expected observed order")
    };
    assert_eq!(order.original_size, dec!(2));
    assert_eq!(order.remaining_size, dec!(0));
    assert_eq!(order.limit_price, dec!(100.000000000000000001));
    assert_eq!(order.status, "canceled");
    assert_eq!(order.order_id, "42");
    assert!(order.buy);
}
#[test]
fn missing_and_future_lifecycle_labels_remain_observations() {
    let i = intent();
    assert!(matches!(
        parse(&i, &json!({"status":"unknownOid"})).unwrap().outcome,
        Outcome::Missing
    ));
    let future = parse(&i, &response(&i, "futureVenueLifecycleStatus")).unwrap();
    let Outcome::Order(order) = future.outcome else {
        panic!("expected order")
    };
    assert_eq!(order.status, "futureVenueLifecycleStatus");
    let mut contradictory = response(&i, "filled");
    contradictory["status"] = json!("unknownOid");
    assert!(parse(&i, &contradictory).is_err());
}
#[test]
fn returned_order_identity_must_match_every_intent_scope_field() {
    let i = intent();
    for variant in 0..6 {
        let mut body = response(&i, "open");
        let o = &mut body["order"]["order"];
        match variant {
            0 => o["coin"] = json!("BTC"),
            1 => o["side"] = json!("A"),
            2 => o["reduceOnly"] = json!(true),
            3 => o["cloid"] = Value::Null,
            4 => o["cloid"] = json!("0x00000000000000000000000000000000"),
            _ => o["oid"] = json!(0),
        };
        assert!(parse(&i, &body).is_err(), "variant {variant}");
    }
}
#[test]
fn unsupported_or_inconsistent_orders_cannot_form_native_evidence() {
    let i = intent();
    for variant in 0..8 {
        let mut body = response(&i, "open");
        let o = &mut body["order"]["order"];
        match variant {
            0 => o["origSz"] = json!("2.000000000000000001"),
            1 => o["sz"] = json!("-1"),
            2 => o["sz"] = json!("3"),
            3 => o["limitPx"] = json!("0"),
            4 => o["isTrigger"] = json!(true),
            5 => o["isPositionTpsl"] = json!(true),
            6 => o["children"] = json!([{}]),
            _ => o["side"] = json!("unknown"),
        };
        assert!(parse(&i, &body).is_err());
    }
}
#[test]
fn body_text_decimal_and_lifecycle_time_budgets_fail_closed() {
    let i = intent();
    assert!(order_status::parse_hyperliquid(ACCOUNT, &i, &" ".repeat(65537), NOW, NOW).is_err());
    for variant in 0..5 {
        let mut body = response(&i, "filled");
        match variant {
            0 => body["order"]["statusTimestamp"] = json!(NOW + 1),
            1 => body["order"]["statusTimestamp"] = json!(NOW - 501),
            2 => body["order"]["order"]["limitPx"] = json!("9".repeat(129)),
            3 => body["order"]["status"] = json!("x".repeat(65)),
            _ => body["order"]["order"]["orderType"] = json!("bad\ntext"),
        };
        assert!(parse(&i, &body).is_err());
    }
}
#[test]
fn acquisition_and_account_scope_do_not_refresh_old_observations() {
    let i = intent();
    let original = parse(&i, &json!({"status":"unknownOid"})).unwrap();
    assert!(original.validate(&i, ACCOUNT, NOW + 30001).is_err());
    let db = database(&i);
    assert!(order_status::record_observation(&db, &original, NOW + 30001).is_err());
    assert_eq!(count(&db), 0);
    for variant in 0..5 {
        let mut obs = original.clone();
        match variant {
            0 => obs.account = "0x2222222222222222222222222222222222222222".into(),
            1 => obs.environment = "testnet".into(),
            2 => obs.acquired_started_ms = NOW - 1001,
            3 => obs.acquired_finished_ms = NOW + 1,
            _ => obs.intent_id = uuid::Uuid::new_v4().to_string(),
        };
        assert!(obs.validate(&i, ACCOUNT, NOW).is_err());
    }
    assert!(order_status::parse_hyperliquid(
        ACCOUNT,
        &i,
        &json!({"status":"unknownOid"}).to_string(),
        NOW,
        NOW + 15001
    )
    .is_err());
}
#[test]
fn lifecycle_observations_never_mutate_execution_or_financial_holds() {
    for status in ["missing", "filled", "rejected", "canceled"] {
        let i = intent();
        let db = database(&i);
        let body = if status == "missing" {
            json!({"status":"unknownOid"})
        } else {
            response(&i, status)
        };
        let obs = parse(&i, &body).unwrap();
        assert!(order_status::record_observation(&db, &obs, NOW).unwrap());
        assert_eq!(count(&db), 1);
        held(&db, &i);
    }
}
#[test]
fn durable_replay_is_idempotent_and_conflicting_identity_preserves_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("status.db");
    let i = intent();
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    db.reserve_intent(&i).unwrap();
    let obs = parse(&i, &json!({"status":"unknownOid"})).unwrap();
    assert!(order_status::record_observation(&db, &obs, NOW).unwrap());
    drop(db);
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    assert!(!order_status::record_observation(&db, &obs, NOW + 60_001).unwrap());
    let mut changed = parse(&i, &response(&i, "filled")).unwrap();
    changed.id = obs.id;
    assert!(order_status::record_observation(&db, &changed, NOW).is_err());
    assert_eq!(count(&db), 1);
    held(&db, &i);
}
#[test]
fn persistence_failures_and_invalid_binding_cannot_partially_record_or_release() {
    let i = intent();
    let db = database(&i);
    let obs = parse(&i, &json!({"status":"unknownOid"})).unwrap();
    let mut other = obs.clone();
    other.account = "0x2222222222222222222222222222222222222222".into();
    assert!(order_status::record_observation(&db, &other, NOW).is_err());
    assert_eq!(count(&db), 0);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_status BEFORE INSERT ON execution_status_observations BEGIN SELECT RAISE(ABORT,'offline'); END;")).unwrap();
    assert!(order_status::record_observation(&db, &obs, NOW).is_err());
    assert_eq!(count(&db), 0);
    held(&db, &i);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_status"))
        .unwrap();
    order_status::record_observation(&db, &obs, NOW).unwrap();
    db.with_conn(|c| {
        c.execute(
            "UPDATE execution_status_observations SET observation_json=?1",
            [" ".repeat(65537)],
        )
    })
    .unwrap();
    assert!(order_status::record_observation(&db, &obs, NOW).is_err());
    assert_eq!(count(&db), 1);
    held(&db, &i);
}

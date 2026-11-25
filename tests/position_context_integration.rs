use chrono::Utc;
use hydra::{
    core::{OpenPosition, OrderIntent, Side},
    position_context::{self, EntryStateOrigin},
    storage::Storage,
};

fn position() -> OpenPosition {
    OpenPosition {
        id: 0,
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: "LONG".into(),
        size: 1.0,
        entry_price: 100.0,
        stop_price: Some(98.0),
        take_profit: Some(108.0),
        opened_at: Utc::now(),
        strategy: Some("momentum".into()),
        trailing_high: Some(100.0),
    }
}
fn reserve(storage: &Storage) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    storage
        .reserve_intent(&OrderIntent {
            id: id.clone(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: Side::Long,
            size: 1.0,
            limit_price: Some(100.0),
            reduce_only: false,
            stop_price: Some(98.0),
            take_profit: Some(108.0),
            source_signal: None,
            created_at: Utc::now(),
        })
        .unwrap();
    id
}
fn db() -> Storage {
    let storage = Storage::open(":memory:").unwrap();
    storage.migrate().unwrap();
    storage
}

#[test]
fn original_r_and_context_survive_restart_and_stop_ratcheting() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.db");
    let storage = Storage::open(&path).unwrap();
    storage.migrate().unwrap();
    let intent = reserve(&storage);
    let context = r#"{"strategy":"momentum","side":"LONG","atr_pct":0.3}"#;
    let id = position_context::insert_position_with_context(
        &storage,
        &position(),
        &intent,
        Some(2.0),
        Some(context),
    )
    .unwrap();
    storage
        .update_position_trailing(id, 106.0, Some(104.0))
        .unwrap();
    drop(storage);
    let reopened = Storage::open(path).unwrap();
    reopened.migrate().unwrap();
    let state = position_context::load_entry_state(&reopened, id).unwrap();
    assert_eq!(state.origin, EntryStateOrigin::Recorded);
    assert_eq!(state.initial_r, Some(2.0));
    assert_eq!(state.entry_intent_id.as_deref(), Some(intent.as_str()));
    assert_eq!(state.context_json.as_deref(), Some(context));
    assert_eq!(
        reopened.list_open_positions().unwrap()[0].stop_price,
        Some(104.0)
    );
}

#[test]
fn legacy_entries_remain_unknown_even_if_their_current_stop_is_available() {
    let storage = db();
    let id = storage.insert_open_position(&position()).unwrap();
    let state = position_context::load_entry_state(&storage, id).unwrap();
    assert_eq!(state.origin, EntryStateOrigin::LegacyUnknown);
    assert_eq!(state.initial_r, None);
    assert_eq!(state.context_json, None);
    position_context::migrate(&storage).unwrap();
    assert_eq!(
        position_context::load_entry_state(&storage, id).unwrap(),
        state
    );
    assert!(position_context::load_entry_state(&storage, id + 1).is_err());
}

#[test]
fn metadata_write_failure_and_duplicate_intent_roll_back_position_insertion() {
    let storage = db();
    let intent = reserve(&storage);
    position_context::migrate(&storage).unwrap();
    storage.with_conn(|conn|conn.execute_batch("CREATE TRIGGER reject_context BEFORE INSERT ON position_entry_state BEGIN SELECT RAISE(ABORT,'injected metadata failure'); END;")).unwrap();
    assert!(position_context::insert_position_with_context(
        &storage,
        &position(),
        &intent,
        Some(2.0),
        None
    )
    .is_err());
    assert!(storage.list_open_positions().unwrap().is_empty());
    storage
        .with_conn(|conn| conn.execute_batch("DROP TRIGGER reject_context;"))
        .unwrap();
    position_context::insert_position_with_context(&storage, &position(), &intent, Some(2.0), None)
        .unwrap();
    assert!(position_context::insert_position_with_context(
        &storage,
        &position(),
        &intent,
        Some(2.0),
        None
    )
    .is_err());
    assert_eq!(storage.list_open_positions().unwrap().len(), 1);
}

#[test]
fn invalid_original_facts_and_unbounded_context_are_rejected_before_insertion() {
    let storage = db();
    let intent = reserve(&storage);
    for risk in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(position_context::insert_position_with_context(
            &storage,
            &position(),
            &intent,
            Some(risk),
            None
        )
        .is_err());
    }
    for context in [
        "[]".to_string(),
        "null".to_string(),
        "bad".to_string(),
        format!("{{\"x\":\"{}\"}}", "a".repeat(33000)),
        format!("{{\"x\":{}}}", "[".repeat(10) + "0" + &"]".repeat(10)),
    ] {
        assert!(position_context::insert_position_with_context(
            &storage,
            &position(),
            &intent,
            None,
            Some(&context)
        )
        .is_err());
    }
    let mut invalid = position();
    invalid.size = f64::NAN;
    assert!(position_context::insert_position_with_context(
        &storage, &invalid, &intent, None, None
    )
    .is_err());
    assert!(position_context::insert_position_with_context(
        &storage,
        &position(),
        &intent.replace('-', ""),
        None,
        None
    )
    .is_err());
    assert!(storage.list_open_positions().unwrap().is_empty());
}

#[test]
fn missing_journal_identity_cannot_leave_an_orphan_position() {
    let storage = db();
    let missing = uuid::Uuid::new_v4().to_string();
    assert!(position_context::insert_position_with_context(
        &storage,
        &position(),
        &missing,
        Some(2.0),
        None
    )
    .is_err());
    assert!(storage.list_open_positions().unwrap().is_empty());
}

#[test]
fn entry_metadata_cannot_be_attached_to_another_instrument_intent() {
    let storage = db();
    let intent = reserve(&storage);
    let mut other = position();
    other.symbol = "BTC".into();
    assert!(position_context::insert_position_with_context(
        &storage,
        &other,
        &intent,
        Some(2.0),
        None
    )
    .is_err());
    assert!(storage.list_open_positions().unwrap().is_empty());
}

fn recovery_fixture() -> (Storage, String, Vec<hydra::adapters::VenuePosition>) {
    let storage = db();
    storage
        .bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let intent = reserve(&storage);
    position_context::insert_position_with_context(&storage, &position(), &intent, Some(2.0), None)
        .unwrap();
    storage
        .finish_intent(
            &intent,
            "filled",
            Some(&hydra::core::OrderResult {
                intent_id: intent.clone(),
                accepted: true,
                venue_order_id: Some("42".into()),
                filled_size: 1.0,
                avg_fill_price: Some(100.0),
                message: "offline fixture".into(),
            }),
        )
        .unwrap();
    let remote = vec![hydra::adapters::VenuePosition {
        symbol: "ETH".into(),
        size: 1.0,
        entry_price: 100.0,
        unrealized_pnl: 0.0,
        leverage: 1.0,
    }];
    (storage, intent, remote)
}

#[test]
fn known_entry_crash_window_recovers_once_without_duplicate_position() {
    let (storage, intent, remote) = recovery_fixture();
    let account = "0x1111111111111111111111111111111111111111";
    assert_eq!(
        position_context::recover_applied_entries(&storage, account, "hyperliquid", &remote)
            .unwrap(),
        vec![intent]
    );
    assert!(
        position_context::recover_applied_entries(&storage, account, "hyperliquid", &remote)
            .unwrap()
            .is_empty()
    );
    storage.ensure_execution_resolved().unwrap();
    assert_eq!(storage.list_open_positions().unwrap().len(), 1);
}

#[test]
fn recovery_requires_matching_account_position_and_known_receipt() {
    let (storage, intent, mut remote) = recovery_fixture();
    let account = "0x1111111111111111111111111111111111111111";
    assert!(position_context::recover_applied_entries(
        &storage,
        "0x2222222222222222222222222222222222222222",
        "hyperliquid",
        &remote
    )
    .is_err());
    remote[0].size = 2.0;
    assert!(
        position_context::recover_applied_entries(&storage, account, "hyperliquid", &remote)
            .is_err()
    );
    assert!(storage.ensure_execution_resolved().is_err());
    remote[0].size = 1.0;
    storage
        .finish_intent(&intent, "acknowledgment_unknown", None)
        .unwrap();
    assert!(
        position_context::recover_applied_entries(&storage, account, "hyperliquid", &remote)
            .unwrap()
            .is_empty()
    );
    assert!(storage.ensure_execution_resolved().is_err());
}

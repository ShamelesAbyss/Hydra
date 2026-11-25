//! Read-only operator inspection never constructs a venue or trading engine.
use chrono::{TimeZone, Utc};
use hydra::{
    core::{OrderIntent, Side},
    order_status, status_history,
    storage::Storage,
};
use std::path::Path;
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
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: Utc.timestamp_millis_opt(NOW - 1000).unwrap(),
    }
}
fn database(path: &Path) -> (Storage, OrderIntent) {
    let db = Storage::open(path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let i = intent();
    db.reserve_intent(&i).unwrap();
    (db, i)
}
fn observe(db: &Storage, i: &OrderIntent) -> order_status::Observation {
    let obs = order_status::parse_hyperliquid(ACCOUNT, i, r#"{"status":"unknownOid"}"#, NOW, NOW)
        .unwrap();
    order_status::record_observation(db, &obs, NOW).unwrap();
    obs
}
#[test]
fn read_only_inspection_preserves_bytes_and_original_times() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let (db, i) = database(&path);
    let first = observe(&db, &i);
    let second = observe(&db, &i);
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let h = status_history::inspect(&path, &i.id, 20).unwrap();
    assert!(h.read_only);
    assert!(!h.source_is_live);
    assert!(!h.exchange_history_complete);
    assert!(h.table_available);
    assert_eq!(h.journal_state, "submitted");
    assert_eq!(h.observations.len(), 2);
    assert_eq!(h.observations[0].observation.id, second.id);
    assert_eq!(h.observations[1].observation.id, first.id);
    assert_eq!(h.observations[0].observation.acquired_finished_ms, NOW);
    assert_eq!(h.observations[0].recorded_at_ms, NOW);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn absent_files_are_not_created_and_input_budgets_fail_before_open() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.db");
    let i = intent();
    assert!(status_history::inspect(&missing, &i.id, 20).is_err());
    assert!(!missing.exists());
    for limit in [0, 65, usize::MAX] {
        assert!(status_history::inspect(&missing, &i.id, limit).is_err());
        assert!(!missing.exists());
    }
    assert!(status_history::inspect(&missing, "not-an-id", 20).is_err());
    assert!(!missing.exists());
}
#[test]
fn older_history_reports_unavailable_table_without_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let (db, i) = database(&path);
    db.with_conn(|c| c.execute_batch("DROP TABLE execution_status_observations"))
        .unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let h = status_history::inspect(&path, &i.id, 20).unwrap();
    assert!(!h.table_available);
    assert!(h.observations.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn unknown_identity_binding_or_newer_schema_is_never_guessed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.db");
    let (db, i) = database(&path);
    assert!(status_history::inspect(&path, &intent().id, 20).is_err());
    db.with_conn(|c| {
        c.execute(
            "DELETE FROM meta_kv WHERE key='trading_account_binding'",
            [],
        )
    })
    .unwrap();
    assert!(status_history::inspect(&path, &i.id, 20).is_err());
    // Removing only one binding key leaves retained binding-audit metadata.
    // Do not rewrite that history to set up the independent version test.
    let newer = dir.path().join("newer.db");
    let (db, i) = database(&newer);
    db.with_conn(|c| c.pragma_update(None, "user_version", 3))
        .unwrap();
    assert!(status_history::inspect(&newer, &i.id, 20).is_err());
}
#[test]
fn limits_and_global_scan_window_are_explicitly_partial() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window.db");
    let (db, i) = database(&path);
    observe(&db, &i);
    observe(&db, &i);
    let h = status_history::inspect(&path, &i.id, 1).unwrap();
    assert_eq!(h.observations.len(), 1);
    assert!(h.records_capped);
    assert!(!h.window_capped);
    let filler = intent();
    db.reserve_intent(&filler).unwrap();
    let mut obs =
        order_status::parse_hyperliquid(ACCOUNT, &filler, r#"{"status":"unknownOid"}"#, NOW, NOW)
            .unwrap();
    db.with_conn(|c|->anyhow::Result<()>{let tx=c.unchecked_transaction()?;for n in 1..=10000u128 {obs.id=uuid::Uuid::from_u128(n).to_string();tx.execute("INSERT INTO execution_status_observations(observation_id,intent_id,account,requested_cloid,acquired_started_ms,acquired_finished_ms,observation_json,recorded_at_ms,source) VALUES(?1,?2,?3,?4,?5,?5,?6,?5,'native_hyperliquid_info_order_status')",rusqlite::params![obs.id,obs.intent_id,obs.account,obs.requested_cloid,NOW,serde_json::to_string(&obs)?])?;}tx.commit()?;Ok(())}).unwrap();
    let h = status_history::inspect(&path, &i.id, 20).unwrap();
    assert!(h.window_capped);
    assert!(h.observations.is_empty());
    assert!(!h.exchange_history_complete);
}
#[test]
fn corrupt_selected_provenance_and_oversized_payloads_fail_closed() {
    for variant in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.db");
        let (db, i) = database(&path);
        let obs = observe(&db, &i);
        db.with_conn(|c| match variant {
            0 => c.execute(
                "UPDATE execution_status_observations SET account=?1",
                ["0x2222222222222222222222222222222222222222"],
            ),
            1 => c.execute(
                "UPDATE execution_status_observations SET observation_json=?1",
                [" ".repeat(65537)],
            ),
            2 => c.execute(
                "UPDATE execution_status_observations SET requested_cloid=?1",
                ["0x00000000000000000000000000000000"],
            ),
            _ => c.execute(
                "UPDATE execution_status_observations SET acquired_finished_ms=?1",
                [NOW + 1],
            ),
        })
        .unwrap();
        assert!(
            status_history::inspect(&path, &i.id, 20).is_err(),
            "{}",
            obs.id
        );
    }
}
#[test]
fn schema_views_and_oversized_binding_are_not_trusted_as_native_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema.db");
    let (db, i) = database(&path);
    db.with_conn(|c|c.execute_batch("DROP TABLE execution_status_observations; CREATE VIEW execution_status_observations AS SELECT 1;")).unwrap();
    assert!(status_history::inspect(&path, &i.id, 20).is_err());
    db.with_conn(|c| {
        c.execute(
            "UPDATE meta_kv SET value=?1 WHERE key='trading_account_binding'",
            ["x".repeat(65537)],
        )
    })
    .unwrap();
    assert!(status_history::inspect(&path, &i.id, 20).is_err());
}
#[test]
fn cli_bypasses_missing_configuration_and_invalid_wallet_environment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cli.db");
    let (db, i) = database(&path);
    observe(&db, &i);
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(dir.path())
        .args(["--config", "absent.toml", "status-history", "--db"])
        .arg(&path)
        .args(["--intent", &i.id, "--limit", "1"])
        .env("ENABLE_LIVE_TRADING", "true")
        .env("HYPERLIQUID_PRIVATE_KEY", "invalid-never-used")
        .env("HYPERLIQUID_ACCOUNT_ADDRESS", "invalid-never-used")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["read_only"], true);
    assert_eq!(value["source_is_live"], false);
    assert_eq!(value["exchange_history_complete"], false);
    assert_eq!(value["observations"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

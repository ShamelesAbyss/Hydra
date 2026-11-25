use hydra::{
    core::{OrderIntent, Side},
    storage::Storage,
};
#[test]
fn inspection_bypasses_configuration_wallet_and_database_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cli.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let i = OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Long,
        size: 1.0,
        limit_price: Some(100.0),
        reduce_only: false,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: chrono::Utc::now(),
    };
    db.reserve_intent(&i).unwrap();
    db.with_conn(|c| c.execute_batch("DROP TABLE execution_wire_requests"))
        .unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(dir.path())
        .args(["--config", "missing.toml", "wire-history", "--db"])
        .arg(&path)
        .args(["--intent", &i.id])
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
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["read_only"], true);
    assert_eq!(v["source_is_live"], false);
    assert_eq!(v["table_available"], false);
    assert!(v["record"].is_null());
    assert_eq!(v["submission_or_acceptance_proven"], false);
    assert_eq!(v["signed_envelope_available"], false);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

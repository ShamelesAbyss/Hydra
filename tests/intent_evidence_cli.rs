use hydra::{
    core::{OrderIntent, Side},
    storage::Storage,
};
#[test]
fn combined_cli_bypasses_trading_configuration_and_preserves_database() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("history.sqlite");
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
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(d.path())
        .args(["--config", "missing.toml", "intent-evidence", "--db"])
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
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["action"]["schema_version"], 2);
    assert_eq!(v["lifecycle"]["schema_version"], 1);
    assert_eq!(v["action"]["account"], v["lifecycle"]["account"]);
    assert_eq!(v["journal_state"], "submitted");
    assert!(v["native_acknowledged_quantity"].is_null());
    for key in [
        "source_is_live",
        "exchange_history_complete",
        "automatic_repair_authorized",
        "risk_release_authorized",
        "financial_settlement_verified",
    ] {
        assert_eq!(v[key], false, "{key}");
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn missing_history_and_invalid_limits_never_initialize_database() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("missing.sqlite");
    let id = uuid::Uuid::new_v4().to_string();
    for limit in ["0", "20", "65"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
            .current_dir(d.path())
            .args(["--config", "missing.toml", "intent-evidence", "--db"])
            .arg(&path)
            .args(["--intent", &id, "--limit", limit])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(!path.exists());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("failed to load configuration"));
    }
}

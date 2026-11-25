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

#[test]
fn prepared_history_is_offline_protocol_two_without_signature_or_submission_proof() {
    use hydra::{
        hyperliquid_wire::{action_hash, agent_digest, OrderAction, UnsignedAction},
        wire_history::Preparation,
    };
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("prepared.sqlite");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    let account = "0x1111111111111111111111111111111111111111";
    let signer = "0x2222222222222222222222222222222222222222";
    db.bind_trading_account(account, false).unwrap();
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
    let a = OrderAction::ioc(4, true, 100.0, 1.0, false, &i.id).unwrap();
    // Synthetic persisted metadata fixture, not a real signer or venue request.
    let action = UnsignedAction {
        asset: 4,
        buy: true,
        price: "100".into(),
        size: "1".into(),
        reduce_only: false,
        client_order_id: hydra::order_status::client_order_id(&i.id).unwrap(),
        tif: "Ioc".into(),
        grouping: "na".into(),
        messagepack_hex: hex::encode(a.packed().unwrap()),
    };
    let hash = action_hash(&a.packed().unwrap(), 100, None, None);
    let recorded = i.created_at.timestamp_millis();
    let p = Preparation {
        intent_id: i.id.clone(),
        account: account.into(),
        signer: signer.into(),
        nonce: "100".into(),
        connection_hash: hex::encode(hash),
        agent_digest: hex::encode(agent_digest(hash, true)),
        domain_chain_id: 1337,
        agent_source: "a".into(),
        vault_address: None,
        expires_after_ms: None,
        phase: "prepared_not_sent".into(),
        prepared_at_ms: recorded + 1,
    };
    db.with_conn(|c|c.execute("INSERT INTO execution_wire_requests(intent_id,account,venue,environment,size_decimals,action_json,recorded_at_ms,source) VALUES(?1,?2,'hyperliquid','mainnet',4,?3,?4,'native_ioc_unsigned_action')",rusqlite::params![i.id,account,serde_json::to_string(&action).unwrap(),recorded])).unwrap();
    db.with_conn(|c|c.execute("INSERT INTO execution_signing_preparations(intent_id,account,signer,nonce,environment,preparation_json,source) VALUES(?1,?2,?3,'100','mainnet',?4,'native_ioc_signing_preparation')",rusqlite::params![i.id,account,signer,serde_json::to_string(&p).unwrap()])).unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(d.path())
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
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["read_only"], true);
    assert_eq!(value["source_is_live"], false);
    assert_eq!(value["signed_envelope_available"], false);
    assert_eq!(value["submission_or_acceptance_proven"], false);
    assert_eq!(
        value["signing_preparation"],
        serde_json::to_value(p).unwrap()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

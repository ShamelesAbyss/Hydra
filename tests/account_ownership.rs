use hydra::ownership::{canonical_account, AccountOwnership};
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";

#[test]
fn ownership_is_independent_of_database_and_released_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let first = AccountOwnership::acquire(root.path(), ACCOUNT).unwrap();
    assert!(AccountOwnership::acquire(root.path(), ACCOUNT).is_err());
    let other =
        AccountOwnership::acquire(root.path(), "0x2222222222222222222222222222222222222222")
            .unwrap();
    drop(first);
    assert!(AccountOwnership::acquire(root.path(), ACCOUNT).is_ok());
    drop(other);
}
#[test]
fn canonical_identity_rejects_path_injection_zero_and_malformed_addresses() {
    for bad in [
        "../../foo",
        "0x0000000000000000000000000000000000000000",
        "0xGG11111111111111111111111111111111111111",
    ] {
        assert!(canonical_account(bad).is_err());
    }
    assert_eq!(
        canonical_account("0xAb11111111111111111111111111111111111111").unwrap(),
        "0xab11111111111111111111111111111111111111"
    );
}

#[test]
fn process_lock_child() {
    if let Some(root) = std::env::var_os("HYDRA_OFFLINE_LOCK_TEST") {
        assert!(AccountOwnership::acquire(std::path::Path::new(&root), ACCOUNT).is_err());
    }
}

#[test]
fn independent_process_cannot_acquire_an_owned_account() {
    let root = tempfile::tempdir().unwrap();
    let _guard = AccountOwnership::acquire(root.path(), ACCOUNT).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_lock_child", "--nocapture"])
        .env("HYDRA_OFFLINE_LOCK_TEST", root.path())
        .output()
        .unwrap();
    assert!(result.status.success());
}

#[test]
fn stale_equity_and_debug_output_never_supply_a_fictional_balance_or_key() {
    let db = hydra::storage::Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    let mut portfolio = hydra::portfolio::PortfolioManager::new(db);
    assert!(portfolio
        .verified_equity(std::time::Duration::from_secs(30))
        .is_err());
    portfolio.sync_equity("hyperliquid", 123.0).unwrap();
    assert_eq!(
        portfolio
            .verified_equity(std::time::Duration::from_secs(30))
            .unwrap(),
        123.0
    );
    portfolio.invalidate_equity();
    assert!(portfolio
        .verified_equity(std::time::Duration::from_secs(30))
        .is_err());
    assert!(portfolio.sync_equity("hyperliquid", f64::NAN).is_err());
    let mut config = hydra::config::AppConfig::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap();
    config.hl_private_key = Some("OFFLINE_FAKE_SECRET_SENTINEL".into());
    let debug = format!("{config:?}");
    assert!(!debug.contains("OFFLINE_FAKE_SECRET_SENTINEL"));
    assert!(debug.contains("REDACTED"));
}

#[test]
fn a_live_database_cannot_silently_change_accounts_or_adopt_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bound.db");
    let db = hydra::storage::Storage::open(&path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    assert!(db
        .bind_trading_account("0x2222222222222222222222222222222222222222", true)
        .is_err());
    drop(db);
    let db = hydra::storage::Storage::open(path).unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let legacy = hydra::storage::Storage::open(":memory:").unwrap();
    legacy.migrate().unwrap();
    legacy.with_conn(|c| c.execute("INSERT INTO sessions(started_at,venues,symbols,live_trading) VALUES('offline','hyperliquid','ETH',1)",[])).unwrap();
    assert!(legacy.bind_trading_account(ACCOUNT, false).is_err());
    legacy.bind_trading_account(ACCOUNT, true).unwrap();
    assert!(legacy
        .bind_trading_account("0x2222222222222222222222222222222222222222", false)
        .is_err());
}

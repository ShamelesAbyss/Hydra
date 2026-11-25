//! One bounded local read snapshot. This report is never recovery authority.
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{path::Path, time::Duration};

#[derive(Debug, Serialize)]
pub struct Evidence {
    pub schema_version: u32,
    pub read_only: bool,
    pub source_is_live: bool,
    pub exchange_history_complete: bool,
    pub automatic_repair_authorized: bool,
    pub risk_release_authorized: bool,
    pub financial_settlement_verified: bool,
    pub account: String,
    pub intent_id: String,
    pub journal_state: String,
    pub native_acknowledgment_table_available: bool,
    pub native_acknowledged_quantity: Option<String>,
    pub action: crate::wire_history::History,
    pub lifecycle: crate::status_history::History,
}

pub fn inspect(path: &Path, intent_id: &str, limit: usize) -> Result<Evidence> {
    inspect_with_hook(path, intent_id, limit, || Ok(()))
}
fn inspect_with_hook(
    path: &Path,
    intent_id: &str,
    limit: usize,
    after_action: impl FnOnce() -> Result<()>,
) -> Result<Evidence> {
    ensure!(
        (1..=64).contains(&limit),
        "intent evidence observation limit must be in 1..=64"
    );
    crate::order_status::client_order_id(intent_id)?;
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("opening existing intent evidence read-only")?;
    conn.busy_timeout(Duration::from_millis(500))?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update(None, "trusted_schema", false)?;
    let tx = conn.unchecked_transaction()?;
    let action = crate::wire_history::inspect_on(&tx, intent_id)?;
    after_action()?;
    let lifecycle = crate::status_history::inspect_on(&tx, intent_id, limit)?;
    ensure!(
        action.account == lifecycle.account
            && action.intent_id == lifecycle.intent_id
            && action.journal_state == lifecycle.journal_state,
        "local evidence scope disagrees"
    );
    let native_available = crate::status_history::table(&tx, "execution_fill_facts")?;
    let native = crate::execution_facts::native_ack_quantity_on(&tx, intent_id)?;
    let quantity = if let Some((account, quantity)) = native {
        ensure!(
            account == action.account,
            "native acknowledgment account disagrees"
        );
        Some(quantity.to_string())
    } else {
        None
    };
    let report = Evidence {
        schema_version: 1,
        read_only: true,
        source_is_live: false,
        exchange_history_complete: false,
        automatic_repair_authorized: false,
        risk_release_authorized: false,
        financial_settlement_verified: false,
        account: action.account.clone(),
        intent_id: intent_id.into(),
        journal_state: action.journal_state.clone(),
        native_acknowledgment_table_available: native_available,
        native_acknowledged_quantity: quantity,
        action,
        lifecycle,
    };
    tx.commit()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{OrderIntent, OrderResult, Side},
        execution_facts::PreciseFill,
        storage::Storage,
    };
    const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
    fn setup(path: &Path) -> (Storage, OrderIntent) {
        let db = Storage::open(path).unwrap();
        db.migrate().unwrap();
        db.bind_trading_account(ACCOUNT, false).unwrap();
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
        (db, i)
    }
    fn acknowledge(db: &Storage, i: &OrderIntent) {
        let f = PreciseFill {
            quantity: "1.0000".into(),
            average_price: "100".into(),
            order_id: "42".into(),
        };
        let r = OrderResult {
            intent_id: i.id.clone(),
            accepted: true,
            venue_order_id: Some("42".into()),
            filled_size: 1.0,
            avg_fill_price: Some(100.0),
            message: "offline fixture".into(),
        };
        crate::execution_facts::record_receipt(db, "filled", &r, Some(&f)).unwrap();
    }
    #[test]
    fn combined_snapshot_preserves_bytes_and_does_not_authorize_repair_or_settlement() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("intent.sqlite");
        let (db, i) = setup(&p);
        acknowledge(&db, &i);
        drop(db);
        let before = std::fs::read(&p).unwrap();
        let r = inspect(&p, &i.id, 20).unwrap();
        assert_eq!(r.native_acknowledged_quantity.as_deref(), Some("1.0000"));
        assert_eq!(r.journal_state, "filled");
        assert!(r.read_only);
        assert!(
            !r.source_is_live
                && !r.exchange_history_complete
                && !r.automatic_repair_authorized
                && !r.risk_release_authorized
                && !r.financial_settlement_verified
        );
        assert!(r.action.record.is_none());
        assert!(r.action.signing_preparation.is_none());
        assert!(r.lifecycle.observations.is_empty());
        assert_eq!(std::fs::read(&p).unwrap(), before);
    }
    #[test]
    fn both_histories_use_one_wal_snapshot_across_concurrent_journal_update() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("snapshot.sqlite");
        let (db, i) = setup(&p);
        let r = inspect_with_hook(&p, &i.id, 20, || {
            let writer = Storage::open(&p)?;
            writer.finish_intent(&i.id, "rejected", None)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(r.journal_state, "submitted");
        assert_eq!(r.action.journal_state, "submitted");
        assert_eq!(r.lifecycle.journal_state, "submitted");
        assert_eq!(inspect(&p, &i.id, 20).unwrap().journal_state, "rejected");
        drop(db);
    }
    #[test]
    fn missing_database_invalid_identity_and_budget_do_not_create_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("missing.sqlite");
        let id = uuid::Uuid::new_v4().to_string();
        for limit in [0, 65, usize::MAX] {
            assert!(inspect(&p, &id, limit).is_err());
        }
        assert!(inspect(&p, "invalid", 20).is_err());
        assert!(inspect(&p, &id, 20).is_err());
        assert!(!p.exists());
    }
    #[test]
    fn missing_optional_history_is_explicit_without_migration() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("old.sqlite");
        let (db, i) = setup(&p);
        db.with_conn(|c|c.execute_batch("DROP TABLE execution_signing_preparations;DROP TABLE execution_wire_requests;DROP TABLE execution_status_observations;DROP TABLE execution_fill_facts")).unwrap();
        let r = inspect(&p, &i.id, 20).unwrap();
        assert!(
            !r.action.table_available
                && !r.action.signing_preparation_table_available
                && !r.lifecycle.table_available
        );
        assert!(r.native_acknowledged_quantity.is_none());
        assert!(!r.native_acknowledgment_table_available);
        assert!(!r.exchange_history_complete);
    }
    #[test]
    fn oversized_native_fact_or_journal_metadata_is_refused_without_fallback() {
        for sql in [
            "UPDATE execution_fill_facts SET quantity=printf('%0129d',1)",
            "UPDATE execution_fill_facts SET symbol=printf('%0161d',1)",
            "UPDATE execution_intents SET state=printf('%065d',1)",
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("corrupt.sqlite");
            let (db, i) = setup(&p);
            acknowledge(&db, &i);
            db.with_conn(|c| c.execute_batch(sql)).unwrap();
            assert!(inspect(&p, &i.id, 20).is_err());
            assert!(db.ensure_execution_resolved().is_err());
        }
    }
    #[test]
    fn native_views_unindexed_facts_and_shadowed_lifecycle_rowids_are_refused() {
        for sql in ["DROP TABLE execution_fill_facts;CREATE VIEW execution_fill_facts AS SELECT 1",
            "ALTER TABLE execution_fill_facts RENAME TO old_facts;CREATE TABLE execution_fill_facts AS SELECT * FROM old_facts",
            "ALTER TABLE execution_status_observations RENAME TO old_observations;CREATE TABLE execution_status_observations AS SELECT 1 AS rowid,* FROM old_observations"] {
            let d=tempfile::tempdir().unwrap();let p=d.path().join("shape.sqlite");let(db,i)=setup(&p);
            db.with_conn(|c|c.execute_batch(sql)).unwrap();assert!(inspect(&p,&i.id,20).is_err(),"{sql}");
        }
    }
}

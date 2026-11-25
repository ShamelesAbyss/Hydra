//! Bounded read-only inspection of unsigned pre-submission evidence.
use crate::{core::OrderIntent, hyperliquid_wire::UnsignedAction, status_history::table};
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags};
use rust_decimal::Decimal;
use serde::Serialize;
use std::{path::Path, time::Duration};

#[derive(Debug, Serialize)]
pub struct History {
    pub schema_version: u32,
    pub read_only: bool,
    pub source_is_live: bool,
    pub submission_or_acceptance_proven: bool,
    pub signed_envelope_available: bool,
    pub account: String,
    pub intent_id: String,
    pub journal_state: String,
    pub table_available: bool,
    pub record: Option<ArchivedAction>,
}
#[derive(Debug, Serialize)]
pub struct ArchivedAction {
    pub recorded_at_ms: i64,
    pub size_decimals: u8,
    pub action: UnsignedAction,
}

/// Opens an existing file without migrations, trading configuration or network.
/// Absence of a retained record does not establish that no submission occurred.
pub fn inspect(path: &Path, intent_id: &str) -> Result<History> {
    let cloid = crate::order_status::client_order_id(intent_id)?;
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("opening existing wire history read-only")?;
    conn.busy_timeout(Duration::from_millis(500))?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update(None, "trusted_schema", false)?;
    let tx = conn.unchecked_transaction()?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    ensure!(
        (0..=2).contains(&version),
        "unsupported core history schema"
    );
    ensure!(
        table(&tx, "meta_kv")? && table(&tx, "execution_intents")?,
        "intent/account history unavailable"
    );
    let account: String = tx.query_row("SELECT CASE WHEN length(CAST(value AS BLOB))=42 THEN value END FROM meta_kv WHERE key='trading_account_binding'", [], |r|r.get(0))?;
    ensure!(
        crate::ownership::canonical_account(&account)? == account,
        "invalid history account binding"
    );
    let (state, encoded): (String, String) = tx.query_row("SELECT CASE WHEN length(CAST(state AS BLOB))<=64 THEN state END,CASE WHEN length(CAST(intent_json AS BLOB))<=32768 THEN intent_json END FROM execution_intents WHERE intent_id=?1", [intent_id], |r|Ok((r.get(0)?,r.get(1)?)))?;
    ensure!(
        !state.is_empty() && state.bytes().all(|b| b.is_ascii_graphic()),
        "unsupported journal state"
    );
    let intent: OrderIntent = serde_json::from_str(&encoded)?;
    ensure!(
        intent.id == intent_id
            && intent.venue == "hyperliquid"
            && intent.size.is_finite()
            && intent.size > 0.0,
        "invalid archived intent"
    );
    let available = table(&tx, "execution_wire_requests")?;
    let mut history = History {
        schema_version: 1,
        read_only: true,
        source_is_live: false,
        submission_or_acceptance_proven: false,
        signed_envelope_available: false,
        account,
        intent_id: intent_id.into(),
        journal_state: state,
        table_available: available,
        record: None,
    };
    if available {
        // Require the migration's single-column primary key so a selected-intent
        // read cannot become an unbounded scan over a forged ordinary table.
        let keys: Vec<String> = {
            let mut schema = tx.prepare("SELECT name FROM pragma_table_info('execution_wire_requests') WHERE pk>0 ORDER BY pk")?;
            let keys = schema
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            keys
        };
        ensure!(
            keys == ["intent_id"],
            "unsupported wire history primary key"
        );
        let mut stmt = tx.prepare("SELECT CASE WHEN length(CAST(account AS BLOB))=42 THEN account END,CASE WHEN length(CAST(venue AS BLOB))<=64 THEN venue END,CASE WHEN length(CAST(environment AS BLOB))<=32 THEN environment END,size_decimals,CASE WHEN length(CAST(action_json AS BLOB))<=4096 THEN action_json END,recorded_at_ms,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END FROM execution_wire_requests WHERE intent_id=?1 LIMIT 2")?;
        let mut rows = stmt.query([intent_id])?;
        if let Some(row) = rows.next()? {
            let saved_account: String = row.get(0)?;
            let venue: String = row.get(1)?;
            let environment: String = row.get(2)?;
            let dp: u8 = row.get(3)?;
            let json: String = row.get(4)?;
            let recorded: i64 = row.get(5)?;
            let source: String = row.get(6)?;
            ensure!(
                saved_account == history.account
                    && venue == "hyperliquid"
                    && environment == "mainnet"
                    && source == "native_ioc_unsigned_action"
                    && dp <= 6
                    && recorded > 0,
                "archived wire scope disagrees"
            );
            let action: UnsignedAction = serde_json::from_str(&json)?;
            action.validate()?;
            let quantity = Decimal::from_str_exact(&action.size)?;
            ensure!(
                action.client_order_id == cloid
                    && action.buy == intent.side.is_buy()
                    && action.reduce_only == intent.reduce_only
                    && quantity.normalize().scale() <= u32::from(dp)
                    && quantity <= Decimal::from_str_exact(&intent.size.to_string())?,
                "archived action disagrees with intent"
            );
            history.record = Some(ArchivedAction {
                recorded_at_ms: recorded,
                size_decimals: dp,
                action,
            });
            ensure!(rows.next()?.is_none(), "duplicate archived wire records");
        }
    }
    tx.commit()?;
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::Side, hyperliquid_wire::OrderAction, storage::Storage,
        submission_wire::SubmissionJournal,
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
            size: 2.0,
            limit_price: Some(101.0),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: chrono::Utc::now(),
        };
        db.reserve_intent(&i).unwrap();
        let action = OrderAction::ioc(4, true, 101.0, 1.2345, false, &i.id).unwrap();
        SubmissionJournal::new(&db, &i)
            .record(ACCOUNT, &action, 4)
            .unwrap();
        (db, i)
    }
    fn change(db: &Storage, key: &str, value: serde_json::Value) {
        let saved: String = db
            .with_conn(|c| {
                c.query_row("SELECT action_json FROM execution_wire_requests", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        json[key] = value;
        db.with_conn(|c| {
            c.execute(
                "UPDATE execution_wire_requests SET action_json=?1",
                [json.to_string()],
            )
        })
        .unwrap();
    }
    #[test]
    fn inspection_preserves_database_bytes_times_and_unsigned_limits() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("wire.db");
        let (db, i) = setup(&p);
        let time: i64 = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT recorded_at_ms FROM execution_wire_requests",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        drop(db);
        let before = std::fs::read(&p).unwrap();
        let h = inspect(&p, &i.id).unwrap();
        assert!(h.read_only && h.table_available);
        assert!(
            !h.source_is_live && !h.submission_or_acceptance_proven && !h.signed_envelope_available
        );
        assert_eq!(h.journal_state, "submitted");
        let r = h.record.unwrap();
        assert_eq!(r.recorded_at_ms, time);
        assert_eq!(r.action.size, "1.2345");
        assert_eq!(r.size_decimals, 4);
        assert_eq!(std::fs::read(&p).unwrap(), before);
    }
    #[test]
    fn absent_files_and_invalid_identity_never_create_history() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("missing.db");
        assert!(inspect(&p, &uuid::Uuid::new_v4().to_string()).is_err());
        assert!(!p.exists());
        assert!(inspect(&p, "invalid").is_err());
        assert!(!p.exists());
    }
    #[test]
    fn absent_table_and_absent_record_are_distinct_without_migration() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("legacy.db");
        let (db, i) = setup(&p);
        db.with_conn(|c| c.execute("DELETE FROM execution_wire_requests", []))
            .unwrap();
        let h = inspect(&p, &i.id).unwrap();
        assert!(h.table_available && h.record.is_none());
        db.with_conn(|c| c.execute_batch("DROP TABLE execution_wire_requests"))
            .unwrap();
        let h = inspect(&p, &i.id).unwrap();
        assert!(!h.table_available && h.record.is_none());
        let count: i64 = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name='execution_wire_requests'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn incompatible_schema_and_views_are_refused() {
        for kind in 0..3 {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("schema.db");
            let (db, i) = setup(&p);
            if kind == 1 {
                db.with_conn(|c|c.execute_batch("DROP TABLE execution_wire_requests; CREATE VIEW execution_wire_requests AS SELECT 1")).unwrap();
            } else if kind == 2 {
                db.with_conn(|c|c.execute_batch("ALTER TABLE execution_wire_requests RENAME TO old_wire; CREATE TABLE execution_wire_requests AS SELECT * FROM old_wire")).unwrap();
            } else {
                db.with_conn(|c| c.pragma_update(None, "user_version", 999))
                    .unwrap();
            }
            assert!(inspect(&p, &i.id).is_err());
        }
    }
    #[test]
    fn scope_and_index_precision_contradictions_are_refused() {
        for sql in ["UPDATE execution_wire_requests SET account='0x2222222222222222222222222222222222222222'",
            "UPDATE execution_wire_requests SET size_decimals=2", "UPDATE execution_wire_requests SET recorded_at_ms=0",
            "DELETE FROM meta_kv WHERE key='trading_account_binding'", "UPDATE meta_kv SET value='invalid' WHERE key='trading_account_binding'"] {
            let d=tempfile::tempdir().unwrap();let p=d.path().join("scope.db");let(db,i)=setup(&p);
            db.with_conn(|c|c.execute_batch(sql)).unwrap();assert!(inspect(&p,&i.id).is_err(),"{sql}");
        }
    }
    #[test]
    fn altered_fields_and_messagepack_cannot_disagree() {
        for (key, val) in [
            ("price", serde_json::json!("102")),
            ("size", serde_json::json!("1.2")),
            ("messagepack_hex", serde_json::json!("00")),
            ("buy", serde_json::json!(false)),
            ("tif", serde_json::json!("Gtc")),
            ("signature", serde_json::json!("untrusted")),
            ("size", serde_json::json!("1.234500")),
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("fields.db");
            let (db, i) = setup(&p);
            change(&db, key, val);
            assert!(inspect(&p, &i.id).is_err(), "{key}");
        }
    }
    #[test]
    fn oversized_selected_payload_and_unknown_intent_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("budget.db");
        let (db, i) = setup(&p);
        assert!(inspect(&p, &uuid::Uuid::new_v4().to_string()).is_err());
        db.with_conn(|c| {
            c.execute(
                "UPDATE execution_wire_requests SET action_json=?1",
                ["x".repeat(4097)],
            )
        })
        .unwrap();
        assert!(inspect(&p, &i.id).is_err());
    }
    #[test]
    fn internally_consistent_action_still_must_match_intent() {
        for (size, buy, reduce) in [(3.0, true, false), (1.0, false, false), (1.0, true, true)] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("intent.db");
            let (db, i) = setup(&p);
            let other = OrderAction::ioc(4, buy, 101.0, size, reduce, &i.id)
                .unwrap()
                .unsigned_provenance()
                .unwrap();
            db.with_conn(|c| {
                c.execute(
                    "UPDATE execution_wire_requests SET action_json=?1",
                    [serde_json::to_string(&other).unwrap()],
                )
            })
            .unwrap();
            assert!(inspect(&p, &i.id).is_err());
        }
    }
}

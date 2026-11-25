//! Bounded read-only inspection of unsigned pre-submission evidence.
pub use crate::signing_preparation::Preparation;
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
    pub signing_preparation_table_available: bool,
    pub signing_preparation: Option<Preparation>,
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
    crate::order_status::client_order_id(intent_id)?;
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
    let (available, record) = load_on(&tx, &intent, &account)?;
    let (preparation_available, preparation) =
        crate::signing_preparation::load_on(&tx, &intent, &account, record.as_ref())?;
    let history = History {
        schema_version: 2,
        read_only: true,
        source_is_live: false,
        submission_or_acceptance_proven: false,
        signed_envelope_available: false,
        account,
        intent_id: intent_id.into(),
        journal_state: state,
        table_available: available,
        record,
        signing_preparation_table_available: preparation_available,
        signing_preparation: preparation,
    };
    tx.commit()?;
    Ok(history)
}

/// Reuse the caller's snapshot; never opens a second connection or transaction.
pub(crate) fn load_on(
    conn: &Connection,
    intent: &OrderIntent,
    account: &str,
) -> Result<(bool, Option<ArchivedAction>)> {
    ensure!(
        intent.venue == "hyperliquid"
            && intent.size.is_finite()
            && intent.size > 0.0
            && crate::ownership::canonical_account(account)? == account,
        "invalid native action scope"
    );
    let intent_id = &intent.id;
    let cloid = crate::order_status::client_order_id(intent_id)?;
    let available = table(conn, "execution_wire_requests")?;
    let mut record = None;
    if available {
        // Require the migration's single-column primary key so a selected-intent
        // read cannot become an unbounded scan over a forged ordinary table.
        let keys: Vec<String> = {
            let mut schema = conn.prepare("SELECT name FROM pragma_table_info('execution_wire_requests') WHERE pk>0 ORDER BY pk")?;
            let keys = schema
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            keys
        };
        ensure!(
            keys == ["intent_id"],
            "unsupported wire history primary key"
        );
        let mut stmt = conn.prepare("SELECT CASE WHEN length(CAST(account AS BLOB))=42 THEN account END,CASE WHEN length(CAST(venue AS BLOB))<=64 THEN venue END,CASE WHEN length(CAST(environment AS BLOB))<=32 THEN environment END,size_decimals,CASE WHEN length(CAST(action_json AS BLOB))<=4096 THEN action_json END,recorded_at_ms,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END FROM execution_wire_requests WHERE intent_id=?1 LIMIT 2")?;
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
                saved_account == account
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
            record = Some(ArchivedAction {
                recorded_at_ms: recorded,
                size_decimals: dp,
                action,
            });
            ensure!(rows.next()?.is_none(), "duplicate archived wire records");
        }
    }
    Ok((available, record))
}

pub(crate) fn validate_native_fill_on(
    conn: &Connection,
    intent: &OrderIntent,
    account: &str,
    facts: &crate::execution_facts::PreciseFill,
) -> Result<()> {
    let record = load_on(conn, intent, account)?.1;
    crate::signing_preparation::load_on(conn, intent, account, record.as_ref())?;
    if let Some(record) = record {
        facts.validate_wire_bounds(
            Decimal::from_str_exact(&record.action.size)?,
            Decimal::from_str_exact(&record.action.price)?,
            record.action.buy,
            record.size_decimals,
        )?;
    }
    // Older acknowledgments without retained action remain historical. This does
    // not fabricate pre-write evidence or certify complete signed provenance.
    Ok(())
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

    fn fill(
        i: &OrderIntent,
        quantity: &str,
        price: &str,
    ) -> (
        crate::execution_facts::PreciseFill,
        crate::core::OrderResult,
    ) {
        let facts = crate::execution_facts::PreciseFill {
            quantity: quantity.into(),
            average_price: price.into(),
            order_id: "42".into(),
        };
        let result = crate::core::OrderResult {
            intent_id: i.id.clone(),
            accepted: true,
            venue_order_id: Some("42".into()),
            filled_size: quantity.parse().unwrap(),
            avg_fill_price: Some(price.parse().unwrap()),
            message: "offline receipt".into(),
        };
        (facts, result)
    }
    fn persist(db: &Storage, i: &OrderIntent, quantity: &str, price: &str) -> Result<()> {
        let (f, r) = fill(i, quantity, price);
        crate::execution_facts::record_receipt(db, "filled", &r, Some(&f))
    }
    fn acknowledged(db: &Storage, i: &OrderIntent) -> Result<Option<(String, Decimal)>> {
        db.with_conn(|c| crate::execution_facts::native_ack_quantity_on(c, &i.id))
    }
    #[test]
    fn native_acknowledgment_within_retained_action_survives_restart() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("ack.db");
        let (db, i) = setup(&p);
        persist(&db, &i, "1.2345", "100").unwrap();
        assert!(acknowledged(&db, &i).unwrap().is_some());
        drop(db);
        let db = Storage::open(&p).unwrap();
        db.migrate().unwrap();
        assert_eq!(
            acknowledged(&db, &i).unwrap().unwrap().1,
            Decimal::from_str_exact("1.2345").unwrap()
        );
        assert!(!inspect(&p, &i.id).unwrap().submission_or_acceptance_proven);
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[test]
    fn action_overfill_limit_and_precision_fail_before_acknowledgment_commit() {
        for (quantity, price) in [
            ("1.5", "100"),
            ("1.2345", "101.000000000000000001"),
            ("1.23451", "100"),
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("bounds.db");
            let (db, i) = setup(&p);
            assert!(persist(&db, &i, quantity, price).is_err());
            let (state, count): (String, i64) = db
                .with_conn(|c| -> rusqlite::Result<(String, i64)> {
                    Ok((
                        c.query_row(
                            "SELECT state FROM execution_intents WHERE intent_id=?1",
                            [&i.id],
                            |r| r.get(0),
                        )?,
                        c.query_row("SELECT count(*) FROM execution_fill_facts", [], |r| {
                            r.get(0)
                        })?,
                    ))
                })
                .unwrap();
            assert_eq!(state, "submitted");
            assert_eq!(count, 0);
            assert!(db.ensure_execution_resolved().is_err());
        }
    }
    #[test]
    fn retained_sell_limit_is_checked_in_the_opposite_direction() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sell.db");
        let (db, mut i) = setup(&p);
        i.id = uuid::Uuid::new_v4().to_string();
        i.side = Side::Short;
        db.reserve_intent(&i).unwrap();
        let a = OrderAction::ioc(4, false, 101.0, 1.2345, false, &i.id).unwrap();
        SubmissionJournal::new(&db, &i)
            .record(ACCOUNT, &a, 4)
            .unwrap();
        assert!(persist(&db, &i, "1.2345", "100").is_err());
        persist(&db, &i, "1.2345", "102").unwrap();
        assert!(acknowledged(&db, &i).unwrap().is_some());
    }
    #[test]
    fn contradictory_retained_action_cannot_fall_back_to_legacy_after_restart_read() {
        for bad_json in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("corrupt.db");
            let (db, i) = setup(&p);
            persist(&db, &i, "1.2345", "100").unwrap();
            let encoded = if bad_json {
                "x".repeat(4097)
            } else {
                serde_json::to_string(
                    &OrderAction::ioc(4, true, 99.0, 1.2345, false, &i.id)
                        .unwrap()
                        .unsigned_provenance()
                        .unwrap(),
                )
                .unwrap()
            };
            db.with_conn(|c| {
                c.execute(
                    "UPDATE execution_wire_requests SET action_json=?1",
                    [encoded],
                )
            })
            .unwrap();
            assert!(acknowledged(&db, &i).is_err());
            assert!(db.ensure_execution_resolved().is_err());
        }
    }
    #[test]
    fn historical_acknowledgments_do_not_invent_missing_action_provenance() {
        for missing_table in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("historical.db");
            let (db, i) = setup(&p);
            db.with_conn(|c| {
                c.execute_batch(if missing_table {
                    "DROP TABLE execution_wire_requests"
                } else {
                    "DELETE FROM execution_wire_requests"
                })
            })
            .unwrap();
            persist(&db, &i, "2", "100").unwrap();
            assert_eq!(acknowledged(&db, &i).unwrap().unwrap().1, Decimal::from(2));
            let h = inspect(&p, &i.id).unwrap();
            assert!(h.record.is_none());
            assert_eq!(h.table_available, !missing_table);
        }
    }
    #[test]
    fn incompatible_action_table_prevents_native_receipt_persistence() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("view.db");
        let (db, i) = setup(&p);
        db.with_conn(|c|c.execute_batch("DROP TABLE execution_wire_requests;CREATE VIEW execution_wire_requests AS SELECT 1")).unwrap();
        assert!(persist(&db, &i, "1.2345", "100").is_err());
        let count: i64 = db
            .with_conn(|c| {
                c.query_row("SELECT count(*) FROM execution_fill_facts", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn action_and_acknowledgment_share_the_callers_wal_snapshot() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("snapshot.db");
        let (db, i) = setup(&p);
        persist(&db, &i, "1.2345", "100").unwrap();
        db.with_conn(|c| -> Result<()> {
            let tx = c.unchecked_transaction()?;
            let _: String =
                tx.query_row("SELECT action_json FROM execution_wire_requests", [], |r| {
                    r.get(0)
                })?;
            let writer = Storage::open(&p)?;
            let changed =
                OrderAction::ioc(4, true, 99.0, 1.2345, false, &i.id)?.unsigned_provenance()?;
            writer.with_conn(|w| {
                w.execute(
                    "UPDATE execution_wire_requests SET action_json=?1",
                    [serde_json::to_string(&changed).unwrap()],
                )
            })?;
            assert!(crate::execution_facts::native_ack_quantity_on(&tx, &i.id)?.is_some());
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        assert!(acknowledged(&db, &i).is_err());
    }

    fn prepare(db: &Storage, i: &OrderIntent) {
        let a = OrderAction::ioc(4, true, 101.0, 1.2345, false, &i.id).unwrap();
        SubmissionJournal::new(db, i)
            .record_signing_preparation("0x2222222222222222222222222222222222222222", 100, &a)
            .unwrap();
    }
    fn change_preparation(db: &Storage, key: &str, value: serde_json::Value) {
        let encoded: String = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT preparation_json FROM execution_signing_preparations",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        let mut p: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        p[key] = value;
        db.with_conn(|c| {
            c.execute(
                "UPDATE execution_signing_preparations SET preparation_json=?1",
                [p.to_string()],
            )
        })
        .unwrap();
    }
    #[test]
    fn protocol_two_preserves_preparation_bytes_times_and_evidence_limits() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("prepared.db");
        let (db, i) = setup(&p);
        prepare(&db, &i);
        let expected = inspect(&p, &i.id).unwrap().signing_preparation.unwrap();
        drop(db);
        let before = std::fs::read(&p).unwrap();
        let h = inspect(&p, &i.id).unwrap();
        assert_eq!(h.schema_version, 2);
        assert!(h.read_only && h.signing_preparation_table_available);
        assert_eq!(h.signing_preparation.unwrap(), expected);
        assert!(
            !h.source_is_live && !h.submission_or_acceptance_proven && !h.signed_envelope_available
        );
        assert_eq!(std::fs::read(&p).unwrap(), before);
    }
    #[test]
    fn absent_preparation_table_and_record_are_distinct_without_creation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("historical.db");
        let (db, i) = setup(&p);
        let h = inspect(&p, &i.id).unwrap();
        assert!(h.signing_preparation_table_available && h.signing_preparation.is_none());
        db.with_conn(|c| c.execute_batch("DROP TABLE execution_signing_preparations"))
            .unwrap();
        let h = inspect(&p, &i.id).unwrap();
        assert!(!h.signing_preparation_table_available && h.signing_preparation.is_none());
    }
    #[test]
    fn preparation_scope_domain_phase_and_unknown_fields_are_refused() {
        for (key, value) in [
            (
                "account",
                serde_json::json!("0x3333333333333333333333333333333333333333"),
            ),
            (
                "signer",
                serde_json::json!("0x3333333333333333333333333333333333333333"),
            ),
            ("nonce", serde_json::json!("0100")),
            ("domain_chain_id", serde_json::json!(1)),
            ("agent_source", serde_json::json!("b")),
            (
                "vault_address",
                serde_json::json!("0x3333333333333333333333333333333333333333"),
            ),
            ("expires_after_ms", serde_json::json!(100)),
            ("phase", serde_json::json!("accepted")),
            ("prepared_at_ms", serde_json::json!(0)),
            ("signature", serde_json::json!("untrusted")),
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("scope.db");
            let (db, i) = setup(&p);
            prepare(&db, &i);
            change_preparation(&db, key, value);
            assert!(inspect(&p, &i.id).is_err(), "{key}");
        }
    }
    #[test]
    fn preparation_hashes_must_reproduce_retained_action_and_nonce() {
        for key in ["connection_hash", "agent_digest"] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("hash.db");
            let (db, i) = setup(&p);
            prepare(&db, &i);
            change_preparation(&db, key, serde_json::json!("00".repeat(32)));
            assert!(inspect(&p, &i.id).is_err());
        }
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("nonce.db");
        let (db, i) = setup(&p);
        prepare(&db, &i);
        change_preparation(&db, "nonce", serde_json::json!("101"));
        db.with_conn(|c| c.execute("UPDATE execution_signing_preparations SET nonce='101'", []))
            .unwrap();
        assert!(inspect(&p, &i.id).is_err());
    }
    #[test]
    fn oversized_and_index_contradictory_preparations_are_refused() {
        for sql in ["UPDATE execution_signing_preparations SET signer='0x3333333333333333333333333333333333333333'",
            "UPDATE execution_signing_preparations SET nonce='101'", "UPDATE execution_signing_preparations SET preparation_json=printf('%02049d',1)"] {
            let d=tempfile::tempdir().unwrap();let p=d.path().join("index.db");let(db,i)=setup(&p);prepare(&db,&i);
            db.with_conn(|c|c.execute_batch(sql)).unwrap();assert!(inspect(&p,&i.id).is_err());
        }
    }
    #[test]
    fn orphan_preparation_cannot_become_unknown_unsigned_history() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("orphan.db");
        let (db, i) = setup(&p);
        prepare(&db, &i);
        // Synthetic corruption only: ordinary production writes keep foreign keys.
        db.with_conn(|c|c.execute_batch("PRAGMA foreign_keys=OFF;DELETE FROM execution_wire_requests;PRAGMA foreign_keys=ON")).unwrap();
        assert!(inspect(&p, &i.id).is_err());
    }
    #[test]
    fn preparation_views_and_unindexed_tables_are_refused() {
        for view in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("schema.db");
            let (db, i) = setup(&p);
            prepare(&db, &i);
            db.with_conn(|c|c.execute_batch(if view {"DROP TABLE execution_signing_preparations;CREATE VIEW execution_signing_preparations AS SELECT 1"} else {"ALTER TABLE execution_signing_preparations RENAME TO old_preparations;CREATE TABLE execution_signing_preparations AS SELECT * FROM old_preparations"})).unwrap();
            assert!(inspect(&p, &i.id).is_err());
        }
    }
    #[test]
    fn native_acknowledgment_reads_refuse_corrupt_retained_preparation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("ack-prepared.db");
        let (db, i) = setup(&p);
        prepare(&db, &i);
        persist(&db, &i, "1.2345", "100").unwrap();
        assert!(acknowledged(&db, &i).unwrap().is_some());
        change_preparation(&db, "agent_digest", serde_json::json!("00".repeat(32)));
        assert!(acknowledged(&db, &i).is_err());
        assert!(db.ensure_execution_resolved().is_err());
    }
}

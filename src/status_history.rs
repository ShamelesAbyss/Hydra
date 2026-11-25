//! Bounded offline inspection. This path cannot migrate, fetch or resolve orders.
use crate::{core::OrderIntent, order_status::Observation};
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::{path::Path, time::Duration};
const WINDOW: i64 = 10_000;
#[derive(Clone, Debug, Serialize)]
pub struct History {
    pub schema_version: u32,
    pub read_only: bool,
    pub source_is_live: bool,
    pub exchange_history_complete: bool,
    pub account: String,
    pub intent_id: String,
    pub journal_state: String,
    pub table_available: bool,
    pub scan_window_rows: i64,
    pub window_capped: bool,
    pub records_capped: bool,
    pub observations: Vec<ArchivedObservation>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ArchivedObservation {
    pub observation: Observation,
    pub recorded_at_ms: i64,
}
pub(crate) fn table(conn: &Connection, name: &str) -> Result<bool> {
    let schema: Option<(String,String)> = conn
        .query_row(
            "SELECT type,CASE WHEN length(CAST(sql AS BLOB))<=65536 THEN sql END FROM sqlite_master WHERE name=?1",
            [name],
            |r| Ok((r.get(0)?,r.get(1)?)),
        )
        .optional()?;
    if let Some((kind, sql)) = schema {
        ensure!(
            kind == "table"
                && sql
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE TABLE"),
            "unsupported nonordinary evidence table"
        );
        Ok(true)
    } else {
        Ok(false)
    }
}
/// Scan at most the newest 10,000 global observation row IDs and return at most
/// 64 matching records. Capping is explicit; absence never proves no venue order.
pub fn inspect(path: &Path, intent_id: &str, limit: usize) -> Result<History> {
    ensure!(
        (1..=64).contains(&limit),
        "status history limit must be in 1..=64"
    );
    crate::order_status::client_order_id(intent_id)?;
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("opening existing history read-only")?;
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
        "local intent/account history unavailable"
    );
    let account: String = tx
        .query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB))=42 THEN value END FROM meta_kv WHERE key='trading_account_binding'",
            [],
            |r| r.get(0),
        )
        .context("history account binding unavailable")?;
    ensure!(
        crate::ownership::canonical_account(&account)? == account,
        "invalid history account binding"
    );
    let (state,encoded):(String,Option<String>)=tx.query_row("SELECT CASE WHEN length(CAST(state AS BLOB))<=64 THEN state END,CASE WHEN length(CAST(intent_json AS BLOB))<=32768 THEN intent_json END FROM execution_intents WHERE intent_id=?1",[intent_id],|r|Ok((r.get(0)?,r.get(1)?))).context("local intent unavailable")?;
    ensure!(
        !state.is_empty() && state.len() <= 64 && state.bytes().all(|b| b.is_ascii_graphic()),
        "unsupported journal state"
    );
    let intent: OrderIntent =
        serde_json::from_str(&encoded.context("local intent missing or oversized")?)?;
    ensure!(
        intent.id == intent_id && intent.venue == "hyperliquid",
        "local intent identity disagrees"
    );
    let available = table(&tx, "execution_status_observations")?;
    let mut history = History {
        schema_version: 1,
        read_only: true,
        source_is_live: false,
        exchange_history_complete: false,
        account,
        intent_id: intent_id.into(),
        journal_state: state,
        table_available: available,
        scan_window_rows: WINDOW,
        window_capped: false,
        records_capped: false,
        observations: Vec::new(),
    };
    if available {
        let highest: Option<i64> = tx.query_row(
            "SELECT MAX(rowid) FROM execution_status_observations",
            [],
            |r| r.get(0),
        )?;
        let highest = highest.unwrap_or(0);
        ensure!(highest >= 0, "unsupported observation sequence");
        let floor = highest.saturating_sub(WINDOW).max(0);
        history.window_capped = floor > 0;
        let mut statement=tx.prepare("SELECT CASE WHEN length(CAST(observation_id AS BLOB))=36 THEN observation_id END,CASE WHEN length(CAST(account AS BLOB))=42 THEN account END,CASE WHEN length(CAST(requested_cloid AS BLOB))=34 THEN requested_cloid END,acquired_started_ms,acquired_finished_ms,recorded_at_ms,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(observation_json AS BLOB))<=65536 THEN observation_json END FROM execution_status_observations WHERE rowid>?1 AND intent_id=?2 ORDER BY rowid DESC LIMIT ?3")?;
        let mut rows = statement.query(rusqlite::params![floor, intent_id, (limit + 1) as i64])?;
        while let Some(row) = rows.next()? {
            if history.observations.len() == limit {
                history.records_capped = true;
                break;
            }
            let id: String = row.get(0)?;
            let account: String = row.get(1)?;
            let cloid: String = row.get(2)?;
            let started: i64 = row.get(3)?;
            let finished: i64 = row.get(4)?;
            let recorded: i64 = row.get(5)?;
            let source: String = row.get(6)?;
            let encoded: Option<String> = row.get(7)?;
            let obs: Observation = serde_json::from_str(
                &encoded.context("archived observation missing or oversized")?,
            )?;
            ensure!(
                obs.id == id
                    && obs.account == account
                    && obs.account == history.account
                    && obs.intent_id == intent_id
                    && obs.requested_cloid == cloid
                    && obs.acquired_started_ms == started
                    && obs.acquired_finished_ms == finished
                    && recorded >= finished
                    && source == "native_hyperliquid_info_order_status",
                "archived observation provenance disagrees"
            );
            // Historical validation preserves original times. It does not mark
            // archived evidence fresh enough for any later financial decision.
            obs.validate(&intent, &history.account, finished)?;
            history.observations.push(ArchivedObservation {
                observation: obs,
                recorded_at_ms: recorded,
            });
        }
    }
    tx.commit()?;
    Ok(history)
}

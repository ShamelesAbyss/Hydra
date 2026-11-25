//! Immutable entry metadata survives stop ratcheting and process restart.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{core::OpenPosition, storage::Storage};

const MAX_CONTEXT_BYTES: usize = 32 * 1024;
const MAX_CONTEXT_DEPTH: usize = 8;
const MAX_CONTEXT_NODES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryStateOrigin {
    Recorded,
    LegacyUnknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryState {
    pub position_id: i64,
    pub origin: EntryStateOrigin,
    pub entry_intent_id: Option<String>,
    pub initial_r: Option<f64>,
    pub context_json: Option<String>,
}

pub fn migrate(storage: &Storage) -> Result<()> {
    storage.with_conn(|conn| -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS position_entry_state (
                position_id INTEGER PRIMARY KEY REFERENCES positions(id),
                entry_intent_id TEXT NOT NULL UNIQUE REFERENCES execution_intents(intent_id),
                initial_r REAL,
                context_json TEXT,
                schema_version INTEGER NOT NULL CHECK(schema_version=1)
            );",
        )?;
        Ok(())
    })
}

/// Store the new position and its original entry facts in one transaction.
/// Duplicate entry intents fail without adding another position.
pub fn insert_position_with_context(
    storage: &Storage,
    position: &OpenPosition,
    entry_intent_id: &str,
    initial_r: Option<f64>,
    context_json: Option<&str>,
) -> Result<i64> {
    validate_position(position)?;
    validate_intent_id(entry_intent_id)?;
    validate_initial_r(initial_r)?;
    if let Some(context) = context_json {
        validate_context(context)?;
    }
    migrate(storage)?;
    storage.with_conn(|conn| -> Result<i64> {
        let transaction = conn.unchecked_transaction()?;
        let intent_json: String = transaction.query_row(
            "SELECT intent_json FROM execution_intents WHERE intent_id=?1",
            [entry_intent_id],
            |row| row.get(0),
        )?;
        let intent: crate::core::OrderIntent = serde_json::from_str(&intent_json)?;
        ensure!(
            !intent.reduce_only
                && intent.venue == position.venue
                && intent.symbol == position.symbol
                && intent.side.as_str() == position.side
                && intent.size.is_finite()
                && intent.size >= position.size,
            "position entry does not match its durable intent"
        );
        transaction.execute(
            "INSERT INTO positions (venue,symbol,side,size,entry_price,stop_price,take_profit,
                                    opened_at,strategy,trailing_high)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                position.venue,
                position.symbol,
                position.side,
                position.size,
                position.entry_price,
                position.stop_price,
                position.take_profit,
                position.opened_at.to_rfc3339(),
                position.strategy,
                position.trailing_high,
            ],
        )?;
        let position_id = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO position_entry_state
                (position_id,entry_intent_id,initial_r,context_json,schema_version)
             VALUES (?1,?2,?3,?4,1)",
            params![position_id, entry_intent_id, initial_r, context_json],
        )?;
        transaction.commit()?;
        Ok(position_id)
    })
}

/// Existing positions with no persisted entry record retain explicitly unknown
/// metadata. Their current stop is never substituted for original risk distance.
pub fn load_entry_state(storage: &Storage, position_id: i64) -> Result<EntryState> {
    ensure!(position_id > 0, "invalid position identity");
    storage.with_conn(|conn| -> Result<EntryState> {
        let transaction = conn.unchecked_transaction()?;
        let position_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM positions WHERE id=?1)",
            [position_id],
            |row| row.get(0),
        )?;
        ensure!(position_exists, "position does not exist");
        let table_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='position_entry_state')",
            [],
            |row| row.get(0),
        )?;
        let record = if table_exists {
            transaction.query_row(
                "SELECT entry_intent_id,initial_r,context_json,schema_version
                 FROM position_entry_state WHERE position_id=?1",
                [position_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<String>>(2)?, row.get::<_, i64>(3)?)),
            ).optional()?
        } else {
            None
        };
        let state = match record {
            Some((intent_id, initial_r, context_json, schema_version)) => {
                ensure!(schema_version == 1, "unsupported position entry metadata schema");
                validate_intent_id(&intent_id)?;
                validate_initial_r(initial_r)?;
                if let Some(context) = &context_json {
                    validate_context(context)?;
                }
                EntryState { position_id, origin: EntryStateOrigin::Recorded,
                    entry_intent_id: Some(intent_id), initial_r, context_json }
            }
            None => EntryState { position_id, origin: EntryStateOrigin::LegacyUnknown,
                entry_intent_id: None, initial_r: None, context_json: None },
        };
        transaction.commit()?;
        Ok(state)
    })
}

/// Recover the crash window after atomic position persistence and before the
/// entry journal was marked applied. Only known filled receipts are eligible.
pub fn recover_applied_entries(
    storage: &Storage,
    account: &str,
    venue: &str,
    remote: &[crate::adapters::VenuePosition],
) -> Result<Vec<String>> {
    let account = crate::ownership::canonical_account(account)?;
    crate::account_positions::validate(&storage.list_open_positions()?, remote, venue)?;
    storage.with_conn(|conn| -> Result<Vec<String>> {
        let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
        let binding: Option<String> = tx.query_row(
            "SELECT value FROM meta_kv WHERE key='trading_account_binding'", [], |row| row.get(0)).optional()?;
        ensure!(binding.as_deref() == Some(account.as_str()), "entry recovery requires the database's bound account");
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='position_entry_state')",
            [], |row| row.get(0))?;
        if !exists { tx.commit()?; return Ok(Vec::new()); }
        let rows = {
            let mut query = tx.prepare(
                "SELECT e.intent_id,e.intent_json,e.result_json,p.symbol,p.side,p.size,p.entry_price,p.id
                 FROM execution_intents e JOIN position_entry_state m ON m.entry_intent_id=e.intent_id
                 JOIN positions p ON p.id=m.position_id
                 WHERE e.state='filled' AND p.closed_at IS NULL AND p.venue=?1 LIMIT 1025")?;
            let rows = query.query_map([venue],|row| Ok((
                row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,
                row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,f64>(5)?,
                row.get::<_,f64>(6)?,row.get::<_,i64>(7)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        ensure!(rows.len() <= 1024, "entry recovery resource limit exceeded");
        let mut recovered = Vec::new();
        for (id,intent_json,result_json,symbol,side,size,price,position_id) in rows {
            let intent: crate::core::OrderIntent = serde_json::from_str(&intent_json)?;
            let result: crate::core::OrderResult = serde_json::from_str(&result_json)?;
            ensure!(!intent.reduce_only && intent.id == id && intent.venue == venue
                && intent.symbol == symbol && intent.side.as_str() == side
                && result.intent_id == id && result.accepted && result.filled_size == size
                && result.avg_fill_price == Some(price) && size.is_finite() && size > 0.0,
                "persisted entry receipt does not match its position");
            let observed = remote.iter().find(|p| p.symbol == symbol)
                .context("entry position unavailable in account evidence")?;
            ensure!((observed.size.abs()-size).abs() <= 1e-10
                && (observed.entry_price-price).abs() <= 1e-8,
                "entry recovery account evidence changed");
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute("UPDATE execution_intents SET state='applied',updated_at=?1 WHERE intent_id=?2 AND state='filled'",
                params![now,id])?;
            tx.execute("INSERT INTO meta_kv(key,value,updated_at) VALUES(?1,?2,?3)",
                params![format!("entry_recovery:{id}"),format!("confirmed position {position_id}; account {account}"),now])?;
            recovered.push(id);
        }
        tx.commit()?;
        Ok(recovered)
    })
}

fn validate_intent_id(intent_id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(intent_id)?.to_string() == intent_id,
        "entry intent UUID must use canonical lowercase form"
    );
    Ok(())
}

fn validate_initial_r(initial_r: Option<f64>) -> Result<()> {
    ensure!(
        initial_r.is_none_or(|value| value.is_finite() && value > 0.0),
        "original entry risk distance must be positive and finite or unknown"
    );
    Ok(())
}

fn validate_position(position: &OpenPosition) -> Result<()> {
    ensure!(
        position.side == "LONG" || position.side == "SHORT",
        "invalid position side"
    );
    for (name, value) in [("venue", &position.venue), ("symbol", &position.symbol)] {
        ensure!(
            !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
            "invalid position {name}"
        );
    }
    if let Some(strategy) = &position.strategy {
        ensure!(
            !strategy.is_empty()
                && strategy.len() <= 128
                && !strategy.chars().any(char::is_control),
            "invalid position strategy"
        );
    }
    for value in [
        Some(position.size),
        Some(position.entry_price),
        position.stop_price,
        position.take_profit,
        position.trailing_high,
    ]
    .into_iter()
    .flatten()
    {
        ensure!(
            value.is_finite() && value > 0.0,
            "invalid position quantity or price"
        );
    }
    Ok(())
}

fn validate_context(context: &str) -> Result<()> {
    ensure!(
        context.len() <= MAX_CONTEXT_BYTES,
        "entry context exceeds byte budget"
    );
    let value: Value = serde_json::from_str(context).context("invalid entry context JSON")?;
    ensure!(value.is_object(), "entry context must be a JSON object");
    let mut nodes = 0;
    validate_json_value(&value, 0, &mut nodes)
}

fn validate_json_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes += 1;
    ensure!(
        depth <= MAX_CONTEXT_DEPTH && *nodes <= MAX_CONTEXT_NODES,
        "entry context exceeds shape budget"
    );
    match value {
        Value::Object(fields) => {
            ensure!(
                fields.len() <= 64,
                "entry context has too many object fields"
            );
            for (key, value) in fields {
                ensure!(
                    !key.is_empty() && key.len() <= 128 && !key.chars().any(char::is_control),
                    "invalid entry context field"
                );
                validate_json_value(value, depth + 1, nodes)?;
            }
        }
        Value::Array(items) => {
            ensure!(items.len() <= 256, "entry context array exceeds budget");
            for value in items {
                validate_json_value(value, depth + 1, nodes)?;
            }
        }
        Value::String(value) => ensure!(value.len() <= 4096, "entry context string exceeds budget"),
        _ => {}
    }
    Ok(())
}

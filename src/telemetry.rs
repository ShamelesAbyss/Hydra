//! Versioned, bounded, read-only local telemetry. No migration or wallet access.
//! All data in one response comes from one SQLite read transaction. Recorded
//! history is not a claim that the engine is online or that an account is armed.
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags, Row};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const SCHEMA_VERSION: u32 = 1;
const COUNT_BOUND: usize = 10_000;
const TABLES: &[&str] = &[
    "positions",
    "signals",
    "orders",
    "execution_intents",
    "confirmed_reductions",
    "closed_trades",
    "trade_context",
    "equity_snapshots",
    "sessions",
    "errors",
    "ledger_accounts",
    "ledger_fills",
    "ledger_funding",
    "aegis_accounts",
    "aegis_reservations",
    "aegis_events",
];

#[derive(Clone, Debug, Serialize)]
pub struct BoundedCount {
    pub value: usize,
    pub capped: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct TableAvailability {
    pub name: String,
    pub available: bool,
    pub rows: Option<BoundedCount>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionView {
    pub symbol: String,
    pub venue: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
    pub stop_price: Option<f64>,
    pub strategy: Option<String>,
    pub opened_at: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct IntentView {
    pub id: String,
    pub symbol: String,
    pub venue: String,
    pub state: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct SignalView {
    pub at: String,
    pub symbol: String,
    pub strategy: String,
    pub side: String,
    pub confidence: f64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct EventView {
    pub at: String,
    pub subsystem: String,
    pub classification: String,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct EquityPoint {
    pub venue: String,
    pub recorded_equity: f64,
    pub at: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct RiskAccountView {
    pub account: String,
    pub high_water_equity: String,
    pub breaker_reason: Option<String>,
    pub snapshot_revision: Option<i64>,
    pub active_reservations: BoundedCount,
    pub reserved_notional: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct FillView {
    pub scope: String,
    pub fill_id: String,
    pub order_id: String,
    pub symbol: String,
    pub timestamp_ms: i64,
    pub quantity: String,
    pub price: String,
    pub fee_amount: Option<String>,
    pub fee_currency: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SessionView {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub configured_live: bool,
    pub symbols: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TelemetrySnapshot {
    pub schema_version: u32,
    pub application_version: String,
    pub captured_at: String,
    pub database_path: String,
    pub consistency: String,
    pub row_limit: usize,
    pub tables: Vec<TableAvailability>,
    pub latest_session: Option<SessionView>,
    pub positions: Vec<PositionView>,
    pub intents: Vec<IntentView>,
    pub signals: Vec<SignalView>,
    pub events: Vec<EventView>,
    pub equity: Vec<EquityPoint>,
    pub risk_accounts: Vec<RiskAccountView>,
    pub fills: Vec<FillView>,
    pub diagnostics: Vec<String>,
}
impl TelemetrySnapshot {
    pub fn table(&self, name: &str) -> Option<&TableAvailability> {
        self.tables.iter().find(|t| t.name == name)
    }
    pub fn count(&self, name: &str) -> Option<&BoundedCount> {
        self.table(name).and_then(|t| t.rows.as_ref())
    }
    pub fn has_table(&self, name: &str) -> bool {
        self.table(name).is_some_and(|t| t.available)
    }
}

pub fn load_snapshot(path: impl AsRef<Path>, row_limit: usize) -> Result<TelemetrySnapshot> {
    ensure!(
        (1..=2_000).contains(&row_limit),
        "telemetry row limit must be in 1..=2000"
    );
    let path = path.as_ref();
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("opening read-only Hydra database; the viewer never creates a database")?;
    conn.busy_timeout(Duration::from_millis(100))?;
    conn.pragma_update(None, "query_only", true)?;
    let tx = conn.unchecked_transaction()?;
    let mut tables = Vec::new();
    for &table in TABLES {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |r| r.get(0),
        )?;
        let rows = if exists {
            // Table identifiers come exclusively from this fixed source list.
            let count: i64 = tx.query_row(
                &format!(
                    "SELECT COUNT(*) FROM (SELECT 1 FROM {table} LIMIT {})",
                    COUNT_BOUND + 1
                ),
                [],
                |r| r.get(0),
            )?;
            let count = usize::try_from(count)?;
            Some(BoundedCount {
                value: count.min(COUNT_BOUND),
                capped: count > COUNT_BOUND,
            })
        } else {
            None
        };
        tables.push(TableAvailability {
            name: table.into(),
            available: exists,
            rows,
        });
    }
    let exists = |name: &str| tables.iter().any(|t| t.name == name && t.available);
    let mut diagnostics = Vec::new();
    let positions = if exists("positions") {
        read(&tx,&format!("SELECT symbol,venue,side,size,entry_price,stop_price,strategy,opened_at FROM positions WHERE closed_at IS NULL ORDER BY id DESC LIMIT {row_limit}"),|r|Ok(PositionView{
        symbol:r.get(0)?,venue:r.get(1)?,side:r.get(2)?,size:r.get(3)?,entry_price:r.get(4)?,stop_price:r.get(5)?,strategy:r.get(6)?,opened_at:r.get(7)?}))?
    } else {
        Vec::new()
    };
    for p in &positions {
        ensure!(
            p.size.is_finite()
                && p.size > 0.0
                && p.entry_price.is_finite()
                && p.entry_price > 0.0
                && p.stop_price.is_none_or(|p| p.is_finite() && p > 0.0),
            "invalid recorded position; telemetry unavailable"
        );
    }
    let intents = if exists("execution_intents") {
        read(&tx,&format!("SELECT intent_id,symbol,venue,state,updated_at FROM execution_intents ORDER BY updated_at DESC,intent_id DESC LIMIT {row_limit}"),|r|Ok(IntentView{id:r.get(0)?,symbol:r.get(1)?,venue:r.get(2)?,state:r.get(3)?,updated_at:r.get(4)?}))?
    } else {
        Vec::new()
    };
    let signals = if exists("signals") {
        read(&tx,&format!("SELECT created_at,symbol,strategy,side,confidence,COALESCE(reason,'') FROM signals ORDER BY id DESC LIMIT {row_limit}"),|r|Ok(SignalView{at:r.get(0)?,symbol:r.get(1)?,strategy:r.get(2)?,side:r.get(3)?,confidence:r.get(4)?,reason:redact_diagnostic(&r.get::<_,String>(5)?)}))?
    } else {
        Vec::new()
    };
    let mut events = if exists("errors") {
        read(
            &tx,
            &format!("SELECT at,kind,message FROM errors ORDER BY id DESC LIMIT {row_limit}"),
            |r| {
                Ok(EventView {
                    at: r.get(0)?,
                    subsystem: "BLACK BOX".into(),
                    classification: r.get(1)?,
                    detail: redact_diagnostic(&r.get::<_, String>(2)?),
                })
            },
        )?
    } else {
        Vec::new()
    };
    if exists("aegis_events") {
        events.extend(read(
            &tx,
            &format!(
                "SELECT at_ms,kind,reason FROM aegis_events ORDER BY id DESC LIMIT {row_limit}"
            ),
            |r| {
                let stamp: i64 = r.get(0)?;
                Ok(EventView {
                    at: chrono::DateTime::from_timestamp_millis(stamp)
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "invalid timestamp".into()),
                    subsystem: "AEGIS".into(),
                    classification: r.get(1)?,
                    detail: redact_diagnostic(&r.get::<_, String>(2)?),
                })
            },
        )?);
    }
    events.sort_by(|a, b| b.at.cmp(&a.at));
    events.truncate(row_limit);
    let mut equity = if exists("equity_snapshots") {
        read(&tx,&format!("SELECT venue,equity_usd,at FROM equity_snapshots ORDER BY id DESC LIMIT {row_limit}"),|r|Ok(EquityPoint{venue:r.get(0)?,recorded_equity:r.get(1)?,at:r.get(2)?}))?
    } else {
        Vec::new()
    };
    ensure!(
        equity
            .iter()
            .all(|e| e.recorded_equity.is_finite() && e.recorded_equity >= 0.0),
        "invalid recorded equity"
    );
    equity.reverse();
    let risk_accounts = if exists("aegis_accounts") {
        let mut accounts=read(&tx,&format!("SELECT account,high_water_equity,breaker_reason,snapshot_revision FROM aegis_accounts ORDER BY account LIMIT {row_limit}"),|r|Ok(RiskAccountView{account:r.get(0)?,high_water_equity:r.get(1)?,breaker_reason:r.get::<_,Option<String>>(2)?.map(|s|redact_diagnostic(&s)),snapshot_revision:r.get(3)?,active_reservations:BoundedCount{value:0,capped:false},reserved_notional:None}))?;
        if exists("aegis_reservations") {
            for account in &mut accounts {
                let mut query=tx.prepare(&format!("SELECT notional FROM aegis_reservations WHERE account=?1 AND state='active' LIMIT {}",COUNT_BOUND+1))?;
                let rows = query
                    .query_map([&account.account], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                account.active_reservations = BoundedCount {
                    value: rows.len().min(COUNT_BOUND),
                    capped: rows.len() > COUNT_BOUND,
                };
                if rows.len() <= COUNT_BOUND {
                    let mut total = rust_decimal::Decimal::ZERO;
                    for value in rows {
                        let n = rust_decimal::Decimal::from_str_exact(&value)?;
                        ensure!(n > rust_decimal::Decimal::ZERO, "invalid reserved notional");
                        total = total.checked_add(n).context("risk telemetry overflow")?;
                    }
                    account.reserved_notional = Some(total.to_string());
                }
            }
        }
        accounts
    } else {
        Vec::new()
    };
    let fills = if exists("ledger_fills") {
        read(&tx,&format!("SELECT scope,fill_id,order_id,symbol,timestamp_ms,payload_json FROM ledger_fills ORDER BY timestamp_ms DESC,fill_id DESC LIMIT {row_limit}"),|r|{
        let payload:String=r.get(5)?;
        let json:serde_json::Value=serde_json::from_str(&payload).map_err(|e|rusqlite::Error::FromSqlConversionFailure(5,rusqlite::types::Type::Text,Box::new(e)))?;
        Ok(FillView{scope:r.get(0)?,fill_id:r.get(1)?,order_id:r.get(2)?,symbol:r.get(3)?,timestamp_ms:r.get(4)?,quantity:scalar(json.get("quantity")),price:scalar(json.get("price")),fee_amount:json.get("fee").filter(|f|f.is_object()).map(|f|scalar(f.get("amount"))),fee_currency:json.get("fee").and_then(|f|f.get("token")).and_then(|v|v.as_str()).map(str::to_owned)})
    })?
    } else {
        Vec::new()
    };
    let mut sessions = if exists("sessions") {
        read(&tx,"SELECT id,started_at,ended_at,live_trading,symbols FROM sessions ORDER BY id DESC LIMIT 1",|r|Ok(SessionView{id:r.get(0)?,started_at:r.get(1)?,ended_at:r.get(2)?,configured_live:r.get::<_,i64>(3)?!=0,symbols:r.get(4)?}))?
    } else {
        Vec::new()
    };
    if !exists("positions") && !exists("execution_intents") {
        diagnostics.push("No Hydra execution schema found in this database.".into());
    }
    diagnostics.push(format!(
        "Each row view is limited to {row_limit} records; counts are capped at {COUNT_BOUND}."
    ));
    diagnostics.push("Recorded snapshots do not establish current engine health, live authorization, or exchange connectivity.".into());
    diagnostics.push(
        "Financial environments/accounts are never summed together; legacy PnL is not certified."
            .into(),
    );
    tx.commit()?;
    Ok(TelemetrySnapshot {
        schema_version: SCHEMA_VERSION,
        application_version: env!("CARGO_PKG_VERSION").into(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        database_path: path.display().to_string(),
        consistency: "sqlite_read_transaction".into(),
        row_limit,
        tables,
        latest_session: sessions.pop(),
        positions,
        intents,
        signals,
        events,
        equity,
        risk_accounts,
        fills,
        diagnostics,
    })
}

fn read<T>(
    conn: &Connection,
    sql: &str,
    mut map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| map(row))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
fn scalar(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(s)) => s.chars().take(128).collect(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => "unavailable".into(),
    }
}

/// Deliberately conservative diagnostic redaction. Raw config/intent JSON is
/// never selected; suspected secret-bearing diagnostic lines are removed whole.
pub fn redact_diagnostic(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let sensitive = [
        "private_key",
        "private key",
        "secret",
        "signature",
        "authorization",
        "bearer ",
        "password",
        "api_key",
        "access_token",
        "ghp_",
        "github_pat_",
    ];
    if sensitive.iter().any(|word| lower.contains(word))
        || value
            .as_bytes()
            .windows(64)
            .any(|part| part.iter().all(u8::is_ascii_hexdigit))
    {
        return "[redacted sensitive diagnostic]".into();
    }
    value.chars().take(1_024).collect()
}

pub fn state_counts(snapshot: &TelemetrySnapshot) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for intent in &snapshot.intents {
        *counts.entry(intent.state.clone()).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    #[test]
    fn missing_path_does_not_create_a_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.db");
        assert!(load_snapshot(&path, 100).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn optional_schemas_are_unavailable_not_reported_as_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hydra.db");
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        let before = db
            .with_conn(|c| c.query_row("PRAGMA schema_version", [], |r| r.get::<_, i64>(0)))
            .unwrap();
        let snapshot = load_snapshot(&path, 10).unwrap();
        assert!(snapshot.has_table("positions"));
        assert!(!snapshot.has_table("aegis_accounts"));
        assert_eq!(snapshot.count("signals").unwrap().value, 0);
        assert!(snapshot.count("ledger_fills").is_none());
        assert_eq!(
            before,
            db.with_conn(|c| c.query_row("PRAGMA schema_version", [], |r| r.get::<_, i64>(0)))
                .unwrap()
        );
    }
    #[test]
    fn raw_configuration_is_never_exported_and_diagnostics_are_redacted() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hydra.db");
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        db.with_conn(|c|c.execute("INSERT INTO sessions (started_at,venues,symbols,live_trading,config_snapshot) VALUES ('time','hyperliquid','ETH',0,'PRIVATE_CONFIGURATION_CANARY')",[])).unwrap();
        db.with_conn(|c|c.execute("INSERT INTO errors (session_id,kind,message,at) VALUES (1,'fixture','private_key=DO_NOT_EXPORT','time')",[])).unwrap();
        let json = serde_json::to_string(&load_snapshot(&path, 10).unwrap()).unwrap();
        assert!(!json.contains("PRIVATE_CONFIGURATION_CANARY"));
        assert!(!json.contains("DO_NOT_EXPORT"));
        assert!(json.contains("redacted sensitive"));
    }
    #[test]
    fn row_limits_bound_event_history() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hydra.db");
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        for i in 0..20 {
            db.with_conn(|c|c.execute("INSERT INTO errors (session_id,kind,message,at) VALUES (1,'fixture',?1,'time')",[format!("event {i}")])).unwrap();
        }
        let snapshot = load_snapshot(&path, 3).unwrap();
        assert_eq!(snapshot.events.len(), 3);
        assert_eq!(snapshot.count("errors").unwrap().value, 20);
        assert!(load_snapshot(&path, 0).is_err());
        assert!(load_snapshot(&path, 2001).is_err());
    }
    #[test]
    fn secret_patterns_and_unicode_never_leak_or_panic() {
        assert_eq!(
            redact_diagnostic("Authorization: Bearer example"),
            "[redacted sensitive diagnostic]"
        );
        assert_eq!(
            redact_diagnostic(&"a".repeat(64)),
            "[redacted sensitive diagnostic]"
        );
        assert_eq!(
            redact_diagnostic("ETH partial fill confirmed"),
            "ETH partial fill confirmed"
        );
        assert_eq!(redact_diagnostic(&"μ".repeat(2000)).chars().count(), 1024);
    }
}

//! Durable, isolated SQLite storage for Project Ascendancy paper-trading sessions.
//!
//! Safety & structural isolation:
//! - Stored in `paper_sessions.db`, completely separate from any genuine live financial
//!   `hydra.db` database (`positions`, `fills`, `ledger_fills`).
//! - Separates `public_paper`, `offline_demo`, and `historical_replay` sessions so one
//!   session type cannot overwrite another session's portfolio or metrics.
//! - Persists critical Synapse decisions, risk reservations, simulated orders, closed trades,
//!   equity history, breaker state, and restart recovery snapshots in atomic SQLite transactions.

use crate::paper_pipeline::{
    CycleSummary, PaperClosedTrade, PaperLedger, PaperPosition, PipelineRecoveryCheckpoint,
    SynapsePipeline, SynapsePipelineConfig,
};
use crate::synapse_bus::{
    DataProvenance, HeadId, SynapseEvent, SynapsePayload, SYNAPSE_SCHEMA_VERSION,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const PAPER_STRATEGY_VERSION: &str = "synapse-ema-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSessionKind {
    PublicPaper,
    OfflineDemo,
    HistoricalReplay,
}

impl PaperSessionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PublicPaper => "public_paper",
            Self::OfflineDemo => "offline_demo",
            Self::HistoricalReplay => "historical_replay",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::PublicPaper => "PUBLIC MARKET PAPER",
            Self::OfflineDemo => "OFFLINE SYNTHETIC DEMO",
            Self::HistoricalReplay => "HISTORICAL REPLAY",
        }
    }

    pub fn from_db(s: &str) -> Result<Self> {
        match s {
            "public_paper" => Ok(Self::PublicPaper),
            "offline_demo" => Ok(Self::OfflineDemo),
            "historical_replay" => Ok(Self::HistoricalReplay),
            other => bail!("unknown paper session kind '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSessionStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Archived,
    Faulted,
}

impl PaperSessionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Archived => "archived",
            Self::Faulted => "faulted",
        }
    }

    pub fn from_db(s: &str) -> Result<Self> {
        match s {
            "created" => Ok(Self::Created),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "stopped" => Ok(Self::Stopped),
            "archived" => Ok(Self::Archived),
            "faulted" => Ok(Self::Faulted),
            other => bail!("unknown paper session status '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSessionSummary {
    pub session_id: String,
    pub kind: PaperSessionKind,
    pub label: String,
    pub status: PaperSessionStatus,
    pub strategy_version: String,
    pub config: SynapsePipelineConfig,
    pub ledger: PaperLedger,
    pub watchlist: Vec<String>,
    pub cycle_count: u64,
    pub mtm_equity_usd: Decimal,
    pub owner_pid: Option<u32>,
    pub heartbeat_at: Option<String>,
    pub fault_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct PaperStore {
    inner: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl PaperStore {
    pub fn default_root() -> Result<PathBuf> {
        #[cfg(windows)]
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA required for paper session storage")?;
        #[cfg(not(windows))]
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .context("XDG_STATE_HOME or HOME required for paper session storage")?;
        Ok(base.join("Hydra").join("paper"))
    }

    pub fn default_db_path() -> Result<PathBuf> {
        Ok(Self::default_root()?.join("paper_sessions.db"))
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&Self::default_db_path()?)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create paper dir {}", parent.display()))?;
        }
        let conn = Connection::open(path_ref)
            .with_context(|| format!("open paper SQLite at {}", path_ref.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Self::migrate(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
            path: path_ref.to_path_buf(),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.path
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS paper_sessions (
                session_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL CHECK(kind IN ('public_paper','offline_demo','historical_replay')),
                label TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('created','running','paused','stopped','archived','faulted')),
                strategy_version TEXT NOT NULL,
                config_json TEXT NOT NULL,
                ledger_json TEXT NOT NULL,
                watchlist_json TEXT NOT NULL,
                cycle_count INTEGER NOT NULL DEFAULT 0,
                starting_equity_usd TEXT NOT NULL,
                mtm_equity_usd TEXT NOT NULL,
                realized_net_pnl_usd TEXT NOT NULL,
                total_fees_usd TEXT NOT NULL,
                total_funding_usd TEXT NOT NULL,
                max_drawdown_ratio TEXT NOT NULL,
                breaker_reason TEXT,
                state_checkpoint_json TEXT,
                fault_reason TEXT,
                owner_pid INTEGER,
                heartbeat_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paper_sessions_kind_status
                ON paper_sessions(kind, status);

            CREATE TABLE IF NOT EXISTS paper_positions (
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                symbol TEXT NOT NULL,
                owner_strategy TEXT NOT NULL,
                side TEXT NOT NULL,
                entry_price TEXT NOT NULL,
                qty TEXT NOT NULL,
                notional TEXT NOT NULL,
                stop_price TEXT NOT NULL,
                take_profit_price TEXT NOT NULL,
                entry_fee_usd TEXT NOT NULL,
                opened_at_ms INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                PRIMARY KEY(session_id, symbol)
            );

            CREATE TABLE IF NOT EXISTS paper_closed_trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                symbol TEXT NOT NULL,
                owner_strategy TEXT NOT NULL,
                side TEXT NOT NULL,
                entry_price TEXT NOT NULL,
                exit_price TEXT NOT NULL,
                qty TEXT NOT NULL,
                gross_pnl_usd TEXT NOT NULL,
                fees_usd TEXT NOT NULL,
                realized_net_pnl_usd TEXT NOT NULL,
                exit_reason TEXT NOT NULL,
                closed_at_ms INTEGER NOT NULL,
                normalized_r REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paper_closed_session ON paper_closed_trades(session_id);

            CREATE TABLE IF NOT EXISTS paper_orders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                intent_id TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                symbol TEXT NOT NULL,
                action TEXT NOT NULL,
                price TEXT NOT NULL,
                qty TEXT NOT NULL,
                notional TEXT NOT NULL,
                fee_usd TEXT NOT NULL,
                realized_pnl_usd TEXT,
                provenance TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paper_orders_session ON paper_orders(session_id);

            CREATE TABLE IF NOT EXISTS paper_equity_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                cycle INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                cash_equity_usd TEXT NOT NULL,
                mtm_equity_usd TEXT NOT NULL,
                unrealized_pnl_usd TEXT NOT NULL,
                drawdown_ratio TEXT NOT NULL,
                recorded_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paper_equity_session ON paper_equity_history(session_id);

            CREATE TABLE IF NOT EXISTS paper_risk_reservations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                correlation_id TEXT NOT NULL,
                candidate_id TEXT NOT NULL,
                symbol TEXT NOT NULL,
                approved INTEGER NOT NULL,
                reason TEXT NOT NULL,
                exposure_ratio TEXT NOT NULL,
                breaker_latched INTEGER NOT NULL,
                decided_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS paper_synapse_events (
                event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                event_id INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                head TEXT NOT NULL,
                provenance TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                critical INTEGER NOT NULL,
                timestamp_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_paper_events_session ON paper_synapse_events(session_id, event_seq DESC);

            CREATE TABLE IF NOT EXISTS paper_observations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES paper_sessions(session_id) ON DELETE CASCADE,
                cycle INTEGER NOT NULL,
                provenance TEXT NOT NULL,
                observation_json TEXT NOT NULL,
                observed_at_ms INTEGER NOT NULL
            );
        "#,
        )?;
        let mut has_checkpoint = false;
        let mut has_fault_reason = false;
        {
            let mut stmt = conn.prepare("PRAGMA table_info(paper_sessions)")?;
            let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for col in cols {
                match col?.as_str() {
                    "state_checkpoint_json" => has_checkpoint = true,
                    "fault_reason" => has_fault_reason = true,
                    _ => {}
                }
            }
        }
        if !has_checkpoint {
            conn.execute(
                "ALTER TABLE paper_sessions ADD COLUMN state_checkpoint_json TEXT",
                [],
            )?;
        }
        if !has_fault_reason {
            conn.execute(
                "ALTER TABLE paper_sessions ADD COLUMN fault_reason TEXT",
                [],
            )?;
        }
        Ok(())
    }

    fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.inner.lock().expect("paper SQLite lock poisoned");
        f(&guard)
    }

    pub fn create_session(
        &self,
        kind: PaperSessionKind,
        label: &str,
        config: SynapsePipelineConfig,
    ) -> Result<PaperSessionSummary> {
        let trimmed = label.trim();
        ensure!(
            !trimmed.is_empty() && trimmed.len() <= 96,
            "paper session label must be 1..=96 chars"
        );
        let ledger = PaperLedger::new(config.starting_equity)?;
        let session_id = format!(
            "{}-{}",
            kind.as_str(),
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        );
        let now = Utc::now().to_rfc3339();
        let cfg_json = serde_json::to_string(&config)?;
        let ledger_json = serde_json::to_string(&ledger)?;

        self.with_conn(|conn| -> Result<()> {
            conn.execute(
                r#"INSERT INTO paper_sessions(
                    session_id, kind, label, status, strategy_version,
                    config_json, ledger_json, watchlist_json, cycle_count,
                    starting_equity_usd, mtm_equity_usd, realized_net_pnl_usd,
                    total_fees_usd, total_funding_usd, max_drawdown_ratio,
                    breaker_reason, owner_pid, heartbeat_at, created_at, updated_at
                ) VALUES(?1,?2,?3,'created',?4,?5,?6,'[]',0,?7,?7,'0','0','0','0',NULL,NULL,NULL,?8,?8)"#,
                params![
                    session_id,
                    kind.as_str(),
                    trimmed,
                    PAPER_STRATEGY_VERSION,
                    cfg_json,
                    ledger_json,
                    config.starting_equity.to_string(),
                    now
                ],
            )?;
            Ok(())
        })?;

        self.get_session(&session_id)?
            .ok_or_else(|| anyhow!("newly created paper session missing"))
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<PaperSessionSummary>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT session_id, kind, label, status, strategy_version,
                          config_json, ledger_json, watchlist_json, cycle_count,
                          mtm_equity_usd, owner_pid, heartbeat_at, created_at, updated_at, fault_reason
                   FROM paper_sessions WHERE session_id=?1"#,
                params![session_id],
                row_to_summary,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    /// Returns the most recent non-archived session of `kind`, creating one if none exists.
    pub fn get_or_create_session(
        &self,
        kind: PaperSessionKind,
        default_label: &str,
        default_config: SynapsePipelineConfig,
    ) -> Result<PaperSessionSummary> {
        let existing = self.with_conn(|conn| {
            conn.query_row(
                r#"SELECT session_id, kind, label, status, strategy_version,
                          config_json, ledger_json, watchlist_json, cycle_count,
                          mtm_equity_usd, owner_pid, heartbeat_at, created_at, updated_at, fault_reason
                   FROM paper_sessions
                   WHERE kind=?1 AND status != 'archived'
                   ORDER BY updated_at DESC LIMIT 1"#,
                params![kind.as_str()],
                row_to_summary,
            )
            .optional()
            .map_err(anyhow::Error::from)
        })?;
        if let Some(s) = existing {
            Ok(s)
        } else {
            self.create_session(kind, default_label, default_config)
        }
    }

    pub fn list_sessions(
        &self,
        kind_filter: Option<PaperSessionKind>,
    ) -> Result<Vec<PaperSessionSummary>> {
        self.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(k) = kind_filter {
                let mut stmt = conn.prepare(
                    r#"SELECT session_id, kind, label, status, strategy_version,
                              config_json, ledger_json, watchlist_json, cycle_count,
                              mtm_equity_usd, owner_pid, heartbeat_at, created_at, updated_at, fault_reason
                       FROM paper_sessions WHERE kind=?1 ORDER BY updated_at DESC LIMIT 64"#,
                )?;
                let rows = stmt.query_map(params![k.as_str()], row_to_summary)?;
                for r in rows {
                    out.push(r?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"SELECT session_id, kind, label, status, strategy_version,
                              config_json, ledger_json, watchlist_json, cycle_count,
                              mtm_equity_usd, owner_pid, heartbeat_at, created_at, updated_at, fault_reason
                       FROM paper_sessions ORDER BY updated_at DESC LIMIT 64"#,
                )?;
                let rows = stmt.query_map([], row_to_summary)?;
                for r in rows {
                    out.push(r?);
                }
            }
            Ok(out)
        })
    }

    pub fn load_pipeline(&self, session_id: &str) -> Result<SynapsePipeline> {
        let summary = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow!("paper session '{session_id}' not found"))?;
        let mut pipe = SynapsePipeline::new(summary.config.clone())?;
        pipe.ledger = summary.ledger;
        pipe.watchlist = summary.watchlist;
        pipe.cycle_count = summary.cycle_count;
        pipe.bus.set_session_id(Some(session_id.to_string()));

        let (checkpoint_raw, max_event_id) =
            self.with_conn(|conn| -> Result<(Option<String>, u64)> {
                let cp: Option<String> = conn
                    .query_row(
                        "SELECT state_checkpoint_json FROM paper_sessions WHERE session_id=?1",
                        params![session_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                let max_id: i64 = conn.query_row(
                "SELECT COALESCE(MAX(event_id), 0) FROM paper_synapse_events WHERE session_id=?1",
                params![session_id],
                |r| r.get(0),
            )?;
                Ok((cp, max_id.max(0) as u64))
            })?;

        let mut history = self.load_recent_events(session_id, 512)?;
        history.reverse();
        if let Some(raw) = checkpoint_raw {
            let cp: PipelineRecoveryCheckpoint = serde_json::from_str(&raw).with_context(|| {
                format!("decode recovery checkpoint for session '{session_id}'")
            })?;
            pipe.restore_checkpoint(cp, history);
        } else {
            pipe.bus
                .restore_checkpoint(max_event_id.saturating_add(1), 0, history);
        }
        Ok(pipe)
    }

    pub fn load_recent_events(&self, session_id: &str, limit: usize) -> Result<Vec<SynapseEvent>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT event_id, correlation_id, head, provenance, payload_json, timestamp_ms
                   FROM paper_synapse_events
                   WHERE session_id=?1
                   ORDER BY event_seq DESC
                   LIMIT ?2"#,
            )?;
            let rows = stmt.query_map(params![session_id, limit as i64], |row| {
                let event_id: i64 = row.get(0)?;
                let correlation_id: String = row.get(1)?;
                let head_str: String = row.get(2)?;
                let prov_str: String = row.get(3)?;
                let payload_json: String = row.get(4)?;
                let timestamp_ms: i64 = row.get(5)?;
                Ok((
                    event_id,
                    correlation_id,
                    head_str,
                    prov_str,
                    payload_json,
                    timestamp_ms,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (eid, corr, head_str, prov_str, payload_json, ts_ms) = row?;
                let head: HeadId =
                    serde_json::from_str(&format!("\"{head_str}\"")).unwrap_or(HeadId::Nexus);
                let provenance: DataProvenance = serde_json::from_str(&format!("\"{prov_str}\""))
                    .unwrap_or(DataProvenance::OperationalFault);
                let payload: SynapsePayload = serde_json::from_str(&payload_json)?;
                out.push(SynapseEvent {
                    schema_version: SYNAPSE_SCHEMA_VERSION,
                    event_id: eid.max(0) as u64,
                    session_id: Some(session_id.to_string()),
                    correlation_id: corr,
                    timestamp_ms: ts_ms,
                    head,
                    provenance,
                    payload,
                });
            }
            Ok(out)
        })
    }

    pub fn set_status(
        &self,
        session_id: &str,
        status: PaperSessionStatus,
        owner_pid: Option<u32>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| -> Result<()> {
            let changed = conn.execute(
                "UPDATE paper_sessions SET status=?1, owner_pid=?2, heartbeat_at=CASE WHEN ?2 IS NOT NULL THEN ?3 ELSE heartbeat_at END, fault_reason=CASE WHEN ?1 IN ('running','created') THEN NULL ELSE fault_reason END, updated_at=?3 WHERE session_id=?4",
                params![status.as_str(), owner_pid.map(|p| p as i64), now, session_id],
            )?;
            ensure!(changed == 1, "paper session '{session_id}' not found");
            Ok(())
        })
    }

    pub fn record_heartbeat(&self, session_id: &str, pid: u32) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| -> Result<()> {
            let changed = conn.execute(
                "UPDATE paper_sessions SET owner_pid=?1, heartbeat_at=?2, updated_at=?2 WHERE session_id=?3",
                params![pid as i64, now, session_id],
            )?;
            ensure!(changed == 1, "paper session '{session_id}' not found");
            Ok(())
        })
    }

    pub fn record_fault(&self, session_id: &str, reason: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let trimmed = reason.trim();
        let msg = if trimmed.is_empty() {
            "unspecified worker fault"
        } else {
            trimmed
        };
        self.with_conn(|conn| -> Result<()> {
            let changed = conn.execute(
                "UPDATE paper_sessions SET status='faulted', owner_pid=NULL, fault_reason=?1, updated_at=?2 WHERE session_id=?3",
                params![msg, now, session_id],
            )?;
            ensure!(changed == 1, "paper session '{session_id}' not found");
            Ok(())
        })
    }

    pub fn archive_session_guarded(&self, lock_root: &Path, session_id: &str) -> Result<()> {
        crate::paper_service::PaperSessionLock::assert_unowned(lock_root, session_id)?;
        self.archive_session(session_id)
    }

    pub fn archive_session(&self, session_id: &str) -> Result<()> {
        let summary = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow!("paper session '{session_id}' not found"))?;
        ensure!(
            !matches!(
                summary.status,
                PaperSessionStatus::Running | PaperSessionStatus::Paused
            ),
            "cannot archive a {} paper session; stop it first",
            summary.status.as_str()
        );
        self.set_status(session_id, PaperSessionStatus::Archived, None)
    }

    pub fn reset_session_confirmed_guarded(
        &self,
        lock_root: &Path,
        session_id: &str,
        confirmation: &str,
    ) -> Result<PaperSessionSummary> {
        crate::paper_service::PaperSessionLock::assert_unowned(lock_root, session_id)?;
        self.reset_session_confirmed(session_id, confirmation)
    }

    /// Destructive reset of a paper session's simulated state, requiring explicit confirmation string
    /// `CONFIRM_RESET:<session_id>`. Preserves `kind`, `label`, and starting configuration.
    pub fn reset_session_confirmed(
        &self,
        session_id: &str,
        confirmation: &str,
    ) -> Result<PaperSessionSummary> {
        let expected = format!("CONFIRM_RESET:{session_id}");
        ensure!(
            confirmation == expected,
            "destructive paper reset refused without exact confirmation '{expected}'"
        );
        let summary = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow!("paper session '{session_id}' not found"))?;
        ensure!(
            !matches!(
                summary.status,
                PaperSessionStatus::Running | PaperSessionStatus::Paused
            ),
            "cannot reset a {} paper session; stop it first",
            summary.status.as_str()
        );
        let fresh_ledger = PaperLedger::new(summary.config.starting_equity)?;
        let fresh_ledger_json = serde_json::to_string(&fresh_ledger)?;
        let now = Utc::now().to_rfc3339();

        self.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            tx.execute(
                "DELETE FROM paper_positions WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_closed_trades WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_orders WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_equity_history WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_risk_reservations WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_synapse_events WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                "DELETE FROM paper_observations WHERE session_id=?1",
                params![session_id],
            )?;
            tx.execute(
                r#"UPDATE paper_sessions SET
                    status='created',
                    ledger_json=?1,
                    watchlist_json='[]',
                    cycle_count=0,
                    mtm_equity_usd=?2,
                    realized_net_pnl_usd='0',
                    total_fees_usd='0',
                    total_funding_usd='0',
                    max_drawdown_ratio='0',
                    breaker_reason=NULL,
                    state_checkpoint_json=NULL,
                    fault_reason=NULL,
                    owner_pid=NULL,
                    heartbeat_at=NULL,
                    updated_at=?3
                   WHERE session_id=?4"#,
                params![
                    fresh_ledger_json,
                    summary.config.starting_equity.to_string(),
                    now,
                    session_id
                ],
            )?;
            tx.commit()?;
            Ok(())
        })?;

        self.get_session(session_id)?
            .ok_or_else(|| anyhow!("reset session missing"))
    }

    /// Commits one completed pipeline cycle atomically into SQLite (`IMMEDIATE` transaction).
    #[allow(clippy::too_many_arguments)]
    pub fn commit_cycle(
        &self,
        session_id: &str,
        pipeline: &SynapsePipeline,
        summary: &CycleSummary,
        prev_closed_count: usize,
        new_events: &[SynapseEvent],
        marks: &BTreeMap<String, Decimal>,
        observation_provenance: DataProvenance,
        observation_payload_json: Option<&str>,
    ) -> Result<PaperSessionSummary> {
        let now = Utc::now().to_rfc3339();
        let now_ms = Utc::now().timestamp_millis();
        let ledger_json = serde_json::to_string(&pipeline.ledger)?;
        let wl_json = serde_json::to_string(&pipeline.watchlist)?;
        let checkpoint_json = serde_json::to_string(&pipeline.export_checkpoint())?;
        let unrealized = pipeline.ledger.unrealized_pnl(marks);
        let drawdown = pipeline.ledger.current_drawdown_ratio(marks);

        self.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            tx.execute(
                r#"UPDATE paper_sessions SET
                    ledger_json=?1,
                    watchlist_json=?2,
                    cycle_count=?3,
                    mtm_equity_usd=?4,
                    realized_net_pnl_usd=?5,
                    total_fees_usd=?6,
                    total_funding_usd=?7,
                    max_drawdown_ratio=?8,
                    breaker_reason=?9,
                    state_checkpoint_json=?10,
                    fault_reason=NULL,
                    updated_at=?11
                   WHERE session_id=?12"#,
                params![
                    ledger_json,
                    wl_json,
                    pipeline.cycle_count as i64,
                    summary.mtm_equity_usd.to_string(),
                    pipeline.ledger.realized_net_pnl_usd.to_string(),
                    pipeline.ledger.total_fees_usd.to_string(),
                    pipeline.ledger.total_funding_usd.to_string(),
                    pipeline.ledger.max_drawdown_ratio.to_string(),
                    pipeline.ledger.breaker_reason,
                    checkpoint_json,
                    now,
                    session_id,
                ],
            )?;

            tx.execute(
                "DELETE FROM paper_positions WHERE session_id=?1",
                params![session_id],
            )?;
            for (sym, pos) in &pipeline.ledger.open_positions {
                insert_position_tx(&tx, session_id, sym, pos)?;
            }

            for trade in pipeline.ledger.closed_trades.iter().skip(prev_closed_count) {
                insert_closed_trade_tx(&tx, session_id, trade)?;
            }

            tx.execute(
                r#"INSERT INTO paper_equity_history(
                    session_id, cycle, correlation_id, cash_equity_usd,
                    mtm_equity_usd, unrealized_pnl_usd, drawdown_ratio, recorded_at_ms
                ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"#,
                params![
                    session_id,
                    summary.cycle as i64,
                    summary.correlation_id,
                    pipeline.ledger.cash_equity.to_string(),
                    summary.mtm_equity_usd.to_string(),
                    unrealized.to_string(),
                    drawdown.to_string(),
                    now_ms,
                ],
            )?;

            for ev in new_events {
                let head_s = serde_json::to_value(ev.head)?
                    .as_str()
                    .unwrap_or("nexus")
                    .to_string();
                let prov_s = serde_json::to_value(ev.provenance)?
                    .as_str()
                    .unwrap_or("operational_fault")
                    .to_string();
                let payload_json = serde_json::to_string(&ev.payload)?;
                tx.execute(
                    r#"INSERT INTO paper_synapse_events(
                        session_id, event_id, correlation_id, head, provenance,
                        payload_json, critical, timestamp_ms
                    ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"#,
                    params![
                        session_id,
                        ev.event_id as i64,
                        ev.correlation_id,
                        head_s,
                        prov_s,
                        payload_json,
                        i64::from(ev.is_critical()),
                        ev.timestamp_ms,
                    ],
                )?;

                match &ev.payload {
                    SynapsePayload::AegisDecision {
                        candidate_id,
                        symbol,
                        approved,
                        reason,
                        exposure_ratio,
                        breaker_latched,
                    } => {
                        tx.execute(
                            r#"INSERT INTO paper_risk_reservations(
                                session_id, correlation_id, candidate_id, symbol,
                                approved, reason, exposure_ratio, breaker_latched, decided_at_ms
                            ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
                            params![
                                session_id,
                                ev.correlation_id,
                                candidate_id,
                                symbol,
                                i64::from(*approved),
                                reason,
                                exposure_ratio,
                                i64::from(*breaker_latched),
                                ev.timestamp_ms,
                            ],
                        )?;
                    }
                    SynapsePayload::NexusPaperExecution {
                        intent_id,
                        symbol,
                        action,
                        price,
                        qty,
                        notional,
                        fee_usd,
                        realized_pnl_usd,
                        ..
                    } => {
                        tx.execute(
                            r#"INSERT INTO paper_orders(
                                session_id, intent_id, correlation_id, symbol, action,
                                price, qty, notional, fee_usd, realized_pnl_usd, provenance, timestamp_ms
                            ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
                            params![
                                session_id,
                                intent_id,
                                ev.correlation_id,
                                symbol,
                                action,
                                price,
                                qty,
                                notional,
                                fee_usd,
                                realized_pnl_usd,
                                prov_s,
                                ev.timestamp_ms,
                            ],
                        )?;
                    }
                    _ => {}
                }
            }

            if let Some(obs_json) = observation_payload_json {
                let prov_s = serde_json::to_value(observation_provenance)?
                    .as_str()
                    .unwrap_or("authoritative_exchange")
                    .to_string();
                tx.execute(
                    r#"INSERT INTO paper_observations(
                        session_id, cycle, provenance, observation_json, observed_at_ms
                    ) VALUES(?1,?2,?3,?4,?5)"#,
                    params![
                        session_id,
                        summary.cycle as i64,
                        prov_s,
                        obs_json,
                        now_ms,
                    ],
                )?;
            }

            tx.commit()?;
            Ok(())
        })?;

        self.get_session(session_id)?
            .ok_or_else(|| anyhow!("paper session missing after commit"))
    }
}

fn insert_position_tx(
    tx: &Transaction<'_>,
    session_id: &str,
    sym: &str,
    pos: &PaperPosition,
) -> Result<()> {
    tx.execute(
        r#"INSERT INTO paper_positions(
            session_id, symbol, owner_strategy, side, entry_price, qty,
            notional, stop_price, take_profit_price, entry_fee_usd, opened_at_ms, correlation_id
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
        params![
            session_id,
            sym,
            pos.owner_strategy,
            pos.side.as_str(),
            pos.entry_price.to_string(),
            pos.qty.to_string(),
            pos.notional.to_string(),
            pos.stop_price.to_string(),
            pos.take_profit_price.to_string(),
            pos.entry_fee_usd.to_string(),
            pos.opened_at_ms,
            pos.correlation_id,
        ],
    )?;
    Ok(())
}

fn insert_closed_trade_tx(
    tx: &Transaction<'_>,
    session_id: &str,
    trade: &PaperClosedTrade,
) -> Result<()> {
    tx.execute(
        r#"INSERT INTO paper_closed_trades(
            session_id, symbol, owner_strategy, side, entry_price, exit_price, qty,
            gross_pnl_usd, fees_usd, realized_net_pnl_usd, exit_reason, closed_at_ms, normalized_r
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
        params![
            session_id,
            trade.symbol,
            trade.owner_strategy,
            trade.side.as_str(),
            trade.entry_price.to_string(),
            trade.exit_price.to_string(),
            trade.qty.to_string(),
            trade.gross_pnl_usd.to_string(),
            trade.fees_usd.to_string(),
            trade.realized_net_pnl_usd.to_string(),
            trade.exit_reason,
            trade.closed_at_ms,
            trade.normalized_r,
        ],
    )?;
    Ok(())
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<PaperSessionSummary> {
    let session_id: String = row.get(0)?;
    let kind_s: String = row.get(1)?;
    let label: String = row.get(2)?;
    let status_s: String = row.get(3)?;
    let strategy_version: String = row.get(4)?;
    let config_json: String = row.get(5)?;
    let ledger_json: String = row.get(6)?;
    let wl_json: String = row.get(7)?;
    let cycle_count: i64 = row.get(8)?;
    let mtm_s: String = row.get(9)?;
    let owner_pid: Option<i64> = row.get(10)?;
    let heartbeat_at: Option<String> = row.get(11)?;
    let created_at: String = row.get(12)?;
    let updated_at: String = row.get(13)?;
    let fault_reason: Option<String> = row.get(14)?;

    let to_sql_err = |e: anyhow::Error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::other(e.to_string())),
        )
    };
    let kind = PaperSessionKind::from_db(&kind_s).map_err(to_sql_err)?;
    let status = PaperSessionStatus::from_db(&status_s).map_err(to_sql_err)?;
    let config: SynapsePipelineConfig =
        serde_json::from_str(&config_json).map_err(|e| to_sql_err(e.into()))?;
    let ledger: PaperLedger =
        serde_json::from_str(&ledger_json).map_err(|e| to_sql_err(e.into()))?;
    let watchlist: Vec<String> = serde_json::from_str(&wl_json).unwrap_or_default();
    let mtm_equity_usd: Decimal = mtm_s.parse().unwrap_or(ledger.cash_equity);

    Ok(PaperSessionSummary {
        session_id,
        kind,
        label,
        status,
        strategy_version,
        config,
        ledger,
        watchlist,
        cycle_count: cycle_count.max(0) as u64,
        mtm_equity_usd,
        owner_pid: owner_pid.and_then(|p| u32::try_from(p).ok()),
        heartbeat_at,
        fault_reason,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_paper_sessions_recover_across_restart_and_separate_session_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("paper_sessions.db");
        let store = PaperStore::open(&db_path).unwrap();

        let demo_sess = store
            .create_session(
                PaperSessionKind::OfflineDemo,
                "Offline Synthetic Demo",
                SynapsePipelineConfig::default(),
            )
            .unwrap();
        let pub_sess = store
            .create_session(
                PaperSessionKind::PublicPaper,
                "Public Stream Paper",
                SynapsePipelineConfig::default(),
            )
            .unwrap();
        assert_ne!(demo_sess.session_id, pub_sess.session_id);

        let mut pipe = store.load_pipeline(&demo_sess.session_id).unwrap();
        let mut last_summary = None;
        for _ in 0..4 {
            last_summary = Some(pipe.run_deterministic_demo_step().unwrap());
        }
        let events: Vec<_> = pipe.bus.all_chronological().cloned().collect();
        let marks = BTreeMap::from([
            ("BTC".to_string(), Decimal::from(66_000u32)),
            ("ETH".to_string(), Decimal::from(3_300u32)),
            ("SOL".to_string(), Decimal::from(140u32)),
        ]);
        store
            .commit_cycle(
                &demo_sess.session_id,
                &pipe,
                last_summary.as_ref().unwrap(),
                0,
                &events,
                &marks,
                DataProvenance::SyntheticTestFixture,
                Some("{}"),
            )
            .unwrap();

        // Reopen store from scratch -> demo state persists while public session stays untouched.
        drop(store);
        let reopened = PaperStore::open(&db_path).unwrap();
        let recovered_demo = reopened.load_pipeline(&demo_sess.session_id).unwrap();
        let recovered_pub = reopened.get_session(&pub_sess.session_id).unwrap().unwrap();
        assert_eq!(recovered_demo.cycle_count, 4);
        assert_eq!(recovered_demo.ledger.cash_equity, pipe.ledger.cash_equity);
        assert_eq!(recovered_pub.cycle_count, 0);
        assert!(
            reopened
                .load_recent_events(&demo_sess.session_id, 50)
                .unwrap()
                .len()
                >= 4
        );

        // Destructive reset requires exact confirmation token.
        assert!(reopened
            .reset_session_confirmed(&demo_sess.session_id, "wrong")
            .is_err());
        let reset = reopened
            .reset_session_confirmed(
                &demo_sess.session_id,
                &format!("CONFIRM_RESET:{}", demo_sess.session_id),
            )
            .unwrap();
        assert_eq!(reset.cycle_count, 0);
        assert!(reset.ledger.open_positions.is_empty());
    }

    #[test]
    fn crash_recovery_matches_uninterrupted_run_and_never_collides_event_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("paper_recovery.db");
        let store = PaperStore::open(&db_path).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::OfflineDemo,
                "Recovery Equivalence",
                SynapsePipelineConfig::default(),
            )
            .unwrap();

        let mut uninterrupted = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
        uninterrupted
            .bus
            .set_session_id(Some(sess.session_id.clone()));
        for _ in 0..8 {
            uninterrupted.run_deterministic_demo_step().unwrap();
        }

        let mut split = store.load_pipeline(&sess.session_id).unwrap();
        for _ in 0..4 {
            let before_ev = split.bus.total_published();
            let prev_closed = split.ledger.closed_trades.len();
            let sum = split.run_deterministic_demo_step().unwrap();
            let new_evs: Vec<_> = split
                .bus
                .all_chronological()
                .filter(|e| e.event_id > before_ev)
                .cloned()
                .collect();
            let mut marks = BTreeMap::new();
            for (s, p) in &split.ledger.open_positions {
                marks.insert(s.clone(), p.entry_price);
            }
            store
                .commit_cycle(
                    &sess.session_id,
                    &split,
                    &sum,
                    prev_closed,
                    &new_evs,
                    &marks,
                    DataProvenance::SyntheticTestFixture,
                    Some("{\"cycle\":true}"),
                )
                .unwrap();
        }
        let cp1_next_id = split.bus.next_event_id();
        drop(store);

        let reopened = PaperStore::open(&db_path).unwrap();
        let mut recovered = reopened.load_pipeline(&sess.session_id).unwrap();
        assert_eq!(recovered.bus.next_event_id(), cp1_next_id);
        for _ in 0..4 {
            let before_ev = recovered.bus.total_published();
            let prev_closed = recovered.ledger.closed_trades.len();
            let sum = recovered.run_deterministic_demo_step().unwrap();
            let new_evs: Vec<_> = recovered
                .bus
                .all_chronological()
                .filter(|e| e.event_id > before_ev)
                .cloned()
                .collect();
            let mut marks = BTreeMap::new();
            for (s, p) in &recovered.ledger.open_positions {
                marks.insert(s.clone(), p.entry_price);
            }
            reopened
                .commit_cycle(
                    &sess.session_id,
                    &recovered,
                    &sum,
                    prev_closed,
                    &new_evs,
                    &marks,
                    DataProvenance::SyntheticTestFixture,
                    Some("{\"cycle\":true}"),
                )
                .unwrap();
        }

        assert_eq!(recovered.cycle_count, uninterrupted.cycle_count);
        assert_eq!(recovered.watchlist, uninterrupted.watchlist);
        assert_eq!(recovered.ledger, uninterrupted.ledger);
        assert_eq!(
            recovered.bus.next_event_id(),
            uninterrupted.bus.next_event_id()
        );
        assert_eq!(
            recovered.last_funding_applied_ms,
            uninterrupted.last_funding_applied_ms
        );
        assert_eq!(
            recovered.simulated_samples.len(),
            uninterrupted.simulated_samples.len()
        );

        let db_events = reopened.load_recent_events(&sess.session_id, 500).unwrap();
        let unique_ids: std::collections::BTreeSet<u64> =
            db_events.iter().map(|e| e.event_id).collect();
        assert_eq!(unique_ids.len(), db_events.len());
    }

    #[test]
    fn commit_cycle_sql_abort_rolls_back_all_tables_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("paper_rollback.db");
        let store = PaperStore::open(&db_path).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::OfflineDemo,
                "Rollback Test",
                SynapsePipelineConfig::default(),
            )
            .unwrap();

        let mut pipe = store.load_pipeline(&sess.session_id).unwrap();
        let mut last_sum = None;
        let mut all_evs = Vec::new();
        // Run enough demo steps to emit NexusPaperExecution orders.
        for _ in 0..4 {
            let before = pipe.bus.total_published();
            last_sum = Some(pipe.run_deterministic_demo_step().unwrap());
            all_evs.extend(
                pipe.bus
                    .all_chronological()
                    .filter(|e| e.event_id > before)
                    .cloned(),
            );
        }

        // Inject trigger aborting on paper_orders insert after earlier updates/inserts in tx.
        store.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER fail_orders_insert BEFORE INSERT ON paper_orders BEGIN SELECT RAISE(ABORT, 'injected order fault'); END;",
            )
            .unwrap();
        });

        let err = store.commit_cycle(
            &sess.session_id,
            &pipe,
            last_sum.as_ref().unwrap(),
            0,
            &all_evs,
            &BTreeMap::new(),
            DataProvenance::SyntheticTestFixture,
            Some("{\"aborted\":true}"),
        );
        assert!(err.is_err());

        // Every table must roll back completely to pre-transaction cycle 0.
        let after = store.get_session(&sess.session_id).unwrap().unwrap();
        assert_eq!(after.cycle_count, 0);
        assert!(store
            .load_recent_events(&sess.session_id, 50)
            .unwrap()
            .is_empty());
        let (eq_cnt, obs_cnt, pos_cnt): (i64, i64, i64) = store.with_conn(|conn| {
            let e: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM paper_equity_history WHERE session_id=?1",
                    params![&sess.session_id],
                    |r| r.get(0),
                )
                .unwrap();
            let o: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM paper_observations WHERE session_id=?1",
                    params![&sess.session_id],
                    |r| r.get(0),
                )
                .unwrap();
            let p: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM paper_positions WHERE session_id=?1",
                    params![&sess.session_id],
                    |r| r.get(0),
                )
                .unwrap();
            (e, o, p)
        });
        assert_eq!((eq_cnt, obs_cnt, pos_cnt), (0, 0, 0));
    }

    #[test]
    fn archive_and_reset_refuse_running_and_paused_sessions_and_record_fault_works() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PaperStore::open(tmp.path().join("paper_guards.db")).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::PublicPaper,
                "Guards Test",
                SynapsePipelineConfig::default(),
            )
            .unwrap();
        let token = format!("CONFIRM_RESET:{}", sess.session_id);

        for status in [PaperSessionStatus::Running, PaperSessionStatus::Paused] {
            store
                .set_status(&sess.session_id, status, Some(4242))
                .unwrap();
            assert!(store.archive_session(&sess.session_id).is_err());
            assert!(store
                .reset_session_confirmed(&sess.session_id, &token)
                .is_err());
        }

        store
            .record_fault(&sess.session_id, "SQLite commit aborted")
            .unwrap();
        let faulted = store.get_session(&sess.session_id).unwrap().unwrap();
        assert_eq!(faulted.status, PaperSessionStatus::Faulted);
        assert_eq!(
            faulted.fault_reason.as_deref(),
            Some("SQLite commit aborted")
        );
        assert!(faulted.owner_pid.is_none());
    }
}

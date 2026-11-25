//! SQLite persistent storage (sessions, signals, orders, positions, errors, metrics).

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::core::{OpenPosition, OrderIntent, OrderResult, Signal};

pub mod schema;

/// Thread-safe handle. rusqlite::Connection is !Sync, so we wrap it in a Mutex.
#[derive(Clone)]
pub struct Storage {
    inner: Arc<Mutex<Connection>>,
    path: std::path::PathBuf,
}

impl Storage {
    /// A live engine database has one explicit account identity, independent of
    /// account-scoped research tables. Legacy history needs operator adoption.
    pub fn bind_trading_account(&self, address: &str, adopt_existing_history: bool) -> Result<()> {
        let account = crate::ownership::canonical_account(address)?;
        self.with_conn(|conn| -> Result<()> {
            let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
            let existing: Option<String> = tx.query_row(
                "SELECT value FROM meta_kv WHERE key='trading_account_binding'", [], |row| row.get(0)).optional()?;
            if let Some(existing) = existing {
                anyhow::ensure!(existing == account, "database is bound to another trading account");
                tx.commit()?;
                return Ok(());
            }
            let history: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM positions) OR EXISTS(SELECT 1 FROM execution_intents) OR EXISTS(SELECT 1 FROM closed_trades) OR EXISTS(SELECT 1 FROM sessions WHERE live_trading=1)",
                [], |row| row.get(0))?;
            anyhow::ensure!(!history || adopt_existing_history,
                "unbound historical database requires explicit bind-history --adopt-existing-history");
            let now = Utc::now().to_rfc3339();
            tx.execute("INSERT INTO meta_kv(key,value,updated_at) VALUES('trading_account_binding',?1,?2)", params![account,now])?;
            tx.execute("INSERT INTO meta_kv(key,value,updated_at) VALUES('trading_account_binding_origin',?1,?2)",
                params![if history {"operator_adopted_existing_history"} else {"empty_database"},now])?;
            tx.commit()?;
            Ok(())
        })
    }
    /// Run a closure with mutex-protected access to the underlying SQLite
    /// connection. Used by the v1.5.0 learning module which needs to issue
    /// arbitrary read queries against `trade_context`.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let conn = self.inner.lock().unwrap();
        f(&conn)
    }
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("open sqlite at {}", path.as_ref().display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
            path: path.as_ref().to_path_buf(),
        })
    }

    /// Atomically reserve a UUID before any network write. Duplicate IDs are never retried.
    pub fn reserve_intent(&self, intent: &OrderIntent) -> Result<()> {
        self.with_conn(|conn| Self::insert_intent_on(conn, intent))
    }

    /// Shared by ordinary journaling and atomic Aegis admission. A Transaction
    /// dereferences to its connection, keeping both facts in one commit.
    pub(crate) fn insert_intent_on(conn: &Connection, intent: &OrderIntent) -> Result<()> {
        conn.execute(
            "INSERT INTO execution_intents (intent_id,venue,symbol,intent_json,state,updated_at) VALUES (?1,?2,?3,?4,'submitted',?5)",
            params![intent.id,intent.venue,intent.symbol,serde_json::to_string(intent)?,Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Atomic execution admission requires the same protected connection handle.
    /// File-path equality is insufficient for memory databases or replaced files.
    pub(crate) fn shares_connection(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn finish_intent(&self, id: &str, state: &str, result: Option<&OrderResult>) -> Result<()> {
        self.with_conn(|conn| -> Result<()> {
            let changed = conn.execute(
                "UPDATE execution_intents SET state=?1,result_json=?2,updated_at=?3 WHERE intent_id=?4",
                params![state, result.map(serde_json::to_string).transpose()?, Utc::now().to_rfc3339(), id],
            )?;
            anyhow::ensure!(changed == 1, "execution intent missing");
            Ok(())
        })
    }

    pub fn ensure_symbol_execution_resolved(&self, venue: &str, symbol: &str) -> Result<()> {
        self.with_conn(|conn| -> Result<()> {
            let pending: i64 = conn.query_row(
                "SELECT COUNT(*) FROM execution_intents WHERE venue=?1 AND symbol=?2 AND state NOT IN ('rejected','applied')",
                params![venue, symbol], |row| row.get(0),
            )?;
            anyhow::ensure!(pending == 0, "prior order unresolved for {venue}/{symbol}; reconcile before reducing again");
            Ok(())
        })
    }

    /// In-flight/ambiguous orders and reductions awaiting net accounting block new exposure.
    pub fn ensure_execution_resolved(&self) -> Result<()> {
        self.with_conn(|conn| -> Result<()> {
            let pending: i64 = conn.query_row(
                "SELECT (SELECT COUNT(*) FROM execution_intents WHERE state NOT IN ('rejected','applied')) + (SELECT COUNT(*) FROM confirmed_reductions WHERE net_pnl IS NULL)",
                [], |row| row.get(0),
            )?;
            anyhow::ensure!(pending == 0, "account execution/accounting unresolved: {pending} records; new exposure blocked");
            Ok(())
        })
    }

    /// Record a proven reduction and residual atomically. Gross PnL is diagnostic only.
    /// Net PnL stays unavailable until fee/funding ingestion is implemented.
    pub fn apply_confirmed_reduction(
        &self,
        p: &OpenPosition,
        res: &OrderResult,
        remaining: f64,
        reason: &str,
    ) -> Result<()> {
        self.apply_confirmed_reduction_with_snapshot(p, res, remaining, reason, None)
    }

    pub fn apply_confirmed_reduction_with_snapshot(
        &self,
        p: &OpenPosition,
        res: &OrderResult,
        remaining: f64,
        reason: &str,
        evidence: Option<&crate::adapters::position_evidence::PositionSnapshot>,
    ) -> Result<()> {
        let px = res
            .avg_fill_price
            .ok_or_else(|| anyhow::anyhow!("missing fill price"))?;
        anyhow::ensure!(
            res.accepted
                && res.filled_size.is_finite()
                && res.filled_size > 0.0
                && p.size.is_finite()
                && p.size > 0.0
                && res.filled_size <= p.size
                && px.is_finite()
                && px > 0.0
                && p.entry_price.is_finite()
                && p.entry_price > 0.0
                && remaining.is_finite()
                && remaining >= 0.0
                && (remaining - (p.size - res.filled_size)).abs() <= 1e-10 * p.size.max(1.0),
            "invalid confirmed reduction"
        );
        let gross =
            (px - p.entry_price) * res.filled_size * if p.side == "LONG" { 1.0 } else { -1.0 };
        anyhow::ensure!(gross.is_finite(), "reduction PnL overflow");
        self.with_conn(|conn| -> Result<()> {
            let tx = conn.unchecked_transaction()?;
            let (intent_json, result_json): (String, String) = tx.query_row(
                "SELECT intent_json,result_json FROM execution_intents WHERE intent_id=?1 AND state='filled'",
                [&res.intent_id], |row| Ok((row.get(0)?,row.get(1)?)))?;
            let intent: OrderIntent = serde_json::from_str(&intent_json)?;
            let recorded: OrderResult = serde_json::from_str(&result_json)?;
            anyhow::ensure!(intent.reduce_only && intent.venue == p.venue && intent.symbol == p.symbol
                && intent.side.is_buy() == (p.side == "SHORT")
                && recorded.filled_size == res.filled_size && recorded.avg_fill_price == res.avg_fill_price
                && recorded.accepted && recorded.intent_id == res.intent_id,
                "reduction does not match durable exchange evidence");
            let unchanged:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM positions WHERE id=?1 AND venue=?2 AND symbol=?3 AND side=?4 AND size=?5 AND entry_price=?6 AND closed_at IS NULL)",params![p.id,p.venue,p.symbol,p.side,p.size,p.entry_price],|r|r.get(0))?;
            anyhow::ensure!(unchanged,"position identity changed during close");
            let now = Utc::now().to_rfc3339();
            tx.execute("INSERT INTO confirmed_reductions (intent_id,position_id,filled_size,exit_price,remaining_size,gross_pnl,reason,confirmed_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![res.intent_id,p.id,res.filled_size,px,remaining,gross,reason,now])?;
            let n = tx.execute("UPDATE positions SET size=?1,closed_at=?2 WHERE id=?3 AND closed_at IS NULL AND size=?4",
                params![remaining,if remaining == 0.0 {Some(&now)} else {None},p.id,p.size])?;
            anyhow::ensure!(n == 1, "position changed during close; reconciliation required");
            let precise=if let Some(evidence)=evidence {
                crate::execution_facts::apply_residual_on(&tx,p,res,remaining,evidence,Utc::now().timestamp_millis())?
            } else {false};
            if !precise {crate::execution_facts::invalidate_position_on(&tx,p.id)?;}
            tx.execute("UPDATE execution_intents SET state='applied',updated_at=?1 WHERE intent_id=?2", params![now,res.intent_id])?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn migrate(&self) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(
            version <= 2,
            "database schema is newer than this Hydra build"
        );
        schema::run_migrations(&tx)?;
        crate::execution_facts::migrate_on(&tx)?;
        crate::order_status::migrate_on(&tx)?;
        crate::submission_wire::migrate_on(&tx)?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
        Ok(())
    }

    pub fn begin_session(
        &self,
        cfg: &AppConfig,
        venues: &[String],
        symbols: &[String],
    ) -> Result<i64> {
        let conn = self.inner.lock().unwrap();
        let now = Utc::now();
        conn.execute(
            "INSERT INTO sessions (started_at, venues, symbols, live_trading, config_snapshot)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                now.to_rfc3339(),
                venues.join(","),
                symbols.join(","),
                cfg.live_trading as i64,
                serde_json::to_string(&session_config_snapshot(cfg))?,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn end_session(&self, id: i64, reason: &str, end: DateTime<Utc>) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        // Compute summary
        let trades: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM orders WHERE session_id=?1 AND accepted=1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let realized: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(realized_pnl),0) FROM closed_trades WHERE session_id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        let started: String = conn.query_row(
            "SELECT started_at FROM sessions WHERE id=?1",
            params![id],
            |r| r.get(0),
        )?;
        let started_dt: DateTime<Utc> = DateTime::parse_from_rfc3339(&started)?.with_timezone(&Utc);
        let runtime = (end - started_dt).num_seconds();
        conn.execute(
            "UPDATE sessions SET ended_at=?1, runtime_secs=?2, trades=?3, realized_usd=?4, shutdown_reason=?5 WHERE id=?6",
            params![end.to_rfc3339(), runtime, trades, realized, reason, id],
        )?;
        Ok(())
    }

    pub fn record_signal(&self, session_id: i64, s: &Signal) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO signals (session_id, strategy, venue, symbol, side, confidence, expected_edge_bps,
                                  stop_price, take_profit, reason, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                session_id, s.strategy, s.venue, s.symbol, s.side.as_str(),
                s.confidence, s.expected_edge_bps,
                s.stop_price, s.take_profit, s.reason, s.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn record_order(
        &self,
        session_id: i64,
        intent: &OrderIntent,
        res: &OrderResult,
    ) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO orders (session_id, intent_id, venue, symbol, side, size, limit_price,
                                 reduce_only, accepted, venue_order_id, filled_size, avg_fill_price,
                                 message, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                session_id,
                intent.id,
                intent.venue,
                intent.symbol,
                intent.side.as_str(),
                intent.size,
                intent.limit_price,
                intent.reduce_only as i64,
                res.accepted as i64,
                res.venue_order_id,
                res.filled_size,
                res.avg_fill_price,
                res.message,
                intent.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn record_error(&self, session_id: i64, kind: &str, msg: &str) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO errors (session_id, kind, message, at) VALUES (?1,?2,?3,?4)",
            params![session_id, kind, msg, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn insert_open_position(&self, p: &OpenPosition) -> Result<i64> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO positions (venue, symbol, side, size, entry_price, stop_price, take_profit,
                                    opened_at, strategy, trailing_high)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![p.venue, p.symbol, p.side, p.size, p.entry_price,
                    p.stop_price, p.take_profit, p.opened_at.to_rfc3339(),
                    p.strategy, p.trailing_high],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_open_positions(&self) -> Result<Vec<OpenPosition>> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, venue, symbol, side, size, entry_price, stop_price, take_profit,
                    opened_at, strategy, trailing_high FROM positions WHERE closed_at IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            let opened: String = row.get(8)?;
            let dt = DateTime::parse_from_rfc3339(&opened)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
            Ok(OpenPosition {
                id: row.get(0)?,
                venue: row.get(1)?,
                symbol: row.get(2)?,
                side: row.get(3)?,
                size: row.get(4)?,
                entry_price: row.get(5)?,
                stop_price: row.get(6)?,
                take_profit: row.get(7)?,
                opened_at: dt,
                strategy: row.get(9)?,
                trailing_high: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_position_trailing(
        &self,
        id: i64,
        new_trail: f64,
        new_stop: Option<f64>,
    ) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "UPDATE positions SET trailing_high=?1, stop_price=COALESCE(?2, stop_price) WHERE id=?3",
            params![new_trail, new_stop, id],
        )?;
        Ok(())
    }

    pub fn close_position(
        &self,
        id: i64,
        exit_price: f64,
        realized_pnl: f64,
        session_id: i64,
        exit_reason: &str,
    ) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        // archive
        conn.execute(
            "INSERT INTO closed_trades (position_id, session_id, exit_price, realized_pnl, closed_at, exit_reason)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, session_id, exit_price, realized_pnl, now, exit_reason],
        )?;
        conn.execute(
            "UPDATE positions SET closed_at=?1 WHERE id=?2",
            params![now, id],
        )?;
        Ok(())
    }

    /// v1.6.1: Returns the timestamp of the most recent STOP-OUT close for
    /// the given (symbol, side). Used by the new stop-out-specific cooldown
    /// to prevent the chop-death pattern (re-entering the same losing setup
    /// 3 minutes after getting stopped out by noise). Distinct from
    /// `last_loss_close_for` which counts ANY losing close.
    pub fn last_stopout_close_for(
        &self,
        symbol: &str,
        side: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let conn = self.inner.lock().unwrap();
        let row: Option<String> = conn
            .query_row(
                "SELECT t.closed_at
             FROM closed_trades t
             JOIN positions p ON t.position_id = p.id
             WHERE p.symbol = ?1 AND p.side = ?2 AND t.exit_reason = 'stop'
             ORDER BY t.id DESC LIMIT 1",
                params![symbol, side],
                |r| r.get(0),
            )
            .optional()?;
        Ok(row.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }))
    }

    pub fn sum_realized_since(&self, since: DateTime<Utc>) -> Result<f64> {
        let conn = self.inner.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT COALESCE(SUM(realized_pnl),0) FROM closed_trades WHERE closed_at >= ?1",
                params![since.to_rfc3339()],
                |r| r.get(0),
            )
            .unwrap_or(0.0))
    }

    /// Count of closed trades in a given live session (drives the cockpit's
    /// "trades" counter so it updates in real time instead of at session end).
    pub fn count_closed_trades_in_session(&self, session_id: i64) -> Result<i64> {
        let conn = self.inner.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT COUNT(*) FROM closed_trades WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .unwrap_or(0))
    }

    /// Live sum of realized PnL for a session, computed from `closed_trades`.
    pub fn sum_realized_in_session(&self, session_id: i64) -> Result<f64> {
        let conn = self.inner.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT COALESCE(SUM(realized_pnl),0) FROM closed_trades WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .unwrap_or(0.0))
    }

    /// Returns the timestamp of the most recent LOSING close for the given
    /// (symbol, side). Used by the v1.3.3 re-entry cooldown to block immediate
    /// re-takes of a losing setup. Returns None if there is no prior loss
    /// for this combination, or if the most recent close was a win.
    pub fn last_loss_close_for(&self, symbol: &str, side: &str) -> Result<Option<DateTime<Utc>>> {
        let conn = self.inner.lock().unwrap();
        let row: Option<(String, f64)> = conn
            .query_row(
                "SELECT t.closed_at, t.realized_pnl
             FROM closed_trades t
             JOIN positions p ON t.position_id = p.id
             WHERE p.symbol = ?1 AND p.side = ?2
             ORDER BY t.id DESC LIMIT 1",
                params![symbol, side],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((closed_at, pnl)) if pnl < 0.0 => Ok(DateTime::parse_from_rfc3339(&closed_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))),
            _ => Ok(None),
        }
    }

    /// Count of consecutive losing closes walking back from most recent.
    /// **v1.3.4 contract:**
    /// - Only counts losses whose `closed_at` is AFTER `now − window_hours`.
    ///   Older losses age out automatically — no permanent deadlock.
    /// - If `last_cooldown_reset_at` (in `meta_kv`) is more recent than the
    ///   window cutoff, that wins: only losses after the reset are counted.
    /// - The first WIN encountered walking backwards still terminates the
    ///   streak. A WIN clears the streak immediately, regardless of window.
    pub fn consecutive_losses(&self, window_hours: i64) -> Result<u32> {
        let conn = self.inner.lock().unwrap();
        let mut cutoff = Utc::now() - Duration::hours(window_hours.max(1));
        // If a manual reset is more recent than the window, use that as the cutoff.
        if let Ok(Some(reset_at_str)) = conn
            .query_row::<String, _, _>(
                "SELECT value FROM meta_kv WHERE key = 'last_cooldown_reset_at'",
                [],
                |r| r.get(0),
            )
            .optional()
        {
            if let Ok(reset_at) = DateTime::parse_from_rfc3339(&reset_at_str) {
                let reset_utc = reset_at.with_timezone(&Utc);
                if reset_utc > cutoff {
                    cutoff = reset_utc;
                }
            }
        }
        let mut stmt = conn.prepare(
            "SELECT realized_pnl, closed_at FROM closed_trades
             ORDER BY id DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, f64>(0)?, r.get::<_, String>(1)?)))?;
        let mut n: u32 = 0;
        for row in rows {
            let (pnl, closed_at_str) = row?;
            // Stop walking backwards once we cross the cutoff.
            let closed_at = match DateTime::parse_from_rfc3339(&closed_at_str) {
                Ok(d) => d.with_timezone(&Utc),
                Err(_) => break,
            };
            if closed_at <= cutoff {
                break;
            }
            if pnl < 0.0 {
                n += 1;
            } else {
                break;
            }
        }
        Ok(n)
    }

    /// Stamp `meta_kv` with a manual cooldown-reset timestamp. Subsequent calls
    /// to `consecutive_losses()` ignore any losses that closed before this.
    pub fn reset_consecutive_losses(&self) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO meta_kv (key, value, updated_at) VALUES ('last_cooldown_reset_at', ?1, ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![now],
        )?;
        Ok(())
    }

    pub fn pnl_summary(&self, days: i64) -> Result<Vec<PnlRow>> {
        let since = Utc::now() - Duration::days(days);
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT substr(closed_at,1,10) AS day,
                    COUNT(*) AS trades,
                    SUM(CASE WHEN realized_pnl>0 THEN 1 ELSE 0 END) AS wins,
                    SUM(CASE WHEN realized_pnl<=0 THEN 1 ELSE 0 END) AS losses,
                    COALESCE(SUM(realized_pnl),0) AS realized
             FROM closed_trades WHERE closed_at >= ?1 GROUP BY day ORDER BY day DESC",
        )?;
        let rows = stmt.query_map(params![since.to_rfc3339()], |r| {
            Ok(PnlRow {
                day: r.get(0)?,
                trades: r.get(1)?,
                wins: r.get(2)?,
                losses: r.get(3)?,
                realized_usd: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_sessions(&self, last: i64) -> Result<Vec<SessionRow>> {
        let conn = self.inner.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, started_at, ended_at, runtime_secs, trades, realized_usd, shutdown_reason
             FROM sessions ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![last], |r| {
            let started: String = r.get(1)?;
            let ended: Option<String> = r.get(2)?;
            let started_at = DateTime::parse_from_rfc3339(&started)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let ended_at = ended.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            });
            Ok(SessionRow {
                id: r.get(0)?,
                started_at,
                ended_at,
                runtime_secs: r.get(3)?,
                trades: r.get(4).unwrap_or(0),
                realized_usd: r.get(5).unwrap_or(0.0),
                shutdown_reason: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn heartbeat(&self, session_id: i64, kind: &str) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO heartbeats (session_id, kind, at) VALUES (?1,?2,?3)",
            params![session_id, kind, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn last_equity(&self) -> Result<Option<(String, f64)>> {
        let conn = self.inner.lock().unwrap();
        let row: Option<(String, f64)> = conn
            .query_row(
                "SELECT venue, equity_usd FROM equity_snapshots ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn record_equity(&self, venue: &str, equity_usd: f64) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO equity_snapshots (venue, equity_usd, at) VALUES (?1,?2,?3)",
            params![venue, equity_usd, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_compound_state(&self) -> Result<CompoundState> {
        let conn = self.inner.lock().unwrap();
        let row = conn.query_row(
            "SELECT starting_capital, realized_profit, locked_buffer, current_risk_mult, updated_at
             FROM compound_state WHERE id=1", [], |r| Ok(CompoundState {
                starting_capital: r.get(0)?, realized_profit: r.get(1)?,
                locked_buffer: r.get(2)?, current_risk_mult: r.get(3)?,
                updated_at: r.get(4)?,
            })
        ).optional()?;
        Ok(row.unwrap_or(CompoundState::default()))
    }

    pub fn upsert_compound_state(&self, s: &CompoundState) -> Result<()> {
        let conn = self.inner.lock().unwrap();
        conn.execute(
            "INSERT INTO compound_state (id, starting_capital, realized_profit, locked_buffer, current_risk_mult, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                starting_capital=excluded.starting_capital,
                realized_profit=excluded.realized_profit,
                locked_buffer=excluded.locked_buffer,
                current_risk_mult=excluded.current_risk_mult,
                updated_at=excluded.updated_at",
            params![s.starting_capital, s.realized_profit, s.locked_buffer,
                    s.current_risk_mult, s.updated_at],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub runtime_secs: Option<i64>,
    pub trades: i64,
    pub realized_usd: f64,
    pub shutdown_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PnlRow {
    pub day: String,
    pub trades: i64,
    pub wins: i64,
    pub losses: i64,
    pub realized_usd: f64,
}

#[derive(Debug, Clone)]
pub struct CompoundState {
    pub starting_capital: f64,
    pub realized_profit: f64,
    pub locked_buffer: f64,
    pub current_risk_mult: f64,
    pub updated_at: String,
}
impl Default for CompoundState {
    fn default() -> Self {
        Self {
            starting_capital: 0.0,
            realized_profit: 0.0,
            locked_buffer: 0.0,
            current_risk_mult: 1.0,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

fn session_config_snapshot(cfg: &AppConfig) -> serde_json::Value {
    serde_json::json!({
        "live_trading": cfg.live_trading,
        "hl_network":   "mainnet",
        "risk": {
            "default_risk_pct": cfg.risk.default_risk_pct,
            "max_daily_loss_pct": cfg.risk.max_daily_loss_pct,
            "max_drawdown_pct": cfg.risk.max_drawdown_pct,
        },
        "compound": {
            "reinvest_pct": cfg.compound.reinvest_pct,
            "lock_pct": cfg.compound.lock_pct,
        },
        "strategies": cfg.strategies.global.enabled_strategies.clone(),
    })
}

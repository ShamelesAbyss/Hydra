//! External Process Watchdog, Resource Monitor & Post-Restart Revalidation (`v1.17.0 | PROJECT SENTINEL`).
//!
//! Safety & operational invariants:
//! - Prevents duplicate watchdog instances via exclusive OS file lock (`watchdog.lock`).
//! - Persists restart budgets atomically in `watchdog-state.json` across restarts.
//! - Supervises live worker processes cleanly via non-blocking heartbeat/progress probes,
//!   bounded graceful shutdown -> forced termination on hang, and enforced exponential backoff.
//! - Performs real SQLite `PRAGMA quick_check(1)`, database size checks, and OS disk/RAM/VRAM probes.
//! - Never auto-resumes live execution after a restart (`live_auto_resume_prohibited` is always `true`).

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchdogPolicy {
    pub max_restarts_in_window: usize,
    pub restart_window_secs: u64,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub min_free_disk_bytes: u64,
    pub max_db_bytes: u64,
    pub heartbeat_timeout_ms: i64,
}

impl Default for WatchdogPolicy {
    fn default() -> Self {
        Self {
            max_restarts_in_window: 3,
            restart_window_secs: 300,
            base_backoff_ms: 500,
            max_backoff_ms: 15_000,
            min_free_disk_bytes: 256 * 1024 * 1024, // 256 MiB safety floor
            max_db_bytes: 2 * 1024 * 1024 * 1024,   // 2 GiB safety ceiling
            heartbeat_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogAlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchdogNotification {
    pub timestamp_ms: i64,
    pub severity: WatchdogAlertSeverity,
    pub category: String,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbIntegrityReport {
    pub path: String,
    pub exists: bool,
    pub quick_check_ok: bool,
    pub detail: String,
    pub file_bytes: u64,
    pub within_size_budget: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSpaceReport {
    pub path: String,
    pub available_bytes: Option<u64>,
    pub min_required_bytes: u64,
    pub sufficient: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PostRestartEvidence {
    pub account_identity_ok: Option<bool>,
    pub authorization_ok: Option<bool>,
    pub positions_reconciled: Option<bool>,
    pub unresolved_intents: Option<usize>,
    pub financial_state_verified: Option<bool>,
    pub market_fresh: Option<bool>,
    pub risk_ready: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostRestartRevalidationChecklist {
    pub account_identity_ok: bool,
    pub authorization_ok: bool,
    pub positions_reconciled: bool,
    pub unresolved_intents_zero: bool,
    pub financial_state_verified: bool,
    pub market_fresh: bool,
    pub risk_ready: bool,
    pub can_resume_simulation: bool,
    pub live_auto_resume_prohibited: bool,
}

impl PostRestartRevalidationChecklist {
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        account_identity_ok: bool,
        authorization_ok: bool,
        positions_reconciled: bool,
        unresolved_intents_zero: bool,
        financial_state_verified: bool,
        market_fresh: bool,
        risk_ready: bool,
    ) -> Self {
        let can_resume_simulation =
            positions_reconciled && unresolved_intents_zero && market_fresh && risk_ready;
        Self {
            account_identity_ok,
            authorization_ok,
            positions_reconciled,
            unresolved_intents_zero,
            financial_state_verified,
            market_fresh,
            risk_ready,
            can_resume_simulation,
            live_auto_resume_prohibited: true, // Always requires explicit owner re-entry after restart
        }
    }

    /// Evaluates post-restart readiness from observed database/process evidence.
    /// Unknown inputs fail closed for live authorization and market freshness; offline
    /// simulation resumption is blocked if any verified check fails or unresolved intents remain.
    pub fn from_evidence(ev: &PostRestartEvidence) -> Self {
        let account_identity_ok = ev.account_identity_ok.unwrap_or(false);
        let authorization_ok = ev.authorization_ok.unwrap_or(false);
        let positions_reconciled = ev.positions_reconciled.unwrap_or(false);
        let unresolved_intents_zero = ev.unresolved_intents == Some(0);
        let financial_state_verified = ev.financial_state_verified.unwrap_or(false);
        let market_fresh = ev.market_fresh.unwrap_or(false);
        let risk_ready = ev.risk_ready.unwrap_or(false);
        // When invoked offline without a live feed (`market_fresh == None`), report
        // `market_fresh = false` honestly while allowing offline DB-verified simulation recovery
        // as long as no stale feed failure was explicitly observed.
        let feed_ok_for_sim = ev.market_fresh != Some(false);
        let can_resume_simulation =
            positions_reconciled && unresolved_intents_zero && risk_ready && feed_ok_for_sim;
        Self {
            account_identity_ok,
            authorization_ok,
            positions_reconciled,
            unresolved_intents_zero,
            financial_state_verified,
            market_fresh,
            risk_ready,
            can_resume_simulation,
            live_auto_resume_prohibited: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerHealthProbe {
    pub alive_heartbeat_ms: Option<i64>,
    pub progress_counter: u64,
    pub healthy: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResourceSnapshot {
    pub timestamp_ms: i64,
    pub logical_cores: usize,
    pub ram_total_mb: Option<u64>,
    pub ram_available_mb: Option<u64>,
    pub available_disk_bytes: Option<u64>,
    pub db_bytes: u64,
    pub gpu_name: Option<String>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
    pub gpu_utilization_pct: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartBudgetTracker {
    window_ms: i64,
    max_restarts: usize,
    base_backoff_ms: u64,
    max_backoff_ms: u64,
    history_ms: VecDeque<i64>,
    pub total_restarts: usize,
}

impl RestartBudgetTracker {
    pub fn new(policy: &WatchdogPolicy) -> Self {
        Self {
            window_ms: (policy.restart_window_secs as i64).saturating_mul(1_000),
            max_restarts: policy.max_restarts_in_window,
            base_backoff_ms: policy.base_backoff_ms.max(10),
            max_backoff_ms: policy.max_backoff_ms.max(policy.base_backoff_ms.max(10)),
            history_ms: VecDeque::new(),
            total_restarts: 0,
        }
    }

    pub fn record_restart(&mut self, now_ms: i64) -> Result<Duration> {
        while let Some(&front) = self.history_ms.front() {
            if now_ms.saturating_sub(front) > self.window_ms {
                self.history_ms.pop_front();
            } else {
                break;
            }
        }
        ensure!(
            self.history_ms.len() < self.max_restarts,
            "Critical escalation: restart budget exhausted ({} restarts within {}s window)",
            self.max_restarts,
            self.window_ms / 1000
        );
        self.history_ms.push_back(now_ms);
        self.total_restarts += 1;
        let exp = (self.history_ms.len().saturating_sub(1)).min(6) as u32;
        let backoff = (self.base_backoff_ms.saturating_mul(1u64 << exp)).min(self.max_backoff_ms);
        Ok(Duration::from_millis(backoff))
    }
}

/// Exclusive OS lock guaranteeing only one watchdog supervisor owns `root/watchdog.lock`.
pub struct WatchdogLock {
    _file: File,
}

impl Drop for WatchdogLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

impl WatchdogLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).context("create watchdog root directory")?;
        let lock_path = root.join("watchdog.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .context("open watchdog lockfile")?;
        file.try_lock().map_err(|_| {
            anyhow!(
                "another HYDRA watchdog supervisor already holds {}",
                lock_path.display()
            )
        })?;
        Ok(Self { _file: file })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchdogSupervisionSummary {
    pub launches: usize,
    pub restarts: usize,
    pub escalated: bool,
    pub last_exit_code: Option<i32>,
    pub detail: String,
}

pub struct WatchdogSupervisor {
    root: PathBuf,
    policy: WatchdogPolicy,
    _lock: WatchdogLock,
    tracker: RestartBudgetTracker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedWatchdogState {
    history_ms: VecDeque<i64>,
    total_restarts: usize,
}

impl WatchdogSupervisor {
    pub fn state_path(root: &Path) -> PathBuf {
        root.join("watchdog-state.json")
    }

    pub fn acquire(root: impl AsRef<Path>, policy: WatchdogPolicy) -> Result<Self> {
        let root_pb = root.as_ref().to_path_buf();
        let lock = WatchdogLock::acquire(&root_pb)?;
        let mut tracker = RestartBudgetTracker::new(&policy);
        let sp = Self::state_path(&root_pb);
        if sp.is_file() {
            if let Ok(raw) = fs::read_to_string(&sp) {
                if let Ok(saved) = serde_json::from_str::<PersistedWatchdogState>(&raw) {
                    tracker.history_ms = saved.history_ms;
                    tracker.total_restarts = saved.total_restarts;
                }
            }
        }
        Ok(Self {
            root: root_pb,
            policy,
            _lock: lock,
            tracker,
        })
    }

    fn persist_state(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let saved = PersistedWatchdogState {
            history_ms: self.tracker.history_ms.clone(),
            total_restarts: self.tracker.total_restarts,
        };
        let bytes = serde_json::to_vec_pretty(&saved)?;
        let dst = Self::state_path(&self.root);
        let tmp = self.root.join("watchdog-state.json.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &dst)?;
        Ok(())
    }

    fn record_and_persist_restart(&mut self, now_ms: i64) -> Result<Duration> {
        let res = self.tracker.record_restart(now_ms);
        let _ = self.persist_state();
        res
    }

    pub fn tracker(&self) -> &RestartBudgetTracker {
        &self.tracker
    }

    pub fn policy(&self) -> &WatchdogPolicy {
        &self.policy
    }

    pub fn alerts_path(&self) -> PathBuf {
        self.root.join("watchdog-alerts.jsonl")
    }

    pub fn emit_notification(&self, note: &WatchdogNotification) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let line = serde_json::to_string(note)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.alerts_path())?;
        writeln!(f, "{line}")?;
        f.sync_all()?;
        Ok(())
    }

    pub fn check_database_integrity(db_path: &Path, max_db_bytes: u64) -> DbIntegrityReport {
        if !db_path.is_file() {
            return DbIntegrityReport {
                path: db_path.display().to_string(),
                exists: false,
                quick_check_ok: false,
                detail: "database file not found".into(),
                file_bytes: 0,
                within_size_budget: true,
            };
        }
        let file_bytes = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
        let within_size_budget = file_bytes <= max_db_bytes;
        match Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => {
                match conn.query_row("PRAGMA quick_check(1)", [], |r| r.get::<_, String>(0)) {
                    Ok(res) if res.eq_ignore_ascii_case("ok") => DbIntegrityReport {
                        path: db_path.display().to_string(),
                        exists: true,
                        quick_check_ok: true,
                        detail: "ok".into(),
                        file_bytes,
                        within_size_budget,
                    },
                    Ok(other) => DbIntegrityReport {
                        path: db_path.display().to_string(),
                        exists: true,
                        quick_check_ok: false,
                        detail: other,
                        file_bytes,
                        within_size_budget,
                    },
                    Err(e) => DbIntegrityReport {
                        path: db_path.display().to_string(),
                        exists: true,
                        quick_check_ok: false,
                        detail: e.to_string(),
                        file_bytes,
                        within_size_budget,
                    },
                }
            }
            Err(e) => DbIntegrityReport {
                path: db_path.display().to_string(),
                exists: true,
                quick_check_ok: false,
                detail: e.to_string(),
                file_bytes,
                within_size_budget,
            },
        }
    }

    pub fn check_disk_space(path: &Path, min_required_bytes: u64) -> DiskSpaceReport {
        let avail = query_free_disk_bytes(path);
        let sufficient = matches!(avail, Some(b) if b >= min_required_bytes);
        DiskSpaceReport {
            path: path.display().to_string(),
            available_bytes: avail,
            min_required_bytes,
            sufficient,
        }
    }

    /// Supervises a real external child process across crashes using the configured
    /// restart budget, enforced exponential backoff, and durable fault escalation.
    /// Clean exit code 0 ends supervision normally.
    pub fn supervise_bounded_process(
        &mut self,
        program: &Path,
        args: &[&str],
        max_launches: usize,
    ) -> Result<WatchdogSupervisionSummary> {
        ensure!(max_launches >= 1, "max_launches must be >= 1");
        let mut launches = 0usize;
        let mut last_code = None;
        while launches < max_launches {
            launches += 1;
            let status = Command::new(program)
                .args(args)
                .status()
                .with_context(|| format!("spawn supervised process {}", program.display()))?;
            last_code = status.code();
            if status.success() {
                return Ok(WatchdogSupervisionSummary {
                    launches,
                    restarts: self.tracker.total_restarts,
                    escalated: false,
                    last_exit_code: last_code,
                    detail: "Child exited cleanly (code 0)".into(),
                });
            }

            let now_ms = Utc::now().timestamp_millis();
            match self.record_and_persist_restart(now_ms) {
                Ok(backoff) => {
                    let _ = self.emit_notification(&WatchdogNotification {
                        timestamp_ms: now_ms,
                        severity: WatchdogAlertSeverity::Warning,
                        category: "child_exit".into(),
                        message: format!(
                            "Supervised worker exited non-zero ({last_code:?}); restart #{} scheduled after {}ms backoff",
                            self.tracker.total_restarts,
                            backoff.as_millis()
                        ),
                        remediation: "Revalidate state and monitor worker heartbeat".into(),
                    });
                    std::thread::sleep(backoff);
                }
                Err(err) => {
                    let _ = self.emit_notification(&WatchdogNotification {
                        timestamp_ms: now_ms,
                        severity: WatchdogAlertSeverity::Critical,
                        category: "restart_budget_exhausted".into(),
                        message: err.to_string(),
                        remediation: "Halting automatic restarts; operator inspection required"
                            .into(),
                    });
                    return Ok(WatchdogSupervisionSummary {
                        launches,
                        restarts: self.tracker.total_restarts,
                        escalated: true,
                        last_exit_code: last_code,
                        detail: err.to_string(),
                    });
                }
            }
        }
        Ok(WatchdogSupervisionSummary {
            launches,
            restarts: self.tracker.total_restarts,
            escalated: false,
            last_exit_code: last_code,
            detail: "Reached bounded supervision launch cap".into(),
        })
    }

    /// Supervises a long-lived worker process via non-blocking `try_wait()` + external
    /// heartbeat/progress polling. Detects hangs (`!healthy` or stale heartbeat beyond
    /// `policy.heartbeat_timeout_ms`), attempts graceful stop before forced `kill()`,
    /// enforces exponential backoff, and persists restart budgets + durable alerts.
    #[allow(clippy::too_many_arguments)]
    pub fn supervise_live_worker<FProbe, FStop>(
        &mut self,
        program: &Path,
        args: &[&str],
        poll_interval: Duration,
        graceful_stop_timeout: Duration,
        max_launches: usize,
        mut probe: FProbe,
        mut request_graceful_stop: FStop,
    ) -> Result<WatchdogSupervisionSummary>
    where
        FProbe: FnMut(u32) -> WorkerHealthProbe,
        FStop: FnMut(u32) -> Result<()>,
    {
        ensure!(max_launches >= 1, "max_launches must be >= 1");
        let poll = poll_interval.max(Duration::from_millis(5));
        let mut launches = 0usize;
        let mut last_code = None;

        while launches < max_launches {
            launches += 1;
            let mut child = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .with_context(|| format!("spawn supervised live worker {}", program.display()))?;
            let pid = child.id();
            let spawn_time = Instant::now();
            let startup_grace =
                Duration::from_millis((self.policy.heartbeat_timeout_ms.max(0) as u64).min(5_000));
            let mut ever_healthy = false;
            let mut last_hb_instant = Instant::now();
            let mut last_heartbeat_ms = Utc::now().timestamp_millis();
            let mut last_progress = 0u64;

            let restart_reason: Option<String> = loop {
                if let Some(status) = child.try_wait().context("poll supervised worker")? {
                    last_code = status.code();
                    if status.success() {
                        return Ok(WatchdogSupervisionSummary {
                            launches,
                            restarts: self.tracker.total_restarts,
                            escalated: false,
                            last_exit_code: last_code,
                            detail: "Live worker exited cleanly (code 0)".into(),
                        });
                    }
                    break Some(format!(
                        "Supervised live worker pid={pid} exited ({last_code:?})"
                    ));
                }

                let p = probe(pid);
                let now_ms = Utc::now().timestamp_millis();
                if p.healthy {
                    ever_healthy = true;
                    let advanced = p
                        .alive_heartbeat_ms
                        .is_some_and(|hb| hb > last_heartbeat_ms)
                        || p.progress_counter > last_progress;
                    if advanced {
                        if let Some(hb) = p.alive_heartbeat_ms {
                            last_heartbeat_ms = last_heartbeat_ms.max(hb);
                        }
                        last_progress = last_progress.max(p.progress_counter);
                        last_hb_instant = Instant::now();
                    }
                }

                let startup_grace_elapsed = ever_healthy || spawn_time.elapsed() >= startup_grace;
                let stalled_ms = last_hb_instant.elapsed().as_millis() as i64;
                if (!p.healthy && startup_grace_elapsed)
                    || stalled_ms > self.policy.heartbeat_timeout_ms
                {
                    let _ = request_graceful_stop(pid);
                    let stop_deadline = Instant::now() + graceful_stop_timeout;
                    let mut exited = false;
                    let mut exited_clean = false;
                    while Instant::now() < stop_deadline {
                        if let Some(status) = child.try_wait().unwrap_or(None) {
                            last_code = status.code();
                            exited = true;
                            exited_clean = status.success();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    if exited_clean {
                        return Ok(WatchdogSupervisionSummary {
                            launches,
                            restarts: self.tracker.total_restarts,
                            escalated: false,
                            last_exit_code: last_code,
                            detail: "Live worker exited cleanly (code 0)".into(),
                        });
                    }
                    let _ = self.emit_notification(&WatchdogNotification {
                        timestamp_ms: now_ms,
                        severity: WatchdogAlertSeverity::Warning,
                        category: "worker_hang_detected".into(),
                        message: format!(
                            "Worker pid={pid} unhealthy/stalled ({stalled_ms}ms > {}ms): {}",
                            self.policy.heartbeat_timeout_ms, p.detail
                        ),
                        remediation: "Requesting bounded graceful stop before forced termination"
                            .into(),
                    });
                    if !exited {
                        let _ = child.kill();
                        last_code = child.wait().ok().and_then(|s| s.code());
                        let _ = self.emit_notification(&WatchdogNotification {
                            timestamp_ms: Utc::now().timestamp_millis(),
                            severity: WatchdogAlertSeverity::Warning,
                            category: "worker_hang_terminated".into(),
                            message: format!(
                                "Forced kill on unresponsive worker pid={pid} after {:?} grace window",
                                graceful_stop_timeout
                            ),
                            remediation: "Revalidate state and apply restart backoff".into(),
                        });
                    }
                    break Some(format!("Worker pid={pid} hang-restarted ({})", p.detail));
                }

                std::thread::sleep(poll);
            };

            if let Some(reason) = restart_reason {
                let now_ms = Utc::now().timestamp_millis();
                match self.record_and_persist_restart(now_ms) {
                    Ok(backoff) => {
                        let _ = self.emit_notification(&WatchdogNotification {
                            timestamp_ms: now_ms,
                            severity: WatchdogAlertSeverity::Warning,
                            category: "worker_restart_scheduled".into(),
                            message: format!(
                                "{reason}; restart #{} after {}ms backoff",
                                self.tracker.total_restarts,
                                backoff.as_millis()
                            ),
                            remediation: "Post-restart revalidation required before resuming"
                                .into(),
                        });
                        std::thread::sleep(backoff);
                    }
                    Err(err) => {
                        let _ = self.emit_notification(&WatchdogNotification {
                            timestamp_ms: now_ms,
                            severity: WatchdogAlertSeverity::Critical,
                            category: "restart_budget_exhausted".into(),
                            message: format!("{reason}; {err}"),
                            remediation: "Halting automatic restarts; operator inspection required"
                                .into(),
                        });
                        return Ok(WatchdogSupervisionSummary {
                            launches,
                            restarts: self.tracker.total_restarts,
                            escalated: true,
                            last_exit_code: last_code,
                            detail: err.to_string(),
                        });
                    }
                }
            }
        }

        Ok(WatchdogSupervisionSummary {
            launches,
            restarts: self.tracker.total_restarts,
            escalated: false,
            last_exit_code: last_code,
            detail: "Reached bounded live-worker supervision launch cap".into(),
        })
    }
}

fn sqlite_table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        [table],
        |_| Ok(1i64),
    )
    .is_ok()
}

/// Inspects an existing SQLite DB read-only for unresolved execution intents, account bindings,
/// and persisted `engine_control_authority` emergency-suspension status.
pub fn inspect_post_restart_evidence(
    db_path: &Path,
    db_rep: &DbIntegrityReport,
    disk_rep: &DiskSpaceReport,
) -> PostRestartEvidence {
    let mut unresolved_intents: Option<usize> = None;
    let mut emergency_suspended = false;
    let mut account_identity_ok = Some(false);

    if db_rep.exists && db_rep.quick_check_ok {
        if let Ok(conn) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            let mut total_unresolved = 0usize;
            let mut query_ok = true;

            if sqlite_table_exists(&conn, "orders") {
                match conn.query_row(
                    "SELECT COUNT(*) FROM orders WHERE state IN ('Pending', 'Submitted', 'PartiallyFilled', 'Cancelling', 'Unknown')",
                    [],
                    |r| r.get::<_, i64>(0),
                ) {
                    Ok(c) => total_unresolved = total_unresolved.saturating_add(c.max(0) as usize),
                    Err(_) => query_ok = false,
                }
            }
            if sqlite_table_exists(&conn, "execution_journal") {
                match conn.query_row(
                    "SELECT COUNT(*) FROM execution_journal WHERE state IN ('Submitted', 'Unknown')",
                    [],
                    |r| r.get::<_, i64>(0),
                ) {
                    Ok(c) => total_unresolved = total_unresolved.saturating_add(c.max(0) as usize),
                    Err(_) => query_ok = false,
                }
            }
            if query_ok {
                unresolved_intents = Some(total_unresolved);
            }

            if sqlite_table_exists(&conn, "meta_kv") {
                if let Ok(raw) = conn.query_row(
                    "SELECT value FROM meta_kv WHERE key = 'engine_control_authority'",
                    [],
                    |r| r.get::<_, String>(0),
                ) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                        if val
                            .get("emergency_suspended")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            emergency_suspended = true;
                        }
                    }
                }
                if let Ok(acct) = conn.query_row(
                    "SELECT value FROM meta_kv WHERE key = 'trading_account'",
                    [],
                    |r| r.get::<_, String>(0),
                ) {
                    if !acct.trim().is_empty() {
                        account_identity_ok = Some(true);
                    }
                }
            }
        }
    }

    let positions_reconciled = Some(db_rep.quick_check_ok && unresolved_intents == Some(0));
    let financial_state_verified = Some(db_rep.quick_check_ok);
    let risk_ready = Some(
        db_rep.quick_check_ok
            && disk_rep.sufficient
            && db_rep.within_size_budget
            && !emergency_suspended,
    );

    PostRestartEvidence {
        account_identity_ok,
        authorization_ok: Some(false), // Never assumed without explicit live re-authorization
        positions_reconciled,
        unresolved_intents,
        financial_state_verified,
        market_fresh: None, // Offline DB/disk health check cannot verify live market stream freshness
        risk_ready,
    }
}

pub fn run_watchdog_health_check(
    root: &Path,
    db_path: &Path,
    policy: &WatchdogPolicy,
) -> Result<(
    DbIntegrityReport,
    DiskSpaceReport,
    PostRestartRevalidationChecklist,
)> {
    fs::create_dir_all(root)?;
    let db_rep = WatchdogSupervisor::check_database_integrity(db_path, policy.max_db_bytes);
    let disk_rep = WatchdogSupervisor::check_disk_space(root, policy.min_free_disk_bytes);
    if !db_rep.quick_check_ok || !disk_rep.sufficient {
        let note = WatchdogNotification {
            timestamp_ms: Utc::now().timestamp_millis(),
            severity: WatchdogAlertSeverity::Critical,
            category: "health_check".into(),
            message: format!(
                "db_ok={} within_budget={} disk_ok={}",
                db_rep.quick_check_ok, db_rep.within_size_budget, disk_rep.sufficient
            ),
            remediation: "Inspect SQLite integrity & host storage before resuming simulation"
                .into(),
        };
        if let Ok(sup) = WatchdogSupervisor::acquire(root, policy.clone()) {
            let _ = sup.emit_notification(&note);
        } else {
            bail!(
                "duplicate watchdog supervisor already owns {}",
                root.display()
            );
        }
    }
    let evidence = inspect_post_restart_evidence(db_path, &db_rep, &disk_rep);
    let checklist = PostRestartRevalidationChecklist::from_evidence(&evidence);
    Ok((db_rep, disk_rep, checklist))
}

/// Captures actual local CPU core count, host memory, disk space, DB file footprint,
/// and optional NVIDIA GPU telemetry (`nvidia-smi`, if present).
pub fn measure_host_resources(root: &Path, db_path: &Path) -> HostResourceSnapshot {
    let logical_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let available_disk_bytes = query_free_disk_bytes(root);
    let db_bytes = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    let (ram_total_mb, ram_available_mb) = query_host_memory_mb();
    let (gpu_name, vram_used_mb, vram_total_mb, gpu_utilization_pct) = query_nvidia_gpu_telemetry();
    HostResourceSnapshot {
        timestamp_ms: Utc::now().timestamp_millis(),
        logical_cores,
        ram_total_mb,
        ram_available_mb,
        available_disk_bytes,
        db_bytes,
        gpu_name,
        vram_used_mb,
        vram_total_mb,
        gpu_utilization_pct,
    }
}

#[cfg(windows)]
fn query_host_memory_mb() -> (Option<u64>, Option<u64>) {
    #[repr(C)]
    struct MemoryStatusEx {
        dw_length: u32,
        dw_memory_load: u32,
        ull_total_phys: u64,
        ull_avail_phys: u64,
        ull_total_page_file: u64,
        ull_avail_page_file: u64,
        ull_total_virtual: u64,
        ull_avail_virtual: u64,
        ull_avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(lp_buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut mem = MemoryStatusEx {
        dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
        dw_memory_load: 0,
        ull_total_phys: 0,
        ull_avail_phys: 0,
        ull_total_page_file: 0,
        ull_avail_page_file: 0,
        ull_total_virtual: 0,
        ull_avail_virtual: 0,
        ull_avail_extended_virtual: 0,
    };
    // SAFETY: `dw_length` is initialized to the exact struct size and `mem` is valid aligned stack storage.
    let ok = unsafe { GlobalMemoryStatusEx(&mut mem) };
    if ok != 0 {
        (
            Some(mem.ull_total_phys / (1024 * 1024)),
            Some(mem.ull_avail_phys / (1024 * 1024)),
        )
    } else {
        (None, None)
    }
}

#[cfg(not(windows))]
fn query_host_memory_mb() -> (Option<u64>, Option<u64>) {
    let Ok(txt) = fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let mut total_kb = None;
    let mut avail_kb = None;
    for line in txt.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok());
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok());
        }
    }
    (total_kb.map(|kb| kb / 1024), avail_kb.map(|kb| kb / 1024))
}

fn query_nvidia_gpu_telemetry() -> (Option<String>, Option<u64>, Option<u64>, Option<u32>) {
    let Ok(out) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.used,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .stdin(Stdio::null())
        .output()
    else {
        return (None, None, None, None);
    };
    if !out.status.success() {
        return (None, None, None, None);
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        return (None, None, None, None);
    }
    let name = (!parts[0].is_empty()).then(|| parts[0].to_string());
    let used = parts[1].parse::<u64>().ok();
    let total = parts[2].parse::<u64>().ok();
    let util = parts[3].parse::<u32>().ok();
    (name, used, total, util)
}

#[cfg(windows)]
fn query_free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut avail = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    // SAFETY: valid null-terminated UTF-16 path and valid mutable u64 output pointers.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(avail)
}

#[cfg(not(windows))]
fn query_free_disk_bytes(path: &Path) -> Option<u64> {
    let target = if path.exists() { path } else { Path::new(".") };
    let out = Command::new("df").arg("-Pk").arg(target).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    let line = txt.lines().nth(1)?;
    let avail_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb.saturating_mul(1024))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_lock_restart_escalation_integrity_and_disk_checks_work() {
        let dir = tempfile::tempdir().unwrap();
        let sup1 = WatchdogSupervisor::acquire(dir.path(), WatchdogPolicy::default()).unwrap();
        // Second supervisor on the same root must be refused cleanly.
        assert!(WatchdogSupervisor::acquire(dir.path(), WatchdogPolicy::default()).is_err());

        // DB integrity passes on valid SQLite and fails on corrupt header.
        let valid_db = dir.path().join("ok.db");
        let conn = Connection::open(&valid_db).unwrap();
        conn.execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY); INSERT INTO t VALUES(1);")
            .unwrap();
        drop(conn);
        let ok_rep = WatchdogSupervisor::check_database_integrity(&valid_db, 10_000_000);
        assert!(ok_rep.exists && ok_rep.quick_check_ok && ok_rep.within_size_budget);

        let bad_db = dir.path().join("corrupt.db");
        fs::write(&bad_db, b"not-a-valid-sqlite-header-at-all").unwrap();
        let bad_rep = WatchdogSupervisor::check_database_integrity(&bad_db, 10_000_000);
        assert!(!bad_rep.quick_check_ok);

        // Disk free space reports genuinely and catches excessive floor requirement.
        let disk_ok = WatchdogSupervisor::check_disk_space(dir.path(), 1024);
        assert!(disk_ok.sufficient);
        let disk_exhausted = WatchdogSupervisor::check_disk_space(dir.path(), u64::MAX / 2);
        assert!(!disk_exhausted.sufficient);

        // Restart budget escalates & persists alert JSONL cleanly.
        drop(sup1);
        let pol = WatchdogPolicy {
            max_restarts_in_window: 1,
            ..Default::default()
        };
        let mut sup2 = WatchdogSupervisor::acquire(dir.path(), pol).unwrap();
        let shell = if cfg!(windows) {
            Path::new("cmd")
        } else {
            Path::new("sh")
        };
        let args: &[&str] = if cfg!(windows) {
            &["/C", "exit 1"]
        } else {
            &["-c", "exit 1"]
        };
        let sum = sup2.supervise_bounded_process(shell, args, 4).unwrap();
        assert!(sum.escalated);
        assert_eq!(sum.restarts, 1);
        assert!(sup2.alerts_path().is_file());

        // Post-restart revalidation ALWAYS prohibits automatic live trading resume.
        let check =
            PostRestartRevalidationChecklist::evaluate(true, true, true, true, true, true, true);
        assert!(check.can_resume_simulation);
        assert!(check.live_auto_resume_prohibited);
    }

    #[test]
    fn watchdog_persists_restart_budget_and_terminates_stalled_worker_with_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let pol = WatchdogPolicy {
            max_restarts_in_window: 2,
            base_backoff_ms: 40,
            max_backoff_ms: 120,
            heartbeat_timeout_ms: 100,
            ..Default::default()
        };
        let mut sup = WatchdogSupervisor::acquire(dir.path(), pol.clone()).unwrap();
        let (prog, args): (&Path, &[&str]) = if cfg!(windows) {
            (Path::new("ping"), &["-n", "10", "127.0.0.1"])
        } else {
            (Path::new("sleep"), &["10"])
        };

        let started = Instant::now();
        let sum = sup
            .supervise_live_worker(
                prog,
                args,
                Duration::from_millis(15),
                Duration::from_millis(30),
                2,
                |_pid| WorkerHealthProbe {
                    alive_heartbeat_ms: None,
                    progress_counter: 0,
                    healthy: true,
                    detail: "frozen heartbeat fixture".into(),
                },
                |_pid| Ok(()),
            )
            .unwrap();
        assert_eq!(sum.launches, 2);
        assert_eq!(sum.restarts, 2);
        assert!(!sum.escalated);
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(WatchdogSupervisor::state_path(dir.path()).is_file());
        drop(sup);

        // Re-acquiring watchdog loads persisted restart budget (2 already spent) and escalates immediately on next stall
        let mut sup2 = WatchdogSupervisor::acquire(dir.path(), pol).unwrap();
        assert_eq!(sup2.tracker().total_restarts, 2);
        let sum2 = sup2
            .supervise_live_worker(
                prog,
                args,
                Duration::from_millis(15),
                Duration::from_millis(20),
                2,
                |_pid| WorkerHealthProbe {
                    alive_heartbeat_ms: None,
                    progress_counter: 0,
                    healthy: false,
                    detail: "unhealthy".into(),
                },
                |_pid| Ok(()),
            )
            .unwrap();
        assert!(sum2.escalated);

        // Unresolved DB intents and emergency suspend block simulation resumption
        let db_path = dir.path().join("state.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE orders(id INTEGER PRIMARY KEY, state TEXT NOT NULL);
             INSERT INTO orders(state) VALUES('Submitted');",
        )
        .unwrap();
        drop(conn);
        let (_, _, cl) =
            run_watchdog_health_check(dir.path(), &db_path, &WatchdogPolicy::default()).unwrap();
        assert!(!cl.unresolved_intents_zero);
        assert!(!cl.can_resume_simulation);
        assert!(!cl.market_fresh);
        assert!(cl.live_auto_resume_prohibited);

        let snap = measure_host_resources(dir.path(), &db_path);
        assert!(snap.logical_cores >= 1);
    }
}

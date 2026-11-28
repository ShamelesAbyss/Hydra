//! Independent headless Paper Engine Service with OS session ownership locks
//! and authenticated local loopback control IPC.
//!
//! Architectural guarantees:
//! - Runs completely outside any GUI process so closing/minimizing/restarting the
//!   native cockpit never interrupts ongoing paper simulation.
//! - Uses exclusive OS file locking (`paper-locks/paper-session-<id>.lock`) so two
//!   processes cannot corrupt or duplicate the same session.
//! - Exposes OS-scoped authenticated local TCP control (`127.0.0.1:0` with a 256-bit
//!   random token saved to `paper-control/<id>.json`, `chmod 0600` on Unix) for
//!   `Status`, `Pause`, `Resume`, `Stop`, and `Heartbeat`. Never accepts or transmits
//!   private signing keys.

use crate::hunter_stream::HunterStreamClient;
use crate::paper_pipeline::SynapsePipeline;
use crate::paper_store::{PaperSessionStatus, PaperStore};
use crate::synapse_bus::DataProvenance;
use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

const MAX_IPC_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperControlDescriptor {
    pub schema_version: u32,
    pub session_id: String,
    pub pid: u32,
    pub port: u16,
    pub auth_token_hex: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperControlCommand {
    Status,
    Pause,
    Resume,
    Stop,
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PaperControlWireRequest {
    auth_token_hex: String,
    command: PaperControlCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperControlResponse {
    pub ok: bool,
    pub error: Option<String>,
    pub session_id: String,
    pub status: String,
    pub paused: bool,
    pub cycle_count: u64,
    pub mtm_equity_usd: String,
    pub stream_connected: bool,
    pub stream_reconnects: u64,
    pub fallback_rest_active: bool,
    pub stale_symbols: Vec<String>,
    #[serde(default)]
    pub watchdog_restarts: u64,
    #[serde(default)]
    pub fault_reason: Option<String>,
    pub pid: u32,
    pub checked_at: String,
}

fn read_bounded_ipc_line(stream: &TcpStream, max_bytes: usize) -> Result<String> {
    let cloned = stream.try_clone().context("clone IPC stream")?;
    let mut reader = BufReader::new(cloned.take((max_bytes + 1) as u64));
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    ensure!(n > 0, "empty IPC frame");
    ensure!(
        buf.len() <= max_bytes,
        "IPC frame exceeded {max_bytes} byte limit"
    );
    String::from_utf8(buf).context("IPC frame was not valid UTF-8")
}

pub struct PaperSessionLock {
    _file: File,
}

impl Drop for PaperSessionLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

impl PaperSessionLock {
    pub fn acquire(root: &Path, session_id: &str) -> Result<Self> {
        ensure!(
            !session_id.is_empty()
                && session_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid paper session identifier"
        );
        let lock_dir = root.join("paper-locks");
        fs::create_dir_all(&lock_dir).context("create paper lock directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&lock_dir, fs::Permissions::from_mode(0o700))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_dir.join(format!("paper-session-{session_id}.lock")))?;
        file.try_lock().map_err(|_| {
            anyhow!(
                "paper session '{session_id}' is already owned by another active engine service"
            )
        })?;
        // Never unlink held lock files.
        Ok(Self { _file: file })
    }

    /// Verifies that no active worker holds the OS file lock for `session_id`.
    pub fn assert_unowned(root: &Path, session_id: &str) -> Result<()> {
        ensure!(
            !session_id.is_empty()
                && session_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid paper session identifier"
        );
        let lock_path = root
            .join("paper-locks")
            .join(format!("paper-session-{session_id}.lock"));
        if !lock_path.is_file() {
            return Ok(());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("probe paper session lock {}", lock_path.display()))?;
        file.try_lock().map_err(|_| {
            anyhow!(
                "paper session '{session_id}' is currently locked by an active engine service; stop it before archive/reset"
            )
        })?;
        let _ = file.unlock();
        Ok(())
    }
}

fn descriptor_path(root: &Path, session_id: &str) -> PathBuf {
    root.join("paper-control")
        .join(format!("{session_id}.json"))
}

fn write_control_descriptor(root: &Path, desc: &PaperControlDescriptor) -> Result<PathBuf> {
    let dir = root.join("paper-control");
    fs::create_dir_all(&dir).context("create paper control directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let path = descriptor_path(root, &desc.session_id);
    let tmp = path.with_extension("tmp");
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(serde_json::to_string_pretty(desc)?.as_bytes())?;
        f.sync_all()?;
    }
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

fn generate_auth_token_hex() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(Debug, Clone)]
pub struct LiveServiceTelemetry {
    pub session_id: String,
    pub status: PaperSessionStatus,
    pub paused: bool,
    pub cycle_count: u64,
    pub mtm_equity_usd: String,
    pub stream_connected: bool,
    pub stream_reconnects: u64,
    pub fallback_rest_active: bool,
    pub stale_symbols: Vec<String>,
    pub watchdog_restarts: u64,
    pub fault_reason: Option<String>,
}

struct ControlServerGuard {
    descriptor_file: PathBuf,
    stop_ipc: Arc<AtomicBool>,
}

impl Drop for ControlServerGuard {
    fn drop(&mut self) {
        self.stop_ipc.store(true, Ordering::Relaxed);
        let _ = fs::remove_file(&self.descriptor_file);
    }
}

pub struct PaperServiceClient;

impl PaperServiceClient {
    pub fn load_descriptor(
        root: &Path,
        session_id: &str,
    ) -> Result<Option<PaperControlDescriptor>> {
        let path = descriptor_path(root, session_id);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).context("read paper control descriptor")?;
        let desc: PaperControlDescriptor =
            serde_json::from_str(&raw).context("parse paper control descriptor")?;
        Ok(Some(desc))
    }

    pub fn send_command(
        root: &Path,
        session_id: &str,
        command: PaperControlCommand,
    ) -> Result<PaperControlResponse> {
        let desc = Self::load_descriptor(root, session_id)?.ok_or_else(|| {
            anyhow!("paper session '{session_id}' has no active control endpoint")
        })?;
        Self::send_with_descriptor(&desc, command, None)
    }

    pub fn send_with_descriptor(
        desc: &PaperControlDescriptor,
        command: PaperControlCommand,
        override_token: Option<&str>,
    ) -> Result<PaperControlResponse> {
        let addr = format!("127.0.0.1:{}", desc.port);
        let mut stream = TcpStream::connect_timeout(
            &addr.parse().context("parse loopback IPC addr")?,
            Duration::from_millis(800),
        )
        .context("connect to paper service loopback IPC")?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;

        let req = PaperControlWireRequest {
            auth_token_hex: override_token.unwrap_or(&desc.auth_token_hex).to_string(),
            command,
        };
        let mut wire = serde_json::to_string(&req)?;
        wire.push('\n');
        stream.write_all(wire.as_bytes())?;
        stream.flush()?;

        let raw = read_bounded_ipc_line(&stream, MAX_IPC_LINE_BYTES)?;
        let resp: PaperControlResponse =
            serde_json::from_str(raw.trim()).context("decode IPC control response")?;
        Ok(resp)
    }

    /// Launches an independent headless child process detached from the GUI lifetime.
    ///
    /// When invoked from the standalone cockpit executable, re-execs itself directly in
    /// hidden `--paper-service-worker` mode without requiring any sibling `hydra.exe`.
    pub fn spawn_detached(
        root: &Path,
        db_path: &Path,
        session_id: &str,
        interval_s: u64,
    ) -> Result<u32> {
        let exe = std::env::current_exe().context("locate executable")?;
        let is_cli_binary = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("hydra"));

        let mut cmd = std::process::Command::new(&exe);
        if is_cli_binary {
            cmd.arg("--no-banner").arg("paper-service").arg("run");
        } else {
            cmd.arg("--paper-service-worker");
        }
        cmd.arg("--root")
            .arg(root)
            .arg("--db")
            .arg(db_path)
            .arg("--session")
            .arg(session_id)
            .arg("--interval-s")
            .arg(interval_s.clamp(2, 60).to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }
        let child = cmd.spawn().context("spawn detached paper engine service")?;
        Ok(child.id())
    }
}

pub struct PaperServiceConfig {
    pub root: PathBuf,
    pub store: PaperStore,
    pub session_id: String,
    pub ws_url: String,
    pub info_url: String,
    pub tick_interval_s: u64,
    pub max_ticks: Option<u64>,
}

/// Runs one independent paper engine session until `stop` becomes `true`, IPC `Stop` arrives,
/// or optional `max_ticks` completes. Recovers prior ledger state from SQLite on startup and
/// shuts down cleanly on `Ctrl+C` or IPC request.
pub async fn run_paper_session_service(
    cfg: PaperServiceConfig,
    external_stop: Arc<AtomicBool>,
) -> Result<PaperControlResponse> {
    let _lock = PaperSessionLock::acquire(&cfg.root, &cfg.session_id)?;
    let pid = std::process::id();
    let initial = cfg
        .store
        .get_session(&cfg.session_id)?
        .ok_or_else(|| anyhow!("paper session '{}' not found", cfg.session_id))?;
    ensure!(
        initial.status != PaperSessionStatus::Archived,
        "cannot run archived paper session '{}'",
        cfg.session_id
    );

    let mut pipeline: SynapsePipeline = cfg.store.load_pipeline(&cfg.session_id)?;
    cfg.store
        .set_status(&cfg.session_id, PaperSessionStatus::Running, Some(pid))?;

    let token = generate_auth_token_hex();
    let listener = TcpListener::bind("127.0.0.1:0").context("bind paper service control port")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let desc = PaperControlDescriptor {
        schema_version: 1,
        session_id: cfg.session_id.clone(),
        pid,
        port,
        auth_token_hex: token.clone(),
        started_at: Utc::now().to_rfc3339(),
    };
    let desc_path = write_control_descriptor(&cfg.root, &desc)?;
    let stop_all = Arc::new(AtomicBool::new(false));
    let _ipc_guard = ControlServerGuard {
        descriptor_file: desc_path,
        stop_ipc: stop_all.clone(),
    };

    let shared_telemetry = Arc::new(RwLock::new(LiveServiceTelemetry {
        session_id: cfg.session_id.clone(),
        status: PaperSessionStatus::Running,
        paused: false,
        cycle_count: pipeline.cycle_count,
        mtm_equity_usd: initial.mtm_equity_usd.round_dp(2).to_string(),
        stream_connected: false,
        stream_reconnects: 0,
        fallback_rest_active: false,
        stale_symbols: Vec::new(),
        watchdog_restarts: 0,
        fault_reason: None,
    }));
    let paused_flag = Arc::new(AtomicBool::new(false));

    // Spawn dedicated authenticated loopback IPC thread.
    let ipc_stop = stop_all.clone();
    let ipc_ext_stop = external_stop.clone();
    let ipc_paused = paused_flag.clone();
    let ipc_state = shared_telemetry.clone();
    let ipc_store = cfg.store.clone();
    let ipc_sid = cfg.session_id.clone();
    let ipc_token = token.clone();
    let ipc_thread = std::thread::Builder::new()
        .name("hydra-paper-ipc".into())
        .spawn(move || {
            while !ipc_stop.load(Ordering::Relaxed) && !ipc_ext_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_ipc_connection(
                            stream,
                            &ipc_token,
                            &ipc_paused,
                            &ipc_stop,
                            &ipc_state,
                            &ipc_store,
                            &ipc_sid,
                            pid,
                        );
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    Err(_) => break,
                }
            }
        })
        .context("spawn paper IPC listener")?;

    // Start independent HUNTER public WS+REST market ingestion task.
    let stream_client = Arc::new(HunterStreamClient::new(
        cfg.ws_url.clone(),
        cfg.info_url.clone(),
        pipeline.config.stale_market_max_age_ms,
    ));
    let held_set = Arc::new(RwLock::new(
        pipeline
            .ledger
            .open_positions
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
    ));
    let wl_vec = Arc::new(RwLock::new(pipeline.watchlist.clone()));
    let stream_stop = stop_all.clone();
    let sc_clone = stream_client.clone();
    let held_clone = held_set.clone();
    let wl_clone = wl_vec.clone();
    let mut stream_task = tokio::spawn(async move {
        let _ = sc_clone
            .run_until_stopped(stream_stop, held_clone, wl_clone)
            .await;
    });

    let tick_dur = Duration::from_secs(cfg.tick_interval_s.clamp(1, 60));
    let mut ticks_done = 0u64;
    let mut watchdog_restarts = 0u64;
    let mut faulted_reason: Option<String> = None;

    while !stop_all.load(Ordering::Relaxed) && !external_stop.load(Ordering::Relaxed) {
        if let Some(max) = cfg.max_ticks {
            if ticks_done >= max {
                break;
            }
        }
        ticks_done += 1;

        if stream_task.is_finished() {
            watchdog_restarts = watchdog_restarts.saturating_add(1);
            let s_stop = stop_all.clone();
            let sc_re = stream_client.clone();
            let h_re = held_set.clone();
            let w_re = wl_vec.clone();
            stream_task = tokio::spawn(async move {
                let _ = sc_re.run_until_stopped(s_stop, h_re, w_re).await;
            });
        }

        let now_ms = Utc::now().timestamp_millis();
        if let Err(e) = cfg.store.record_heartbeat(&cfg.session_id, pid) {
            let reason = format!("heartbeat persistence failed: {e:#}");
            let _ = cfg.store.record_fault(&cfg.session_id, &reason);
            let mut t = shared_telemetry.write().expect("telemetry lock");
            t.status = PaperSessionStatus::Faulted;
            t.fault_reason = Some(reason.clone());
            t.watchdog_restarts = watchdog_restarts;
            faulted_reason = Some(reason);
            stop_all.store(true, Ordering::Relaxed);
            break;
        }

        let is_paused = paused_flag.load(Ordering::Relaxed);
        let (health_snapshot, observed_snaps) = {
            let mut g = stream_client.state.write().expect("stream state lock");
            let wl_snapshot = wl_vec.read().expect("wl lock").clone();
            let held_snapshot = held_set.read().expect("held lock").clone();
            let universe = crate::hunter_stream::HunterMarketState::monitor_universe(
                &wl_snapshot,
                &held_snapshot,
            );
            g.refresh_health_diagnostics(&universe, now_ms);
            let snaps: Vec<_> = g
                .meta_ctx
                .keys()
                .chain(g.l2_books.keys())
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|sym| g.observed_market_snapshot(&sym, now_ms))
                .collect();
            (g.health.clone(), snaps)
        };

        if !is_paused && !observed_snaps.is_empty() {
            // Stage cycle execution in a clone and promote live state ONLY after SQLite commits cleanly.
            let mut staged = pipeline.clone();
            let prev_closed = staged.ledger.closed_trades.len();
            let before_ev_id = staged.bus.total_published();
            let mut marks = BTreeMap::new();
            for s in &observed_snaps {
                if s.freshness.mark_is_fresh() {
                    if let Ok(d) = rust_decimal::Decimal::try_from(s.mark_px) {
                        if d > rust_decimal::Decimal::ZERO {
                            marks.insert(s.symbol.clone(), d.round_dp(6));
                        }
                    }
                }
            }
            let obs_summaries: Vec<_> = observed_snaps
                .iter()
                .map(|s| {
                    let tail_bars: Vec<_> = s
                        .completed_candles
                        .iter()
                        .rev()
                        .take(16)
                        .rev()
                        .map(|c| {
                            serde_json::json!({
                                "ts_ms": c.ts.timestamp_millis(),
                                "o": c.open,
                                "h": c.high,
                                "l": c.low,
                                "c": c.close,
                                "v": c.volume,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "strategy_version": crate::paper_store::PAPER_STRATEGY_VERSION,
                        "symbol": s.symbol,
                        "mark_px": s.mark_px,
                        "best_bid": s.best_bid,
                        "best_ask": s.best_ask,
                        "spread_bps": s.spread_bps,
                        "depth_usd": s.depth_usd,
                        "volume_24h_usd": s.volume_24h_usd,
                        "open_interest_usd": s.open_interest_usd,
                        "funding_bps_1h": s.funding_bps_1h,
                        "volatility_pct": s.volatility_pct,
                        "directionality": s.directionality,
                        "coverage_fraction": s.coverage_fraction,
                        "observed_history_minutes": s.observed_history_minutes,
                        "instrument_listing_age_days": s.instrument_listing_age_days,
                        "freshness": s.freshness,
                        "is_stale": s.is_stale,
                        "received_at_ms": s.received_at_ms,
                        "missing_fields": s.missing_fields,
                        "candle_tail_16": tail_bars,
                    })
                })
                .collect();
            let obs_json = match serde_json::to_string(&obs_summaries) {
                Ok(j) => j,
                Err(e) => {
                    let reason = format!("serialize paper observation payload failed: {e:#}");
                    let _ = cfg.store.record_fault(&cfg.session_id, &reason);
                    let mut t = shared_telemetry.write().expect("telemetry lock");
                    t.status = PaperSessionStatus::Faulted;
                    t.fault_reason = Some(reason.clone());
                    faulted_reason = Some(reason);
                    stop_all.store(true, Ordering::Relaxed);
                    break;
                }
            };

            match staged.step_cycle_from_observed(now_ms, &observed_snaps) {
                Ok(sum) => {
                    let new_events: Vec<_> = staged
                        .bus
                        .all_chronological()
                        .filter(|ev| ev.event_id > before_ev_id)
                        .cloned()
                        .collect();
                    match cfg.store.commit_cycle(
                        &cfg.session_id,
                        &staged,
                        &sum,
                        prev_closed,
                        &new_events,
                        &marks,
                        DataProvenance::AuthoritativeExchange,
                        Some(&obs_json),
                    ) {
                        Ok(_) => {
                            pipeline = staged;
                            *held_set.write().expect("held lock") =
                                pipeline.ledger.open_positions.keys().cloned().collect();
                            *wl_vec.write().expect("wl lock") = pipeline.watchlist.clone();
                            let mut t = shared_telemetry.write().expect("telemetry lock");
                            t.cycle_count = sum.cycle;
                            t.mtm_equity_usd = sum.mtm_equity_usd.round_dp(2).to_string();
                        }
                        Err(e) => {
                            let reason = format!("transactional commit_cycle failed: {e:#}");
                            let _ = cfg.store.record_fault(&cfg.session_id, &reason);
                            let mut t = shared_telemetry.write().expect("telemetry lock");
                            t.status = PaperSessionStatus::Faulted;
                            t.fault_reason = Some(reason.clone());
                            t.watchdog_restarts = watchdog_restarts;
                            faulted_reason = Some(reason);
                            stop_all.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                Err(e) => {
                    let reason = format!("pipeline cycle failed: {e:#}");
                    let _ = cfg.store.record_fault(&cfg.session_id, &reason);
                    let mut t = shared_telemetry.write().expect("telemetry lock");
                    t.status = PaperSessionStatus::Faulted;
                    t.fault_reason = Some(reason.clone());
                    t.watchdog_restarts = watchdog_restarts;
                    faulted_reason = Some(reason);
                    stop_all.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        {
            let mut t = shared_telemetry.write().expect("telemetry lock");
            t.paused = is_paused;
            t.status = if is_paused {
                PaperSessionStatus::Paused
            } else {
                PaperSessionStatus::Running
            };
            t.stream_connected = health_snapshot.connected;
            t.stream_reconnects = health_snapshot.reconnect_count;
            t.fallback_rest_active = health_snapshot.fallback_rest_active;
            t.stale_symbols = health_snapshot.stale_symbols;
            t.watchdog_restarts = watchdog_restarts;
        }

        let sleep_step = Duration::from_millis(120);
        let mut waited = Duration::ZERO;
        while waited < tick_dur
            && !stop_all.load(Ordering::Relaxed)
            && !external_stop.load(Ordering::Relaxed)
        {
            tokio::time::sleep(sleep_step).await;
            waited += sleep_step;
        }
    }

    stop_all.store(true, Ordering::Relaxed);
    let _ = tokio::time::timeout(Duration::from_secs(2), stream_task).await;
    let _ = ipc_thread.join();
    if let Some(reason) = faulted_reason {
        bail!("{reason}");
    }
    cfg.store
        .set_status(&cfg.session_id, PaperSessionStatus::Stopped, None)?;

    let g = shared_telemetry.read().expect("telemetry lock");
    Ok(PaperControlResponse {
        ok: true,
        error: None,
        session_id: g.session_id.clone(),
        status: PaperSessionStatus::Stopped.as_str().into(),
        paused: g.paused,
        cycle_count: g.cycle_count,
        mtm_equity_usd: g.mtm_equity_usd.clone(),
        stream_connected: g.stream_connected,
        stream_reconnects: g.stream_reconnects,
        fallback_rest_active: g.fallback_rest_active,
        stale_symbols: g.stale_symbols.clone(),
        watchdog_restarts: g.watchdog_restarts,
        fault_reason: g.fault_reason.clone(),
        pid,
        checked_at: Utc::now().to_rfc3339(),
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_ipc_connection(
    mut stream: TcpStream,
    expected_token: &str,
    paused_flag: &Arc<AtomicBool>,
    stop_flag: &Arc<AtomicBool>,
    shared: &Arc<RwLock<LiveServiceTelemetry>>,
    store: &PaperStore,
    session_id: &str,
    pid: u32,
) -> Result<()> {
    // Windows inherits non-blocking socket state from the listening socket; restore
    // blocking mode before applying read/write timeouts to prevent WSAEWOULDBLOCK / 10054 RST.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let line = read_bounded_ipc_line(&stream, MAX_IPC_LINE_BYTES)?;
    let req: PaperControlWireRequest =
        serde_json::from_str(line.trim()).context("malformed IPC control request")?;
    let g = shared.read().expect("telemetry lock").clone();
    if !constant_time_eq(&req.auth_token_hex, expected_token) {
        let resp = PaperControlResponse {
            ok: false,
            error: Some("unauthorized IPC token".into()),
            session_id: g.session_id,
            status: g.status.as_str().into(),
            paused: g.paused,
            cycle_count: g.cycle_count,
            mtm_equity_usd: g.mtm_equity_usd,
            stream_connected: g.stream_connected,
            stream_reconnects: g.stream_reconnects,
            fallback_rest_active: g.fallback_rest_active,
            stale_symbols: g.stale_symbols,
            watchdog_restarts: g.watchdog_restarts,
            fault_reason: g.fault_reason,
            pid,
            checked_at: Utc::now().to_rfc3339(),
        };
        let mut out = serde_json::to_string(&resp)?;
        out.push('\n');
        stream.write_all(out.as_bytes())?;
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        return Ok(());
    }

    let db_res = match req.command {
        PaperControlCommand::Status | PaperControlCommand::Heartbeat => {
            store.record_heartbeat(session_id, pid)
        }
        PaperControlCommand::Pause => {
            let res = store.set_status(session_id, PaperSessionStatus::Paused, Some(pid));
            if res.is_ok() {
                paused_flag.store(true, Ordering::Relaxed);
            }
            res
        }
        PaperControlCommand::Resume => {
            let res = store.set_status(session_id, PaperSessionStatus::Running, Some(pid));
            if res.is_ok() {
                paused_flag.store(false, Ordering::Relaxed);
            }
            res
        }
        PaperControlCommand::Stop => {
            stop_flag.store(true, Ordering::Relaxed);
            Ok(())
        }
    };

    let g2 = shared.read().expect("telemetry lock").clone();
    let (ok, error) = match db_res {
        Ok(()) => (true, None),
        Err(e) => (false, Some(format!("paper store error: {e:#}"))),
    };
    let resp = PaperControlResponse {
        ok,
        error,
        session_id: g2.session_id,
        status: match req.command {
            PaperControlCommand::Pause if ok => PaperSessionStatus::Paused.as_str(),
            PaperControlCommand::Resume if ok => PaperSessionStatus::Running.as_str(),
            PaperControlCommand::Stop => "stopping",
            _ => g2.status.as_str(),
        }
        .into(),
        paused: paused_flag.load(Ordering::Relaxed),
        cycle_count: g2.cycle_count,
        mtm_equity_usd: g2.mtm_equity_usd,
        stream_connected: g2.stream_connected,
        stream_reconnects: g2.stream_reconnects,
        fallback_rest_active: g2.fallback_rest_active,
        stale_symbols: g2.stale_symbols,
        watchdog_restarts: g2.watchdog_restarts,
        fault_reason: g2.fault_reason,
        pid,
        checked_at: Utc::now().to_rfc3339(),
    };
    let mut out = serde_json::to_string(&resp)?;
    out.push('\n');
    stream.write_all(out.as_bytes())?;
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_pipeline::SynapsePipelineConfig;
    use crate::paper_store::PaperSessionKind;

    #[test]
    fn exclusive_lock_prevents_duplicate_paper_session_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let lock1 = PaperSessionLock::acquire(tmp.path(), "public_paper-abc123").unwrap();
        assert!(PaperSessionLock::acquire(tmp.path(), "public_paper-abc123").is_err());
        drop(lock1);
        assert!(PaperSessionLock::acquire(tmp.path(), "public_paper-abc123").is_ok());
    }

    #[tokio::test]
    async fn authenticated_ipc_rejects_bad_token_and_controls_lifecycle() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PaperStore::open(tmp.path().join("paper_sessions.db")).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::PublicPaper,
                "IPC Test",
                SynapsePipelineConfig::default(),
            )
            .unwrap();

        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let rest_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rest_port = rest_listener.local_addr().unwrap().port();
        drop(rest_listener);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_svc = stop.clone();
        let root_c = tmp.path().to_path_buf();
        let store_c = store.clone();
        let sid = sess.session_id.clone();

        let handle = tokio::spawn(async move {
            run_paper_session_service(
                PaperServiceConfig {
                    root: root_c,
                    store: store_c,
                    session_id: sid,
                    ws_url: format!("ws://127.0.0.1:{ws_port}"),
                    info_url: format!("http://127.0.0.1:{rest_port}/info"),
                    tick_interval_s: 1,
                    max_ticks: None,
                },
                stop_svc,
            )
            .await
        });

        let mut desc = None;
        for _ in 0..40 {
            if let Ok(Some(d)) = PaperServiceClient::load_descriptor(tmp.path(), &sess.session_id) {
                desc = Some(d);
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        let desc = desc.expect("control descriptor written");

        // Reject wrong auth token
        let unauth = PaperServiceClient::send_with_descriptor(
            &desc,
            PaperControlCommand::Stop,
            Some("00000000deadbeef"),
        )
        .unwrap();
        assert!(!unauth.ok);

        // Authorized pause -> resume -> stop
        let paused = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Pause,
        )
        .unwrap();
        assert!(paused.ok && paused.paused);

        let resumed = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Resume,
        )
        .unwrap();
        assert!(resumed.ok && !resumed.paused);

        let stopped = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Stop,
        )
        .unwrap();
        assert!(stopped.ok);

        let res = handle.await.unwrap().unwrap();
        assert_eq!(res.status, "stopped");
        // Control descriptor cleaned up after shutdown
        assert!(
            PaperServiceClient::load_descriptor(tmp.path(), &sess.session_id)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn oversized_ipc_frame_and_held_lock_guards_work_without_blocking() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PaperStore::open(tmp.path().join("paper_sessions.db")).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::PublicPaper,
                "IPC Security & Lock Test",
                SynapsePipelineConfig::default(),
            )
            .unwrap();

        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let rest_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rest_port = rest_listener.local_addr().unwrap().port();
        drop(rest_listener);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_svc = stop.clone();
        let root_c = tmp.path().to_path_buf();
        let store_c = store.clone();
        let sid = sess.session_id.clone();

        let handle = tokio::spawn(async move {
            run_paper_session_service(
                PaperServiceConfig {
                    root: root_c,
                    store: store_c,
                    session_id: sid,
                    ws_url: format!("ws://127.0.0.1:{ws_port}"),
                    info_url: format!("http://127.0.0.1:{rest_port}/info"),
                    tick_interval_s: 1,
                    max_ticks: None,
                },
                stop_svc,
            )
            .await
        });

        let mut desc = None;
        for _ in 0..40 {
            if let Ok(Some(d)) = PaperServiceClient::load_descriptor(tmp.path(), &sess.session_id) {
                desc = Some(d);
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        let desc = desc.expect("control descriptor written");

        // Send an oversized frame > MAX_IPC_LINE_BYTES (16 KiB) directly over TCP.
        {
            let mut raw_stream = TcpStream::connect(format!("127.0.0.1:{}", desc.port)).unwrap();
            raw_stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let oversized = "X".repeat(MAX_IPC_LINE_BYTES + 2048);
            let _ = raw_stream.write_all(oversized.as_bytes());
            let _ = raw_stream.write_all(b"\n");
        }

        // Held OS lock + guarded archive/reset must fail closed even when paused.
        let paused = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Pause,
        )
        .unwrap();
        assert!(paused.ok && paused.paused);
        assert!(store
            .archive_session_guarded(tmp.path(), &sess.session_id)
            .is_err());
        assert!(store
            .reset_session_confirmed_guarded(
                tmp.path(),
                &sess.session_id,
                &format!("CONFIRM_RESET:{}", sess.session_id)
            )
            .is_err());

        // Valid IPC Status + Stop still succeed after the oversized frame.
        let status = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Status,
        )
        .unwrap();
        assert!(status.ok);
        let stopped = PaperServiceClient::send_command(
            tmp.path(),
            &sess.session_id,
            PaperControlCommand::Stop,
        )
        .unwrap();
        assert!(stopped.ok);
        assert_eq!(handle.await.unwrap().unwrap().status, "stopped");

        // Once stopped and lock released, guarded archive succeeds cleanly.
        store
            .archive_session_guarded(tmp.path(), &sess.session_id)
            .unwrap();
    }

    #[tokio::test]
    async fn db_fault_during_service_tick_halts_cleanly_as_faulted() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("paper_fault_svc.db");
        let store = PaperStore::open(&db_path).unwrap();
        let sess = store
            .create_session(
                PaperSessionKind::PublicPaper,
                "Fault Injection Service",
                SynapsePipelineConfig::default(),
            )
            .unwrap();

        // Inject SQLite trigger on heartbeat update after status enters running, simulating disk/constraint fault.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TRIGGER fail_heartbeat BEFORE UPDATE OF heartbeat_at ON paper_sessions WHEN NEW.heartbeat_at IS NOT NULL AND OLD.status = 'running' BEGIN SELECT RAISE(ABORT, 'injected heartbeat disk fault'); END;",
            )
            .unwrap();
        }

        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let rest_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rest_port = rest_listener.local_addr().unwrap().port();
        drop(rest_listener);

        let res = run_paper_session_service(
            PaperServiceConfig {
                root: tmp.path().to_path_buf(),
                store: store.clone(),
                session_id: sess.session_id.clone(),
                ws_url: format!("ws://127.0.0.1:{ws_port}"),
                info_url: format!("http://127.0.0.1:{rest_port}/info"),
                tick_interval_s: 1,
                max_ticks: Some(3),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await;
        assert!(res.is_err());

        let persisted = store.get_session(&sess.session_id).unwrap().unwrap();
        assert_eq!(persisted.status, PaperSessionStatus::Faulted);
        assert!(persisted
            .fault_reason
            .as_deref()
            .is_some_and(|r| r.contains("heartbeat persistence failed")));
    }
}

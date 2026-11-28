//! Authenticated Local Control IPC for the Live Trading Engine (`v1.16.0 | PROJECT CONVERGENCE`).
//!
//! Security & privilege-boundary invariants:
//! - Binds strictly to localhost (`127.0.0.1`) with an OS-scoped control descriptor containing a
//!   256-bit secret token, PID, and endpoint port (`chmod 0600` on Unix; best-effort user ACL on
//!   Windows).
//! - Accepted sockets explicitly restore blocking mode (`set_nonblocking(false)`) before applying
//!   timeouts and bounded chunk reading (`MAX_ENGINE_IPC_BYTES`), preventing Windows 10054 RSTs on
//!   fragmented/concurrent traffic and allowing clean Write-half shutdown after flushing responses.
//! - Constant-time token verification + strictly increasing `nonce` replay protection + timestamp
//!   freshness check.
//! - Unauthorized, malformed, oversized, or timed-out requests NEVER report healthy/dormant status:
//!   they fail closed with `mode: "UNKNOWN"`, `protective_stops_verified: false`, and
//!   `maintenance_ready: false`.
//! - Binds to the real `EngineControlAuthority` (mode, emergency suspension, stop flag, breaker
//!   state, and optional durable SQLite `meta_kv` state + JSONL audit log).

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MAX_ENGINE_IPC_BYTES: u64 = 16 * 1024;
pub const MAX_IPC_CLOCK_SKEW_MS: i64 = 30_000;
pub const MAX_CONCURRENT_IPC_CLIENTS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineControlDescriptor {
    pub schema_version: u32,
    pub engine_id: String,
    pub pid: u32,
    pub port: u16,
    pub token_hex: String,
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineControlCommand {
    Heartbeat,
    GetState,
    RequestModeTransition {
        target_mode: String,
        reason: String,
    },
    ValidatePolicy,
    EmergencySuspend {
        reason: String,
    },
    AuditedBreakerRearm {
        operator_id: String,
        reason: String,
        reset_equity_peak: bool,
    },
    GetPositionProtection,
    MaintenanceReadiness,
    GracefulShutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedEngineRequest {
    pub token_hex: String,
    pub nonce: u64,
    pub issued_at_ms: i64,
    pub command: EngineControlCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineControlResponse {
    pub ok: bool,
    pub engine_id: String,
    pub pid: u32,
    pub mode: String,
    pub emergency_suspended: bool,
    pub breaker_latched: bool,
    pub open_positions: usize,
    pub unresolved_intents: usize,
    pub protective_stops_verified: bool,
    pub maintenance_ready: bool,
    pub message: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone)]
pub struct EngineAuthoritativeState {
    pub engine_id: String,
    pub mode: String,
    pub emergency_suspended: bool,
    pub breaker_latched: bool,
    pub open_positions: usize,
    pub unresolved_intents: usize,
    pub protective_stops_verified: bool,
    pub shutdown_requested: bool,
}

impl EngineAuthoritativeState {
    pub fn dormant(engine_id: impl Into<String>) -> Self {
        Self {
            engine_id: engine_id.into(),
            mode: "DORMANT".into(),
            emergency_suspended: false,
            breaker_latched: false,
            open_positions: 0,
            unresolved_intents: 0,
            protective_stops_verified: true,
            shutdown_requested: false,
        }
    }

    pub fn is_maintenance_ready(&self) -> bool {
        !matches!(self.mode.as_str(), "AWAKENED" | "GUARDIAN")
            && self.open_positions == 0
            && self.unresolved_intents == 0
    }
}

/// Shared authoritative handle connecting the running `Engine` loop, persistent SQLite
/// `Storage` metadata, and the authenticated `EngineIpcServer`.
#[derive(Clone)]
pub struct EngineControlAuthority {
    pub state: Arc<Mutex<EngineAuthoritativeState>>,
    pub stop_flag: Arc<AtomicBool>,
    pub storage: Option<crate::storage::Storage>,
}

impl EngineControlAuthority {
    pub fn new(
        engine_id: impl Into<String>,
        stop_flag: Arc<AtomicBool>,
        storage: Option<crate::storage::Storage>,
    ) -> Self {
        let mut st = EngineAuthoritativeState::dormant(engine_id);
        if let Some(ref store) = storage {
            let saved = store.with_conn(|conn| {
                let mode: Option<String> = conn
                    .query_row(
                        "SELECT value FROM meta_kv WHERE key='engine_mode'",
                        [],
                        |r| r.get(0),
                    )
                    .ok();
                let susp: Option<String> = conn
                    .query_row(
                        "SELECT value FROM meta_kv WHERE key='engine_emergency_suspended'",
                        [],
                        |r| r.get(0),
                    )
                    .ok();
                (mode, susp)
            });
            if saved.1.as_deref() == Some("true") {
                st.emergency_suspended = true;
                st.mode = "SUSPENDED".into();
            } else if let Some(m) = saved.0 {
                // Fail-closed: never auto-resume live execution on startup.
                if matches!(
                    m.as_str(),
                    "SUSPENDED" | "OBSERVATION" | "SIMULATION" | "DORMANT"
                ) {
                    st.mode = m;
                }
            }
        }
        Self {
            state: Arc::new(Mutex::new(st)),
            stop_flag,
            storage,
        }
    }

    pub fn snapshot(&self) -> EngineAuthoritativeState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_emergency_suspended(&self) -> bool {
        self.state.lock().unwrap().emergency_suspended
    }

    pub fn update_runtime_status(
        &self,
        open_positions: usize,
        unresolved_intents: usize,
        protective_stops_verified: bool,
        breaker_latched: bool,
    ) {
        let mut st = self.state.lock().unwrap();
        st.open_positions = open_positions;
        st.unresolved_intents = unresolved_intents;
        st.protective_stops_verified = protective_stops_verified;
        st.breaker_latched = breaker_latched;
        if st.shutdown_requested {
            self.stop_flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn persist_state(&self) {
        let Some(ref store) = self.storage else {
            return;
        };
        let st = self.state.lock().unwrap().clone();
        let now = Utc::now().to_rfc3339();
        let _ = store.with_conn(|conn| -> Result<()> {
            conn.execute(
                "INSERT INTO meta_kv(key,value,updated_at) VALUES('engine_mode',?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![st.mode, now],
            )?;
            conn.execute(
                "INSERT INTO meta_kv(key,value,updated_at) VALUES('engine_emergency_suspended',?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![if st.emergency_suspended { "true" } else { "false" }, now],
            )?;
            conn.execute(
                "INSERT INTO meta_kv(key,value,updated_at) VALUES('engine_breaker_latched',?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![if st.breaker_latched { "true" } else { "false" }, now],
            )?;
            Ok(())
        });
    }
}

pub fn generate_token_hex(engine_id: &str, pid: u32) -> String {
    let mut h = Sha3_256::new();
    h.update(engine_id.as_bytes());
    h.update(pid.to_le_bytes());
    h.update(Utc::now().timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
    h.update(uuid::Uuid::new_v4().as_bytes());
    hex::encode(h.finalize())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn descriptor_path(root: &Path, engine_id: &str) -> PathBuf {
    root.join("engine-control")
        .join(format!("{engine_id}.json"))
}

fn audit_log_path(root: &Path, engine_id: &str) -> PathBuf {
    root.join("engine-control")
        .join(format!("{engine_id}-audit.jsonl"))
}

fn append_security_audit(root: &Path, engine_id: &str, ok: bool, event: &str) {
    let path = audit_log_path(root, engine_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let row = serde_json::json!({
            "timestamp_ms": Utc::now().timestamp_millis(),
            "engine_id": engine_id,
            "ok": ok,
            "event": event,
        });
        let _ = writeln!(f, "{row}");
    }
}

#[cfg(windows)]
fn restrict_windows_owner_acl(path: &Path, is_dir: bool) {
    if let Ok(user) = std::env::var("USERNAME") {
        let trimmed = user.trim();
        if !trimmed.is_empty() {
            let grant = if is_dir {
                format!("{trimmed}:(OI)(CI)(F)")
            } else {
                format!("{trimmed}:(F)")
            };
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .arg("/inheritance:r")
                .arg("/grant:r")
                .arg(&grant)
                .output();
        }
    }
}

pub fn write_descriptor(root: &Path, desc: &EngineControlDescriptor) -> Result<PathBuf> {
    let dir = root.join("engine-control");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    restrict_windows_owner_acl(&dir, true);

    let path = descriptor_path(root, &desc.engine_id);
    let tmp = path.with_extension("tmp");
    let raw = serde_json::to_vec_pretty(desc)?;
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
        f.write_all(&raw)?;
        f.sync_all()?;
    }
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&tmp, &path)?;
    #[cfg(windows)]
    restrict_windows_owner_acl(&path, false);
    Ok(path)
}

/// Fail-closed response used for unauthenticated, malformed, oversized, or timed-out IPC errors.
/// Never claims `DORMANT`, `protective_stops_verified: true`, or `maintenance_ready: true`.
fn fail_closed_error_response(
    engine_id: &str,
    pid: u32,
    msg: String,
    now_ms: i64,
) -> EngineControlResponse {
    EngineControlResponse {
        ok: false,
        engine_id: engine_id.into(),
        pid,
        mode: "UNKNOWN".into(),
        emergency_suspended: true,
        breaker_latched: true,
        open_positions: 0,
        unresolved_intents: 0,
        protective_stops_verified: false,
        maintenance_ready: false,
        message: msg,
        timestamp_ms: now_ms,
    }
}

/// Evaluates an authenticated request against the engine's authoritative state and monotonic nonce tracker.
pub fn handle_authenticated_request(
    expected_token_hex: &str,
    nonce_tracker: &mut BTreeMap<String, u64>,
    state: &mut EngineAuthoritativeState,
    req: &AuthenticatedEngineRequest,
    now_ms: i64,
) -> EngineControlResponse {
    let pid = std::process::id();
    let mk_resp = |ok: bool, msg: String, st: &EngineAuthoritativeState| EngineControlResponse {
        ok,
        engine_id: st.engine_id.clone(),
        pid,
        mode: st.mode.clone(),
        emergency_suspended: st.emergency_suspended,
        breaker_latched: st.breaker_latched,
        open_positions: st.open_positions,
        unresolved_intents: st.unresolved_intents,
        protective_stops_verified: st.protective_stops_verified,
        maintenance_ready: st.is_maintenance_ready(),
        message: msg,
        timestamp_ms: now_ms,
    };

    if !constant_time_eq(expected_token_hex.as_bytes(), req.token_hex.as_bytes()) {
        return fail_closed_error_response(
            &state.engine_id,
            pid,
            "Unauthorized engine IPC token".into(),
            now_ms,
        );
    }
    if (now_ms - req.issued_at_ms).abs() > MAX_IPC_CLOCK_SKEW_MS {
        return fail_closed_error_response(
            &state.engine_id,
            pid,
            "Stale IPC timestamp outside freshness window".into(),
            now_ms,
        );
    }
    let last_nonce = nonce_tracker.get(&state.engine_id).copied().unwrap_or(0);
    if req.nonce <= last_nonce {
        return fail_closed_error_response(
            &state.engine_id,
            pid,
            "Replayed or non-monotonic IPC nonce rejected".into(),
            now_ms,
        );
    }
    nonce_tracker.insert(state.engine_id.clone(), req.nonce);

    match &req.command {
        EngineControlCommand::Heartbeat => mk_resp(true, "heartbeat ok".into(), state),
        EngineControlCommand::GetState => mk_resp(true, "authoritative state".into(), state),
        EngineControlCommand::ValidatePolicy => mk_resp(
            true,
            format!(
                "certification={}",
                crate::live_gate::certification_summary()
            ),
            state,
        ),
        EngineControlCommand::RequestModeTransition {
            target_mode,
            reason,
        } => {
            let norm = target_mode.trim().to_ascii_uppercase();
            if matches!(norm.as_str(), "AWAKENED" | "GUARDIAN") {
                return mk_resp(
                    false,
                    format!(
                        "Mode '{norm}' refused: live execution blocked by PendingCertification ({reason})"
                    ),
                    state,
                );
            }
            if state.emergency_suspended && norm != "SUSPENDED" {
                return mk_resp(
                    false,
                    "Emergency suspension active; cannot transition out of SUSPENDED".into(),
                    state,
                );
            }
            if matches!(
                norm.as_str(),
                "DORMANT" | "OBSERVATION" | "SIMULATION" | "SUSPENDED"
            ) {
                state.mode = norm.clone();
                mk_resp(true, format!("Transitioned to {norm}: {reason}"), state)
            } else {
                mk_resp(
                    false,
                    format!("Unsupported operational mode '{target_mode}'"),
                    state,
                )
            }
        }
        EngineControlCommand::EmergencySuspend { reason } => {
            state.emergency_suspended = true;
            state.mode = "SUSPENDED".into();
            mk_resp(true, format!("Emergency suspended: {reason}"), state)
        }
        EngineControlCommand::AuditedBreakerRearm {
            operator_id,
            reason,
            ..
        } => {
            if operator_id.trim().is_empty() || reason.trim().is_empty() {
                return mk_resp(
                    false,
                    "Operator ID and reason required for rearm".into(),
                    state,
                );
            }
            if state.emergency_suspended {
                return mk_resp(
                    false,
                    "Cannot rearm breaker during emergency suspension".into(),
                    state,
                );
            }
            state.breaker_latched = false;
            mk_resp(true, format!("Breaker rearmed by {operator_id}"), state)
        }
        EngineControlCommand::GetPositionProtection => {
            mk_resp(true, "protective status".into(), state)
        }
        EngineControlCommand::MaintenanceReadiness => mk_resp(
            true,
            format!("maintenance_ready={}", state.is_maintenance_ready()),
            state,
        ),
        EngineControlCommand::GracefulShutdown => {
            state.shutdown_requested = true;
            mk_resp(true, "Graceful shutdown acknowledged".into(), state)
        }
    }
}

enum BoundedReadOutcome {
    Line(String),
    Oversized,
    TimedOut,
    Malformed,
}

fn read_bounded_ipc_line(stream: &mut TcpStream) -> BoundedReadOutcome {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => {
                if buf.is_empty() {
                    return BoundedReadOutcome::Malformed;
                }
                break;
            }
            Ok(n) => {
                for &b in &chunk[..n] {
                    if buf.len() as u64 >= MAX_ENGINE_IPC_BYTES {
                        return BoundedReadOutcome::Oversized;
                    }
                    buf.push(b);
                    if b == b'\n' {
                        return match String::from_utf8(buf) {
                            Ok(s) => BoundedReadOutcome::Line(s),
                            Err(_) => BoundedReadOutcome::Malformed,
                        };
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return BoundedReadOutcome::TimedOut;
            }
            Err(_) => return BoundedReadOutcome::Malformed,
        }
    }
    match String::from_utf8(buf) {
        Ok(s) => BoundedReadOutcome::Line(s),
        Err(_) => BoundedReadOutcome::Malformed,
    }
}

pub struct EngineIpcServer {
    pub descriptor: EngineControlDescriptor,
    pub state: Arc<Mutex<EngineAuthoritativeState>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EngineIpcServer {
    pub fn start(root: &Path, engine_id: &str) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let authority = EngineControlAuthority::new(engine_id, stop, None);
        Self::start_with_authority(root, authority)
    }

    pub fn start_with_authority(root: &Path, authority: EngineControlAuthority) -> Result<Self> {
        let engine_id = authority.snapshot().engine_id;
        let listener = TcpListener::bind("127.0.0.1:0").context("bind engine IPC listener")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let pid = std::process::id();
        let desc = EngineControlDescriptor {
            schema_version: 1,
            engine_id: engine_id.clone(),
            pid,
            port,
            token_hex: generate_token_hex(&engine_id, pid),
            started_at_ms: Utc::now().timestamp_millis(),
        };
        write_descriptor(root, &desc)?;

        let state = authority.state.clone();
        let stop = authority.stop_flag.clone();
        let stop_clone = stop.clone();
        let nonces = Arc::new(Mutex::new(BTreeMap::new()));
        let active_clients = Arc::new(AtomicUsize::new(0));
        let token = desc.token_hex.clone();
        let audit_root = root.to_path_buf();

        let handle = std::thread::Builder::new()
            .name(format!("hydra-engine-ipc-{engine_id}"))
            .spawn(move || {
                while !stop_clone.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // CRITICAL (Windows 10054 fix): clear inherited non-blocking mode!
                            let _ = stream.set_nonblocking(false);
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

                            let prev = active_clients.fetch_add(1, Ordering::SeqCst);
                            if prev >= MAX_CONCURRENT_IPC_CLIENTS {
                                active_clients.fetch_sub(1, Ordering::SeqCst);
                                let resp = fail_closed_error_response(
                                    &engine_id,
                                    pid,
                                    "Engine IPC concurrency ceiling exceeded".into(),
                                    Utc::now().timestamp_millis(),
                                );
                                if let Ok(raw) = serde_json::to_string(&resp) {
                                    let _ = stream.write_all(raw.as_bytes());
                                    let _ = stream.write_all(b"\n");
                                    let _ = stream.flush();
                                    let _ = stream.shutdown(Shutdown::Write);
                                }
                                continue;
                            }

                            let auth_c = authority.clone();
                            let nonces_c = nonces.clone();
                            let token_c = token.clone();
                            let eid_c = engine_id.clone();
                            let aroot_c = audit_root.clone();
                            let stop_c = stop_clone.clone();
                            let active_c = active_clients.clone();

                            std::thread::spawn(move || {
                                let resp = match read_bounded_ipc_line(&mut stream) {
                                    BoundedReadOutcome::Oversized => {
                                        append_security_audit(&aroot_c, &eid_c, false, "oversized_frame");
                                        fail_closed_error_response(
                                            &eid_c,
                                            pid,
                                            "Oversized engine IPC request rejected".into(),
                                            Utc::now().timestamp_millis(),
                                        )
                                    }
                                    BoundedReadOutcome::TimedOut => {
                                        append_security_audit(&aroot_c, &eid_c, false, "read_timeout");
                                        fail_closed_error_response(
                                            &eid_c,
                                            pid,
                                            "Engine IPC request timed out".into(),
                                            Utc::now().timestamp_millis(),
                                        )
                                    }
                                    BoundedReadOutcome::Malformed => {
                                        append_security_audit(&aroot_c, &eid_c, false, "malformed_frame");
                                        fail_closed_error_response(
                                            &eid_c,
                                            pid,
                                            "Malformed engine IPC JSON".into(),
                                            Utc::now().timestamp_millis(),
                                        )
                                    }
                                    BoundedReadOutcome::Line(line) => {
                                        match serde_json::from_str::<AuthenticatedEngineRequest>(
                                            line.trim(),
                                        ) {
                                            Ok(req) => {
                                                let mut ng = nonces_c.lock().unwrap();
                                                let mut st = auth_c.state.lock().unwrap();
                                                let r = handle_authenticated_request(
                                                    &token_c,
                                                    &mut ng,
                                                    &mut st,
                                                    &req,
                                                    Utc::now().timestamp_millis(),
                                                );
                                                let need_persist = matches!(
                                                    req.command,
                                                    EngineControlCommand::EmergencySuspend { .. }
                                                        | EngineControlCommand::RequestModeTransition { .. }
                                                        | EngineControlCommand::AuditedBreakerRearm { .. }
                                                );
                                                let shut = st.shutdown_requested;
                                                drop(st);
                                                drop(ng);
                                                if need_persist {
                                                    auth_c.persist_state();
                                                }
                                                append_security_audit(
                                                    &aroot_c,
                                                    &eid_c,
                                                    r.ok,
                                                    &r.message,
                                                );
                                                if shut {
                                                    stop_c.store(true, Ordering::SeqCst);
                                                }
                                                r
                                            }
                                            Err(e) => {
                                                append_security_audit(
                                                    &aroot_c,
                                                    &eid_c,
                                                    false,
                                                    "malformed_json",
                                                );
                                                fail_closed_error_response(
                                                    &eid_c,
                                                    pid,
                                                    format!("Malformed engine IPC JSON: {e}"),
                                                    Utc::now().timestamp_millis(),
                                                )
                                            }
                                        }
                                    }
                                };
                                if let Ok(raw) = serde_json::to_string(&resp) {
                                    let _ = stream.write_all(raw.as_bytes());
                                    let _ = stream.write_all(b"\n");
                                    let _ = stream.flush();
                                    let _ = stream.shutdown(Shutdown::Write);
                                }
                                active_c.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                        Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            })?;

        Ok(Self {
            descriptor: desc,
            state,
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for EngineIpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct EngineIpcClient;

impl EngineIpcClient {
    pub fn send_command(
        root: &Path,
        engine_id: &str,
        nonce: u64,
        command: EngineControlCommand,
    ) -> Result<EngineControlResponse> {
        let p = descriptor_path(root, engine_id);
        let desc: EngineControlDescriptor = serde_json::from_str(&fs::read_to_string(&p)?)?;
        let req = AuthenticatedEngineRequest {
            token_hex: desc.token_hex,
            nonce,
            issued_at_ms: Utc::now().timestamp_millis(),
            command,
        };
        Self::send_raw(desc.port, &req)
    }

    pub fn send_raw(port: u16, req: &AuthenticatedEngineRequest) -> Result<EngineControlResponse> {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let raw = serde_json::to_string(req)?;
        stream.write_all(raw.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut limited = BufReader::new(stream).take(MAX_ENGINE_IPC_BYTES + 1);
        let mut line = String::new();
        let n = limited.read_line(&mut line)?;
        ensure!(
            (n as u64) <= MAX_ENGINE_IPC_BYTES,
            "response exceeded MAX_ENGINE_IPC_BYTES"
        );
        ensure!(!line.trim().is_empty(), "empty IPC response");
        Ok(serde_json::from_str(line.trim())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_engine_ipc_enforces_tokens_nonces_oversized_bounds_and_live_mode_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let mut srv = EngineIpcServer::start(dir.path(), "main-engine").unwrap();
        let port = srv.descriptor.port;

        // 1. Wrong token rejected AND fails closed (never reports healthy/ready)
        let bad = AuthenticatedEngineRequest {
            token_hex: "0000".into(),
            nonce: 1,
            issued_at_ms: Utc::now().timestamp_millis(),
            command: EngineControlCommand::Heartbeat,
        };
        let r_bad = EngineIpcClient::send_raw(port, &bad).unwrap();
        assert!(!r_bad.ok);
        assert!(r_bad.message.contains("Unauthorized"));
        assert_eq!(r_bad.mode, "UNKNOWN");
        assert!(!r_bad.protective_stops_verified);
        assert!(!r_bad.maintenance_ready);
        assert!(r_bad.emergency_suspended);

        // 2. Valid request for AWAKENED transition is refused by PendingCertification
        let r_live = EngineIpcClient::send_command(
            dir.path(),
            "main-engine",
            1,
            EngineControlCommand::RequestModeTransition {
                target_mode: "AWAKENED".into(),
                reason: "test".into(),
            },
        )
        .unwrap();
        assert!(!r_live.ok);
        assert!(r_live.message.contains("PendingCertification"));

        // 3. Replay with same nonce=1 rejected cleanly and fails closed
        let r_replay = EngineIpcClient::send_command(
            dir.path(),
            "main-engine",
            1,
            EngineControlCommand::GetState,
        )
        .unwrap();
        assert!(!r_replay.ok);
        assert!(r_replay.message.contains("nonce"));
        assert!(!r_replay.maintenance_ready);

        // 4. Valid EmergencySuspend + GracefulShutdown succeed cleanly
        let r_susp = EngineIpcClient::send_command(
            dir.path(),
            "main-engine",
            2,
            EngineControlCommand::EmergencySuspend {
                reason: "operator test".into(),
            },
        )
        .unwrap();
        assert!(r_susp.ok);
        assert!(r_susp.emergency_suspended);
        assert_eq!(r_susp.mode, "SUSPENDED");

        // 5. Oversized payload (> 16 KiB) rejected before parsing, fail closed
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut raw_s = TcpStream::connect(addr).unwrap();
        raw_s
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let huge = format!("{}\n", "x".repeat(20_000));
        raw_s.write_all(huge.as_bytes()).unwrap();
        let _ = raw_s.flush();
        let mut line = String::new();
        BufReader::new(raw_s).read_line(&mut line).unwrap();
        let parsed: EngineControlResponse = serde_json::from_str(line.trim()).unwrap();
        assert!(!parsed.ok);
        assert!(parsed.message.contains("Oversized"));
        assert!(!parsed.protective_stops_verified);
        assert!(!parsed.maintenance_ready);

        let r_stop = EngineIpcClient::send_command(
            dir.path(),
            "main-engine",
            3,
            EngineControlCommand::GracefulShutdown,
        )
        .unwrap();
        assert!(r_stop.ok);
        srv.stop();
    }

    #[test]
    fn fragmented_slow_concurrent_requests_and_storage_authority_work_without_reset() {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::Storage::open(dir.path().join("eng_ipc.db")).unwrap();
        storage
            .with_conn(crate::storage::schema::run_migrations)
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let auth = EngineControlAuthority::new("auth-eng", stop.clone(), Some(storage.clone()));
        let mut srv = EngineIpcServer::start_with_authority(dir.path(), auth.clone()).unwrap();
        let port = srv.descriptor.port;
        let token = srv.descriptor.token_hex.clone();

        // 1. Slow peer holding an incomplete frame does not block concurrent EmergencySuspend
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut slow = TcpStream::connect(addr).unwrap();
        slow.write_all(b"{\"token_hex\":\"partial\"").unwrap();
        slow.flush().unwrap();

        let susp = EngineIpcClient::send_command(
            dir.path(),
            "auth-eng",
            1,
            EngineControlCommand::EmergencySuspend {
                reason: "slow-peer concurrency test".into(),
            },
        )
        .unwrap();
        assert!(susp.ok && susp.emergency_suspended);
        assert!(auth.is_emergency_suspended());
        let saved_susp: String = storage.with_conn(|c| {
            c.query_row(
                "SELECT value FROM meta_kv WHERE key='engine_emergency_suspended'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        });
        assert_eq!(saved_susp, "true");

        // 2. Fragmented multi-chunk request succeeds reliably
        let mut frag = TcpStream::connect(addr).unwrap();
        frag.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let req_json = serde_json::to_string(&AuthenticatedEngineRequest {
            token_hex: token.clone(),
            nonce: 2,
            issued_at_ms: Utc::now().timestamp_millis(),
            command: EngineControlCommand::GetState,
        })
        .unwrap()
            + "\n";
        for chunk in req_json.as_bytes().chunks(13) {
            frag.write_all(chunk).unwrap();
            frag.flush().unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut f_line = String::new();
        BufReader::new(frag).read_line(&mut f_line).unwrap();
        let f_resp: EngineControlResponse = serde_json::from_str(f_line.trim()).unwrap();
        assert!(f_resp.ok && f_resp.emergency_suspended);

        // 3. Parallel burst of 20 bad-token requests completes with 0 socket resets
        std::thread::scope(|s| {
            for i in 0..20u64 {
                s.spawn(move || {
                    let r = EngineIpcClient::send_raw(
                        port,
                        &AuthenticatedEngineRequest {
                            token_hex: format!("bad-{i}"),
                            nonce: 1,
                            issued_at_ms: Utc::now().timestamp_millis(),
                            command: EngineControlCommand::Heartbeat,
                        },
                    )
                    .expect("no WSAECONNRESET 10054 under parallel load");
                    assert!(!r.ok);
                    assert_eq!(r.mode, "UNKNOWN");
                });
            }
        });

        drop(slow);
        let r_stop = EngineIpcClient::send_command(
            dir.path(),
            "auth-eng",
            3,
            EngineControlCommand::GracefulShutdown,
        )
        .unwrap();
        assert!(r_stop.ok);
        assert!(stop.load(Ordering::SeqCst));
        srv.stop();
    }
}

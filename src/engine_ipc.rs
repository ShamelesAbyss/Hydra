//! Authenticated Local Control IPC for the Live Trading Engine (`v1.14.0 | PROJECT IMMORTALITY`).
//!
//! Security & privilege-boundary invariants:
//! - Binds strictly to localhost (`127.0.0.1`) with an OS-scoped control descriptor containing a
//!   256-bit secret token, PID, and endpoint port (`chmod 0600` on Unix).
//! - Pre-allocation bounded line reading (`MAX_ENGINE_IPC_BYTES`) before deserialization.
//! - Constant-time token verification + strictly increasing `nonce` replay protection + timestamp
//!   freshness check.
//! - Engine remains sole execution/signing authority; cockpit/CLI act purely as authenticated
//!   clients and never receive private keys or secret material.

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MAX_ENGINE_IPC_BYTES: u64 = 16 * 1024;
pub const MAX_IPC_CLOCK_SKEW_MS: i64 = 30_000;

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

pub fn write_descriptor(root: &Path, desc: &EngineControlDescriptor) -> Result<PathBuf> {
    let dir = root.join("engine-control");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
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
    Ok(path)
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
        return mk_resp(false, "Unauthorized engine IPC token".into(), state);
    }
    if (now_ms - req.issued_at_ms).abs() > MAX_IPC_CLOCK_SKEW_MS {
        return mk_resp(
            false,
            "Stale IPC timestamp outside freshness window".into(),
            state,
        );
    }
    let last_nonce = nonce_tracker.get(&state.engine_id).copied().unwrap_or(0);
    if req.nonce <= last_nonce {
        return mk_resp(
            false,
            "Replayed or non-monotonic IPC nonce rejected".into(),
            state,
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

pub struct EngineIpcServer {
    pub descriptor: EngineControlDescriptor,
    pub state: Arc<Mutex<EngineAuthoritativeState>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EngineIpcServer {
    pub fn start(root: &Path, engine_id: &str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("bind engine IPC listener")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let pid = std::process::id();
        let desc = EngineControlDescriptor {
            schema_version: 1,
            engine_id: engine_id.into(),
            pid,
            port,
            token_hex: generate_token_hex(engine_id, pid),
            started_at_ms: Utc::now().timestamp_millis(),
        };
        write_descriptor(root, &desc)?;

        let state = Arc::new(Mutex::new(EngineAuthoritativeState::dormant(engine_id)));
        let st_clone = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let token = desc.token_hex.clone();

        let handle = std::thread::Builder::new()
            .name(format!("hydra-engine-ipc-{engine_id}"))
            .spawn(move || {
                let mut nonces = BTreeMap::new();
                while !stop_clone.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                            let mut limited = BufReader::new(std::io::Read::by_ref(&mut stream))
                                .take(MAX_ENGINE_IPC_BYTES + 1);
                            let mut line = String::new();
                            let resp = match limited.read_line(&mut line) {
                                Ok(n) if n as u64 > MAX_ENGINE_IPC_BYTES => EngineControlResponse {
                                    ok: false,
                                    engine_id: "".into(),
                                    pid,
                                    mode: "DORMANT".into(),
                                    emergency_suspended: false,
                                    breaker_latched: false,
                                    open_positions: 0,
                                    unresolved_intents: 0,
                                    protective_stops_verified: true,
                                    maintenance_ready: true,
                                    message: "Oversized engine IPC request rejected".into(),
                                    timestamp_ms: Utc::now().timestamp_millis(),
                                },
                                Ok(_) => match serde_json::from_str::<AuthenticatedEngineRequest>(
                                    line.trim(),
                                ) {
                                    Ok(req) => {
                                        let mut st = st_clone.lock().unwrap();
                                        let r = handle_authenticated_request(
                                            &token,
                                            &mut nonces,
                                            &mut st,
                                            &req,
                                            Utc::now().timestamp_millis(),
                                        );
                                        if st.shutdown_requested {
                                            stop_clone.store(true, Ordering::SeqCst);
                                        }
                                        r
                                    }
                                    Err(e) => EngineControlResponse {
                                        ok: false,
                                        engine_id: "".into(),
                                        pid,
                                        mode: "DORMANT".into(),
                                        emergency_suspended: false,
                                        breaker_latched: false,
                                        open_positions: 0,
                                        unresolved_intents: 0,
                                        protective_stops_verified: true,
                                        maintenance_ready: true,
                                        message: format!("Malformed engine IPC JSON: {e}"),
                                        timestamp_ms: Utc::now().timestamp_millis(),
                                    },
                                },
                                Err(_) => continue,
                            };
                            if let Ok(raw) = serde_json::to_string(&resp) {
                                let _ = stream.write_all(raw.as_bytes());
                                let _ = stream.write_all(b"\n");
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(15));
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

        // 1. Wrong token rejected
        let bad = AuthenticatedEngineRequest {
            token_hex: "0000".into(),
            nonce: 1,
            issued_at_ms: Utc::now().timestamp_millis(),
            command: EngineControlCommand::Heartbeat,
        };
        let r_bad = EngineIpcClient::send_raw(port, &bad).unwrap();
        assert!(!r_bad.ok);
        assert!(r_bad.message.contains("Unauthorized"));

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

        // 3. Replay with same nonce=1 rejected cleanly
        let r_replay = EngineIpcClient::send_command(
            dir.path(),
            "main-engine",
            1,
            EngineControlCommand::GetState,
        )
        .unwrap();
        assert!(!r_replay.ok);
        assert!(r_replay.message.contains("nonce"));

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

        // 5. Oversized payload (> 16 KiB) rejected before parsing
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut raw_s = TcpStream::connect(addr).unwrap();
        let huge = format!("{}\n", "x".repeat(20_000));
        raw_s.write_all(huge.as_bytes()).unwrap();
        let mut line = String::new();
        BufReader::new(raw_s).read_line(&mut line).unwrap();
        let parsed: EngineControlResponse = serde_json::from_str(line.trim()).unwrap();
        assert!(!parsed.ok);
        assert!(parsed.message.contains("Oversized"));

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
}

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;

#[derive(Clone, Copy, clap::ValueEnum)]
enum CliMode {
    Spatial,
    Classic,
}

#[derive(Parser)]
#[command(
    version,
    about = "HYDRA — Project Leviathan unified native graphical command center"
)]
struct Options {
    /// Optional path to an existing Hydra SQLite history database.
    /// Defaults safely to the OS user data directory on clean launches.
    #[arg(long)]
    db: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = hydra::desktop_renderer::Backend::Auto)]
    renderer: hydra::desktop_renderer::Backend,
    #[arg(long)]
    fullscreen: bool,
    #[arg(long, value_enum, default_value_t = hydra::leviathan_scene::SceneQuality::High)]
    quality: hydra::leviathan_scene::SceneQuality,
    #[arg(long)]
    reduced_motion: bool,
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(15..=144))]
    frame_cap: u32,
    /// Use OS window decorations instead of the integrated borderless frame.
    #[arg(long)]
    decorated: bool,
    /// Open directly to the LEVIATHAN TREASURY / THE OFFERING settings tab.
    #[arg(long)]
    treasury: bool,
    /// Open directly to the LEVIATHAN EVOLUTION native updates tab.
    #[arg(long)]
    updates: bool,
    /// Optional subsystem head index (0..=6) to focus at startup.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=6))]
    head: Option<u8>,
    /// Interface presentation mode: spatial holographic panels or classic docked rails.
    #[arg(long, value_enum)]
    mode: Option<CliMode>,
    /// Optional initial camera horizontal orbit azimuth in degrees (-180..=180).
    #[arg(long)]
    orbit: Option<f32>,
    /// Internal headless paper-engine service worker mode for self-contained distribution.
    #[arg(long, hide = true)]
    paper_service_worker: bool,
    /// Paper session ID when invoking `--paper-service-worker`.
    #[arg(long, hide = true)]
    session: Option<String>,
    /// Service control & lock root directory when invoking `--paper-service-worker`.
    #[arg(long, hide = true)]
    root: Option<PathBuf>,
    /// Tick cadence in seconds when invoking `--paper-service-worker`.
    #[arg(long, hide = true, default_value_t = 5)]
    interval_s: u64,
    /// Optional finite tick budget for automated standalone verification.
    #[arg(long, hide = true)]
    max_ticks: Option<u64>,
    /// Optional WebSocket endpoint override for `--paper-service-worker`.
    #[arg(long, hide = true)]
    ws_url: Option<String>,
    /// Optional HTTP /info endpoint override for `--paper-service-worker`.
    #[arg(long, hide = true)]
    info_url: Option<String>,
    /// Internal headless external WatchdogSupervisor mode for self-contained supervision.
    #[arg(long, hide = true)]
    watchdog_supervisor: bool,
    /// Supervised target (`paper` or `engine`) when invoking `--watchdog-supervisor`.
    #[arg(long, hide = true, default_value = "paper")]
    watchdog_target: String,
    /// Engine ID when invoking `--watchdog-supervisor --watchdog-target engine`.
    #[arg(long, hide = true, default_value = "main-engine")]
    engine_id: String,
    /// Maximum supervised worker launches for `--watchdog-supervisor`.
    #[arg(long, hide = true, default_value_t = 4)]
    watchdog_max_launches: usize,
    /// Watchdog heartbeat stall timeout in milliseconds.
    #[arg(long, hide = true, default_value_t = 30_000)]
    watchdog_heartbeat_ms: i64,
    /// Internal headless engine worker mode for self-contained standalone supervision (`--watchdog-target engine`).
    #[arg(long, hide = true)]
    engine_worker: bool,
    /// Comma-separated symbols for `--engine-worker`.
    #[arg(long, hide = true, default_value = "BTC,ETH,SOL")]
    symbols: String,
    /// Optional strategy configuration artifact for `--engine-worker`.
    #[arg(long = "strategy-config", hide = true)]
    strategy_config: Option<PathBuf>,
    /// Engine tick cadence in ms for `--engine-worker`.
    #[arg(long, hide = true, default_value_t = 1000)]
    engine_poll_ms: u64,
    /// Strictly non-production offline observation fixture mode for `--engine-worker`.
    #[arg(long, hide = true)]
    engine_offline_fixture: bool,
}

fn run_headless_engine_worker(options: &Options) -> anyhow::Result<()> {
    let root = match options.root.as_ref() {
        Some(r) => r.clone(),
        None => hydra::paper_store::PaperStore::default_root()?,
    };
    let db_path = options.db.clone().unwrap_or_else(|| root.join("engine.db"));
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_sig = stop.clone();
    let _ = ctrlc::set_handler(move || {
        stop_sig.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let syms: Vec<String> = options
        .symbols
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let cfg = hydra::engine_worker::EngineWorkerConfig {
        root,
        db_path,
        engine_id: options.engine_id.clone(),
        symbols: syms,
        poll_interval_ms: options.engine_poll_ms,
        max_ticks: options.max_ticks,
        strategy_config: options.strategy_config.clone(),
        offline_fixture: options.engine_offline_fixture,
    };
    let _ticks = rt.block_on(hydra::engine_worker::run_headless_engine_worker(cfg, stop))?;
    Ok(())
}

fn run_headless_watchdog_supervisor(options: &Options) -> anyhow::Result<()> {
    let root = match options.root.as_ref() {
        Some(r) => r.clone(),
        None => hydra::paper_store::PaperStore::default_root()?,
    };
    let exe = std::env::current_exe()?;
    let policy = hydra::watchdog::WatchdogPolicy {
        heartbeat_timeout_ms: options.watchdog_heartbeat_ms.max(1_000),
        ..Default::default()
    };
    let mut supervisor = hydra::watchdog::WatchdogSupervisor::acquire(&root, policy)?;
    let poll_interval = std::time::Duration::from_millis(500);
    let stop_grace = std::time::Duration::from_secs(4);
    let max_launches = options.watchdog_max_launches.max(1);

    let summary = if options.watchdog_target.eq_ignore_ascii_case("engine") {
        let engine_id = options.engine_id.clone();
        let db_str = options
            .db
            .clone()
            .unwrap_or_else(|| root.join("engine.db"))
            .display()
            .to_string();
        let root_str = root.display().to_string();
        let poll_str = options.engine_poll_ms.clamp(200, 30_000).to_string();
        let max_ticks_str = options.max_ticks.map(|m| m.to_string());
        let sc_str = options
            .strategy_config
            .as_ref()
            .map(|p| p.display().to_string());
        let mut owned_args: Vec<String> = vec![
            "--engine-worker".into(),
            "--root".into(),
            root_str,
            "--db".into(),
            db_str,
            "--engine-id".into(),
            engine_id.clone(),
            "--symbols".into(),
            options.symbols.clone(),
            "--engine-poll-ms".into(),
            poll_str,
        ];
        if let Some(ref m) = max_ticks_str {
            owned_args.push("--max-ticks".into());
            owned_args.push(m.clone());
        }
        if let Some(ref s) = sc_str {
            owned_args.push("--strategy-config".into());
            owned_args.push(s.clone());
        }
        if options.engine_offline_fixture {
            owned_args.push("--engine-offline-fixture".into());
        }
        let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();
        let root_probe = root.clone();
        let root_stop = root.clone();
        let eid_probe = engine_id.clone();
        let eid_stop = engine_id;
        supervisor.supervise_live_worker(
            &exe,
            &arg_refs,
            poll_interval,
            stop_grace,
            max_launches,
            move |pid| match hydra::engine_ipc::EngineIpcClient::send_command(
                &root_probe,
                &eid_probe,
                hydra::engine_ipc::EngineIpcClient::next_nonce(),
                hydra::engine_ipc::EngineControlCommand::Heartbeat,
            ) {
                Ok(resp) => hydra::watchdog::WorkerHealthProbe {
                    alive_heartbeat_ms: (resp.last_tick_ms > 0).then_some(resp.last_tick_ms),
                    progress_counter: resp.engine_ticks,
                    healthy: resp.ok
                        && resp.pid == pid
                        && !resp.breaker_latched
                        && !resp.emergency_suspended,
                    detail: format!(
                        "pid={} mode={} ticks={} pilot={} block={}",
                        resp.pid,
                        resp.mode,
                        resp.engine_ticks,
                        resp.pilot_active,
                        resp.entry_block_reason
                    ),
                },
                Err(e) => hydra::watchdog::WorkerHealthProbe {
                    alive_heartbeat_ms: None,
                    progress_counter: 0,
                    healthy: false,
                    detail: format!("engine IPC heartbeat unreachable: {e}"),
                },
            },
            move |_pid| {
                let _ = hydra::engine_ipc::EngineIpcClient::send_command(
                    &root_stop,
                    &eid_stop,
                    hydra::engine_ipc::EngineIpcClient::next_nonce(),
                    hydra::engine_ipc::EngineControlCommand::GracefulShutdown,
                );
                Ok(())
            },
        )?
    } else {
        let session_id = options
            .session
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--session is required with --watchdog-supervisor"))?
            .to_string();
        let root_str = root.display().to_string();
        let interval_str = options.interval_s.clamp(2, 60).to_string();
        let db_str = options.db.as_ref().map(|p| p.display().to_string());
        let max_ticks_str = options.max_ticks.map(|m| m.to_string());
        let mut owned_args: Vec<String> = vec![
            "--paper-service-worker".into(),
            "--session".into(),
            session_id.clone(),
            "--root".into(),
            root_str,
            "--interval-s".into(),
            interval_str,
        ];
        if let Some(ref d) = db_str {
            owned_args.push("--db".into());
            owned_args.push(d.clone());
        }
        if let Some(ref m) = max_ticks_str {
            owned_args.push("--max-ticks".into());
            owned_args.push(m.clone());
        }
        if let Some(ref ws) = options.ws_url {
            owned_args.push("--ws-url".into());
            owned_args.push(ws.clone());
        }
        if let Some(ref info) = options.info_url {
            owned_args.push("--info-url".into());
            owned_args.push(info.clone());
        }
        let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();
        let root_probe = root.clone();
        let root_stop = root.clone();
        let sid_probe = session_id.clone();
        let sid_stop = session_id;
        supervisor.supervise_live_worker(
            &exe,
            &arg_refs,
            poll_interval,
            stop_grace,
            max_launches,
            move |pid| match hydra::paper_service::PaperServiceClient::send_command(
                &root_probe,
                &sid_probe,
                hydra::paper_service::PaperControlCommand::Status,
            ) {
                Ok(resp) => {
                    let hb_ms = chrono::DateTime::parse_from_rfc3339(&resp.checked_at)
                        .ok()
                        .map(|dt| dt.timestamp_millis());
                    let healthy = resp.ok
                        && resp.pid == pid
                        && resp.status != "faulted"
                        && resp.fault_reason.is_none();
                    hydra::watchdog::WorkerHealthProbe {
                        alive_heartbeat_ms: hb_ms,
                        progress_counter: resp.cycle_count,
                        healthy,
                        detail: format!(
                            "status={} cycle={} fault={:?}",
                            resp.status, resp.cycle_count, resp.fault_reason
                        ),
                    }
                }
                Err(e) => hydra::watchdog::WorkerHealthProbe {
                    alive_heartbeat_ms: None,
                    progress_counter: 0,
                    healthy: false,
                    detail: format!("paper IPC probe failed: {e}"),
                },
            },
            move |_pid| {
                let _ = hydra::paper_service::PaperServiceClient::send_command(
                    &root_stop,
                    &sid_stop,
                    hydra::paper_service::PaperControlCommand::Stop,
                );
                Ok(())
            },
        )?
    };

    hydra::startup_diagnostics::log_startup(&format!(
        "WATCHDOG SUPERVISOR FINISHED: launches={} restarts={} escalated={} detail={}",
        summary.launches, summary.restarts, summary.escalated, summary.detail
    ));
    Ok(())
}

fn run_headless_paper_worker(options: &Options) -> anyhow::Result<()> {
    let session_id = options
        .session
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--session is required with --paper-service-worker"))?;
    let store = match options.db.as_ref() {
        Some(p) => hydra::paper_store::PaperStore::open(p)?,
        None => hydra::paper_store::PaperStore::open_default()?,
    };
    let root = match options.root.as_ref() {
        Some(r) => r.clone(),
        None => hydra::paper_store::PaperStore::default_root()?,
    };
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_sig = stop.clone();
    let _ = ctrlc::set_handler(move || {
        stop_sig.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let cfg = hydra::paper_service::PaperServiceConfig {
        root,
        store,
        session_id: session_id.to_string(),
        ws_url: options
            .ws_url
            .clone()
            .unwrap_or_else(|| hydra::hunter_stream::HYPERLIQUID_PUBLIC_WS_URL.to_string()),
        info_url: options
            .info_url
            .clone()
            .unwrap_or_else(|| hydra::hunter::HYPERLIQUID_PUBLIC_INFO_URL.to_string()),
        tick_interval_s: options.interval_s,
        max_ticks: options.max_ticks,
    };
    let _resp = rt.block_on(hydra::paper_service::run_paper_session_service(cfg, stop))?;
    Ok(())
}

fn main() {
    hydra::startup_diagnostics::install_panic_hook();
    let options = Options::parse();
    if options.watchdog_supervisor {
        if let Err(err) = run_headless_watchdog_supervisor(&options) {
            hydra::startup_diagnostics::log_startup(&format!(
                "WATCHDOG SUPERVISOR FAILED: {err:#}"
            ));
            eprintln!("watchdog supervisor failed: {err:#}");
            std::process::exit(1);
        }
        return;
    }
    if options.engine_worker {
        if let Err(err) = run_headless_engine_worker(&options) {
            hydra::startup_diagnostics::log_startup(&format!(
                "HEADLESS ENGINE WORKER FAILED: {err:#}"
            ));
            eprintln!("headless engine worker failed: {err:#}");
            std::process::exit(1);
        }
        return;
    }
    if options.paper_service_worker {
        if let Err(err) = run_headless_paper_worker(&options) {
            hydra::startup_diagnostics::log_startup(&format!(
                "HEADLESS PAPER WORKER FAILED: {err:#}"
            ));
            eprintln!("headless paper worker failed: {err:#}");
            std::process::exit(1);
        }
        return;
    }
    let db_path = hydra::startup_diagnostics::resolve_default_database(options.db);
    let db_health = hydra::startup_diagnostics::verify_local_database_health(&db_path);
    let mode_override = options.mode.map(|m| match m {
        CliMode::Spatial => hydra::leviathan_spatial::PresentationMode::Spatial,
        CliMode::Classic => hydra::leviathan_spatial::PresentationMode::Classic,
    });
    hydra::startup_diagnostics::log_startup(&format!(
        "Launching HYDRA cockpit v{} (renderer={}, fullscreen={}, quality={:?}, db={}, db_health={:?})",
        env!("CARGO_PKG_VERSION"),
        options.renderer.name(),
        options.fullscreen,
        options.quality,
        db_path.display(),
        db_health
    ));

    if let Err(err) = hydra::cockpit::run_with_spatial_state(
        db_path,
        options.renderer,
        options.fullscreen,
        !options.decorated,
        options.quality,
        options.reduced_motion,
        options.frame_cap,
        options.treasury,
        options.updates,
        options.head.map(usize::from),
        mode_override,
        options.orbit,
    ) {
        let detail = format!("{err}");
        hydra::startup_diagnostics::log_startup(&format!("STARTUP FAILED: {detail}"));
        let report = hydra::startup_diagnostics::write_crash_report(
            "Graphical Cockpit Startup Failed",
            &detail,
        );
        hydra::startup_diagnostics::show_startup_error_dialog(
            "HYDRA — Project Leviathan",
            &detail,
            report.as_deref(),
        );
        std::process::exit(1);
    }
}

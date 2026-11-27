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

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;

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
}

fn main() {
    hydra::startup_diagnostics::install_panic_hook();
    let options = Options::parse();
    let db_path = hydra::startup_diagnostics::resolve_default_database(options.db);
    let db_health = hydra::startup_diagnostics::verify_local_database_health(&db_path);
    hydra::startup_diagnostics::log_startup(&format!(
        "Launching HYDRA cockpit v{} (renderer={}, fullscreen={}, quality={:?}, db={}, db_health={:?})",
        env!("CARGO_PKG_VERSION"),
        options.renderer.name(),
        options.fullscreen,
        options.quality,
        db_path.display(),
        db_health
    ));

    if let Err(err) = hydra::cockpit::run_with_initial_view(
        db_path,
        options.renderer,
        options.fullscreen,
        !options.decorated,
        options.quality,
        options.reduced_motion,
        options.frame_cap,
        options.treasury,
        options.updates,
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

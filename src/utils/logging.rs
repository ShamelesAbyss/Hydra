//! Logging init + tail utility.

use anyhow::Result;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::AppConfig;
use crate::tui::{DashLayer, SharedDash};

/// Plain logging: stdout + rolling daily file.
/// Used when the TUI is disabled (`--no-tui`) or for one-shot commands.
pub fn init(cfg: &AppConfig) -> Result<()> {
    fs::create_dir_all(&cfg.log_dir).ok();
    let file_appender = tracing_appender::rolling::daily(&cfg.log_dir, "hydra.log");

    let filter = EnvFilter::try_new(&cfg.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let console = fmt::layer()
        .with_target(false)
        .with_timer(fmt::time::uptime())
        .compact();

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_timer(fmt::time::uptime());

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .with(file_layer)
        .try_init();

    Ok(())
}

/// TUI logging: rolling daily file + custom layer that pipes events into the
/// dashboard feed. Stdout is intentionally NOT attached so the alt-screen
/// render is never corrupted by log lines.
pub fn init_tui(cfg: &AppConfig, dash: SharedDash) -> Result<()> {
    fs::create_dir_all(&cfg.log_dir).ok();
    let file_appender = tracing_appender::rolling::daily(&cfg.log_dir, "hydra.log");

    let filter = EnvFilter::try_new(&cfg.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_timer(fmt::time::uptime());

    let dash_layer = DashLayer::new(dash);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(dash_layer)
        .try_init();

    Ok(())
}

pub fn tail_latest(cfg: &AppConfig, tail: usize) -> Result<()> {
    let dir: PathBuf = cfg.log_dir.clone();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.file_name().map(|n| n.to_string_lossy().contains("hydra.log")).unwrap_or(false))
        .collect();
    if files.is_empty() {
        println!("(no log files in {})", dir.display());
        return Ok(());
    }
    files.sort();
    let latest = files.last().unwrap().clone();
    let f = fs::File::open(&latest)?;
    let size = f.metadata()?.len();
    let mut reader = BufReader::new(f);
    let start = size.saturating_sub(65_536);
    reader.seek(SeekFrom::Start(start))?;
    let mut lines: Vec<String> = Vec::new();
    for l in reader.lines().flatten() { lines.push(l); }
    let n = lines.len();
    for l in &lines[n.saturating_sub(tail)..] {
        println!("{l}");
    }
    Ok(())
}

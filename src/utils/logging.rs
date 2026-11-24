//! Logging init + tail utility.

use anyhow::Result;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::AppConfig;
use crate::tui::{DashLayer, SharedDash};

// The legacy SDK logs signed request bodies at debug level. This independent
// metadata filter cannot be overridden by a more-specific LOG_LEVEL directive.
fn safe_metadata(metadata: &tracing::Metadata<'_>) -> bool {
    !metadata.target().starts_with("hyperliquid_rust_sdk")
        || *metadata.level() <= tracing::Level::INFO
}

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
        .with(tracing_subscriber::filter::filter_fn(safe_metadata))
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
        .with(tracing_subscriber::filter::filter_fn(safe_metadata))
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
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .map(|n| n.to_string_lossy().contains("hydra.log"))
                    .unwrap_or(false)
        })
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
    for l in reader.lines() {
        lines.push(l?);
    }
    let n = lines.len();
    for l in &lines[n.saturating_sub(tail)..] {
        println!("{l}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn sdk_payload_logs_are_blocked_even_with_specific_trace_directive() {
        let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = Capture(buffer.clone());
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::filter_fn(safe_metadata))
            .with(EnvFilter::new("trace,hyperliquid_rust_sdk::exchange=trace"))
            .with(
                fmt::layer()
                    .with_ansi(false)
                    .without_time()
                    .with_writer(move || writer.clone()),
            );
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target:"hyperliquid_rust_sdk::exchange::exchange_client", "SENSITIVE_PAYLOAD_FIXTURE");
            tracing::info!(target:"hydra::execution", "safe operational event");
        });
        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(!output.contains("SENSITIVE_PAYLOAD_FIXTURE"));
        assert!(output.contains("safe operational event"));
    }
}

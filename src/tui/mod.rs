//! HYDRA Cockpit — read-only full-screen TUI.
//!
//! Architecture:
//! * a `SharedDash` (`Arc<RwLock<DashState>>`) is owned jointly by the engine
//!   (writer) and the TUI task (reader). The engine never blocks on the TUI.
//! * the TUI task runs on its own tokio future at ~4 fps. All keyboard input
//!   is polled non-blockingly. The TUI is strictly read-only; the *only* key
//!   it handles is Ctrl+C, which is translated into the same `stop` atomic
//!   used by the SIGINT handler so the engine shuts down cleanly.
//! * the terminal state (alt screen, raw mode) is restored automatically on
//!   drop, on panic (custom hook), and on error. If the TUI task fails for
//!   any reason, the engine keeps trading — logs are preserved in the rolling
//!   file and also pushed back into stdout once the TUI restore happens.
//!
//! This module adds **zero** code paths that can affect trading behaviour.

pub mod render;
pub mod state;
pub mod tracing_layer;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub use state::{shared, DashState, SharedDash};
pub use tracing_layer::DashLayer;

const FRAME_INTERVAL: Duration = Duration::from_millis(250);

/// RAII guard: always restores the terminal, even on panic.
struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen).context("enter alt screen")?;
        Ok(Self)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup. Ignore errors — we're shutting down or panicking.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        prev(info);
    }));
}

/// Run the TUI until the `stop` flag is set or a fatal error occurs.
/// On return, the terminal has been restored and logs can resume on stdout.
///
/// This function catches its own internal errors and returns `Ok(())` when the
/// stop flag was observed, or `Err` when a terminal setup error prevents any
/// rendering at all (caller should then fall back to plain-log mode).
pub async fn run(dash: SharedDash, stop: Arc<AtomicBool>) -> Result<()> {
    install_panic_hook();
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(backend).context("build ratatui terminal")?;
    terminal.clear().ok();

    let mut next = tokio::time::Instant::now() + FRAME_INTERVAL;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        // --- 1. Poll input (non-blocking, 40ms window). Only Ctrl+C is honoured.
        // Spread the poll across the render interval so we stay responsive.
        let poll_deadline = tokio::time::Instant::now() + Duration::from_millis(40);
        while tokio::time::Instant::now() < poll_deadline {
            // `crossterm::event::poll` is blocking with a timeout; run under
            // spawn_blocking in short windows so it can't starve the runtime.
            let poll_res = tokio::task::spawn_blocking(|| {
                event::poll(Duration::from_millis(20)).unwrap_or(false)
            })
            .await
            .unwrap_or(false);
            if !poll_res {
                break;
            }
            if let Ok(ev) = event::read() {
                if let Event::Key(k) = ev {
                    if k.kind == KeyEventKind::Press
                        && k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        stop.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }

        // --- 2. Snapshot the dashboard under a brief read lock.
        // We clone out so we don't hold the lock across the draw call.
        let snapshot: DashState = match dash.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        // --- 3. Draw a frame. Any draw error is logged (to file) and we try again.
        if let Err(e) = terminal.draw(|f| render::render(f, &snapshot)) {
            // Write to file log via tracing — won't corrupt terminal because
            // stdout layer is disabled while the TUI is active.
            tracing::error!(error=%e, "tui draw error");
        }

        // --- 4. Sleep to next frame; skip if we fell behind.
        let now = tokio::time::Instant::now();
        if now < next {
            tokio::time::sleep_until(next).await;
        }
        next = tokio::time::Instant::now() + FRAME_INTERVAL;
    }

    // Terminal is restored by `_guard` on drop.
    Ok(())
}

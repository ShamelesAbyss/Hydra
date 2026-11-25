//! CLI definition and command dispatch.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use comfy_table::{presets::UTF8_BORDERS_ONLY, ContentArrangement, Table};
use owo_colors::OwoColorize;

use crate::config::AppConfig;
use crate::core::engine::Engine;
use crate::monitoring::HealthReport;
use crate::reporting::SessionReporter;
use crate::state::StateManager;
use crate::storage::Storage;
use crate::utils::banner;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Venue {
    Hyperliquid,
}

#[derive(Parser, Debug)]
#[command(
    name = "hydra",
    about = "HYDRA Project Leviathan · native trading engine and research command center",
    version
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        env = "HYDRA_CONFIG",
        default_value = "config/strategies.toml"
    )]
    pub config: String,

    #[arg(long, global = true, env = "LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Suppress the startup banner.
    #[arg(long, global = true)]
    pub no_banner: bool,

    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the live trading engine.
    Run {
        #[arg(long, value_enum, default_value_t = Venue::Hyperliquid)]
        venue: Venue,
        #[arg(long, default_value = "ETH,BTC,HYPE,SOL")]
        symbols: String,
        /// Assert live execution was configured; cannot override ENABLE_LIVE_TRADING=false.
        #[arg(long)]
        live: bool,
        /// Disable the live Cockpit TUI and use plain scrolling logs instead.
        #[arg(long = "no-tui")]
        no_tui: bool,
    },
    Status,
    Positions,
    Pnl {
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    Sessions {
        #[arg(long, default_value_t = 10)]
        last: i64,
    },
    Logs {
        #[arg(long, default_value_t = 60)]
        tail: usize,
    },
    Stop,
    ConfigCheck,
    Recover,
    Health,
    /// Bind a local engine history database to its trading account.
    BindHistory {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long)]
        account: String,
        #[arg(long)]
        adopt_existing_history: bool,
    },
    /// Run deterministic candle research with explicit simulation costs.
    Backtest {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
    },
    /// Run a bounded seeded momentum-parameter research search.
    Optimize {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
    },
    /// Initialize a new local history database without exchange access.
    InitHistory {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
    },
    /// Print a versioned, read-only JSON snapshot of a local Hydra database.
    Telemetry {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect retained local lifecycle evidence. No network, wallet or migrations.
    StatusHistory {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long)]
        intent: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Inspect retained unsigned native actions. No network, wallet or migrations.
    WireHistory {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long)]
        intent: String,
    },
    /// Retrieve read-only exchange fill/funding evidence. Never submits orders.
    SyncLedger {
        #[arg(long)]
        since_ms: i64,
        #[arg(long)]
        end_ms: Option<i64>,
    },
    /// Manually clear the consecutive-loss circuit breaker. Stamps a reset
    /// timestamp; subsequent loss-streak checks ignore losses before this.
    ResetCooldown,
    EmergencyFlatten {
        #[arg(long)]
        confirm: bool,
    },
}

pub async fn dispatch(cli: Cli) -> Result<()> {
    if let Command::BindHistory {
        db,
        account,
        adopt_existing_history,
    } = &cli.cmd
    {
        anyhow::ensure!(db.is_file(), "initialize the history database first");
        let storage = Storage::open(db)?;
        storage.migrate()?;
        storage.bind_trading_account(account, *adopt_existing_history)?;
        println!("History database account binding verified");
        return Ok(());
    }
    if let Command::Backtest { candles, options } = &cli.cmd {
        let config = AppConfig::load_research(&cli.config, cli.log_level.as_deref())?;
        let bars = crate::research::read_json::<Vec<crate::core::Candle>>(candles)?;
        let options = options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::oracle::ReplayOptions>(p))
            .transpose()?
            .unwrap_or_default();
        let report = crate::oracle::replay(&config, &bars, &options)?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    if let Command::Optimize { candles, options } = &cli.cmd {
        let config = AppConfig::load_research(&cli.config, cli.log_level.as_deref())?;
        let bars = crate::research::read_json::<Vec<crate::core::Candle>>(candles)?;
        let options = options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::genesis::SearchOptions>(p))
            .transpose()?
            .unwrap_or_default();
        let report = crate::genesis::optimize(&config, &bars, &options)?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    if let Command::InitHistory { db } = &cli.cmd {
        crate::local_setup::initialize_history(db)?;
        println!("Initialized local history at {}", db.display());
        return Ok(());
    }
    if let Command::Telemetry { db, limit } = &cli.cmd {
        let snapshot = crate::telemetry::load_snapshot(db, *limit)?;
        println!("{}", serde_json::to_string(&snapshot)?);
        return Ok(());
    }
    if let Command::StatusHistory { db, intent, limit } = &cli.cmd {
        let history = crate::status_history::inspect(db, intent, *limit)?;
        println!("{}", serde_json::to_string(&history)?);
        return Ok(());
    }
    if let Command::WireHistory { db, intent } = &cli.cmd {
        let history = crate::wire_history::inspect(db, intent)?;
        println!("{}", serde_json::to_string(&history)?);
        return Ok(());
    }
    if !cli.no_banner {
        banner::print_banner();
    }

    let cfg = AppConfig::load(&cli.config, cli.log_level.as_deref())
        .context("failed to load configuration")?;

    // Logging setup is deferred for the `Run` command so we can conditionally
    // attach the Cockpit tracing layer. All other commands use plain stdout logs.
    if !matches!(cli.cmd, Command::Run { .. }) {
        crate::utils::logging::init(&cfg)?;
    }

    let storage = Storage::open(&cfg.db_path).context("failed to open SQLite storage")?;
    storage
        .migrate()
        .context("failed to run SQLite migrations")?;

    match cli.cmd {
        Command::BindHistory { .. } => unreachable!("handled before configuration"),
        Command::Backtest { .. } | Command::Optimize { .. } => {
            unreachable!("offline research handled before engine storage setup")
        }
        Command::InitHistory { .. } => unreachable!("handled before configuration setup"),
        Command::Telemetry { .. } => unreachable!("handled before configuration and storage setup"),
        Command::StatusHistory { .. } => {
            unreachable!("handled before configuration and storage setup")
        }
        Command::WireHistory { .. } => {
            unreachable!("handled before configuration and storage setup")
        }
        Command::Run {
            venue,
            symbols,
            live,
            no_tui,
        } => {
            let symbols: Vec<String> = symbols
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect();

            if no_tui {
                // Plain-log path: exactly the pre-v1.3.0 behaviour.
                crate::utils::logging::init(&cfg)?;
                let mut engine = Engine::new(cfg, storage, venue, symbols, live, None).await?;
                return engine.run().await;
            }

            // ── Cockpit TUI path ──
            // Give the user 1.5s to enjoy the banner, then take over the screen.
            tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;

            let dash = crate::tui::shared();
            // Attach the TUI tracing layer so all engine logs flow into the feed
            // pane (no stdout layer — alt-screen would otherwise be corrupted).
            crate::utils::logging::init_tui(&cfg, dash.clone())?;

            let mut engine =
                Engine::new(cfg, storage, venue, symbols, live, Some(dash.clone())).await?;
            let stop_flag = engine.stop_flag();

            // Spawn the TUI in its own task. If it fails for any reason we log
            // the error to the file and keep trading — the engine loop is
            // never gated on the TUI.
            let tui_dash = dash.clone();
            let tui_stop = stop_flag.clone();
            let tui_handle = tokio::spawn(async move {
                match crate::tui::run(tui_dash, tui_stop).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::error!(error=%e, "cockpit tui exited with error — continuing in headless mode")
                    }
                }
            });

            let engine_result = engine.run().await;

            // Ensure TUI tears down before we print the footer to stdout.
            stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), tui_handle).await;

            engine_result
        }
        Command::Status => commands::status(&cfg, &storage).await,
        Command::Positions => commands::positions(&storage),
        Command::Pnl { days } => commands::pnl(&storage, days),
        Command::Sessions { last } => commands::sessions(&storage, last),
        Command::Logs { tail } => commands::logs(&cfg, tail),
        Command::Stop => commands::stop(&cfg),
        Command::ConfigCheck => commands::config_check(&cfg),
        Command::Recover => {
            StateManager::new(storage.clone())
                .force_reconcile(&cfg)
                .await
        }
        Command::Health => {
            let report = HealthReport::gather(&cfg, &storage).await;
            report.print();
            if report.ok {
                Ok(())
            } else {
                Err(anyhow::anyhow!("unhealthy"))
            }
        }
        Command::SyncLedger { since_ms, end_ms } => {
            anyhow::ensure!(cfg.hl_account_address.is_some(), "sync-ledger requires the explicit trading-account address, not a derived signer address");
            let adapter = crate::adapters::hyperliquid::HyperliquidAdapter::new(&cfg).await?;
            let report = adapter
                .sync_history(
                    storage.clone(),
                    since_ms,
                    end_ms.unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Command::ResetCooldown => {
            storage.reset_consecutive_losses()?;
            let n = storage.consecutive_losses(cfg.risk.consecutive_loss_window_hours)?;
            println!(
                "{}",
                "✔ consecutive-loss cooldown reset".bright_green().bold()
            );
            println!(
                "  current streak now reads: {}/{}",
                n, cfg.risk.max_consecutive_losses
            );
            println!("  the bot will resume taking new signals on its next tick.");
            Ok(())
        }
        Command::EmergencyFlatten { confirm } => {
            if !confirm {
                println!(
                    "{}",
                    " ⚠  refusing to flatten without --confirm"
                        .bright_red()
                        .bold()
                );
                return Ok(());
            }
            StateManager::new(storage.clone())
                .emergency_flatten(&cfg)
                .await
        }
    }
}

mod commands {
    use super::*;

    pub async fn status(cfg: &AppConfig, storage: &Storage) -> Result<()> {
        SessionReporter::print_header(cfg, storage)?;
        let sm = StateManager::new(storage.clone());
        sm.print_snapshot(cfg).await
    }

    pub fn positions(storage: &Storage) -> Result<()> {
        let rows = storage.list_open_positions()?;
        banner::section("open positions");
        if rows.is_empty() {
            println!("  {}", "no open positions".yellow().italic());
            return Ok(());
        }
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY)
            .set_content_arrangement(ContentArrangement::Dynamic);
        t.set_header(vec![
            "Venue".bright_white().bold().to_string(),
            "Symbol".bright_white().bold().to_string(),
            "Side".bright_white().bold().to_string(),
            "Size".bright_white().bold().to_string(),
            "Entry".bright_white().bold().to_string(),
            "Stop".bright_white().bold().to_string(),
            "TP".bright_white().bold().to_string(),
            "Opened".bright_white().bold().to_string(),
        ]);
        for p in &rows {
            let side = if p.side == "LONG" {
                p.side.bright_green().to_string()
            } else {
                p.side.bright_red().to_string()
            };
            t.add_row(vec![
                p.venue.bright_cyan().to_string(),
                p.symbol.clone(),
                side,
                format!("{:.6}", p.size),
                format!("{:.4}", p.entry_price),
                p.stop_price
                    .map(|v| format!("{:.4}", v))
                    .unwrap_or_else(|| "-".into()),
                p.take_profit
                    .map(|v| format!("{:.4}", v))
                    .unwrap_or_else(|| "-".into()),
                p.opened_at.to_rfc3339(),
            ]);
        }
        println!("{t}");
        Ok(())
    }

    pub fn pnl(storage: &Storage, days: i64) -> Result<()> {
        let rows = storage.pnl_summary(days)?;
        banner::section(&format!("realized pnl — last {days}d"));
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY)
            .set_content_arrangement(ContentArrangement::Dynamic);
        t.set_header(
            vec!["Day", "Trades", "Wins", "Losses", "Realized USD"]
                .into_iter()
                .map(|s| s.bright_white().bold().to_string())
                .collect::<Vec<_>>(),
        );
        for r in rows {
            let realized = if r.realized_usd >= 0.0 {
                format!("+${:.2}", r.realized_usd)
                    .bright_green()
                    .to_string()
            } else {
                format!("-${:.2}", r.realized_usd.abs())
                    .bright_red()
                    .to_string()
            };
            t.add_row(vec![
                r.day,
                r.trades.to_string(),
                r.wins.to_string(),
                r.losses.to_string(),
                realized,
            ]);
        }
        println!("{t}");
        Ok(())
    }

    pub fn sessions(storage: &Storage, last: i64) -> Result<()> {
        let rows = storage.list_sessions(last)?;
        banner::section(&format!("last {last} sessions"));
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY)
            .set_content_arrangement(ContentArrangement::Dynamic);
        t.set_header(
            vec![
                "ID", "Start", "End", "Runtime", "Trades", "Realized", "Reason",
            ]
            .into_iter()
            .map(|s| s.bright_white().bold().to_string())
            .collect::<Vec<_>>(),
        );
        for s in rows {
            t.add_row(vec![
                s.id.to_string(),
                s.started_at.to_rfc3339(),
                s.ended_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "LIVE".bright_green().to_string()),
                s.runtime_secs
                    .map(|r| format!("{}s", r))
                    .unwrap_or_else(|| "-".into()),
                s.trades.to_string(),
                format!("${:.2}", s.realized_usd),
                s.shutdown_reason.unwrap_or_else(|| "-".into()),
            ]);
        }
        println!("{t}");
        Ok(())
    }

    pub fn logs(cfg: &AppConfig, tail: usize) -> Result<()> {
        banner::section(&format!("tail -n {tail}"));
        crate::utils::logging::tail_latest(cfg, tail)
    }

    pub fn stop(cfg: &AppConfig) -> Result<()> {
        let path = std::path::Path::new(&cfg.data_dir).join("STOP");
        std::fs::write(&path, chrono::Utc::now().to_rfc3339())?;
        println!(
            " {} wrote stop flag → {}",
            "✔".bright_green().bold(),
            path.display().to_string().dimmed()
        );
        Ok(())
    }

    pub fn config_check(cfg: &AppConfig) -> Result<()> {
        cfg.print_summary();
        cfg.validate()
    }
}

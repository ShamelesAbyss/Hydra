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
    /// Run offline FANG, EMA Anchor, COGNITION or AEGIS diagnostics.
    Diagnose {
        #[arg(long, value_enum)]
        kind: crate::diagnostics::DiagnosticKind,
        #[arg(long)]
        snapshot: std::path::PathBuf,
    },
    /// Rank an offline approved-market snapshot without engine or wallet access.
    Hunter {
        #[arg(long)]
        snapshot: std::path::PathBuf,
    },
    /// Run deterministic candle research with explicit simulation costs.
    Backtest {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
    },
    /// Compare fixed-configuration held-out folds and explicit simulation costs.
    ValidateResearch {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
    },
    /// Assess a frozen training frontier on chronologically later held-out data.
    AssessCandidates {
        #[arg(long)]
        training: std::path::PathBuf,
        #[arg(long)]
        held_out: std::path::PathBuf,
        #[arg(long)]
        checkpoint: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
        #[arg(long)]
        validation_options: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 4)]
        limit: usize,
    },
    /// Advance complete optimizer generations and publish an immutable checkpoint.
    OptimizeStep {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long)]
        options: Option<std::path::PathBuf>,
        #[arg(long)]
        checkpoint: Option<std::path::PathBuf>,
        #[arg(long)]
        output: std::path::PathBuf,
        #[arg(long, default_value_t = 1)]
        generations: usize,
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
    /// Inspect retained native actions and signing preparation. No network or wallet.
    WireHistory {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long)]
        intent: String,
    },
    /// Inspect consistent local intent evidence. No network, repair or hold release.
    IntentEvidence {
        #[arg(long, default_value = "data/hydra.db")]
        db: std::path::PathBuf,
        #[arg(long)]
        intent: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
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
    /// Run the connected HUNTER->COGNITION->FANG->AEGIS->NEXUS paper simulation pipeline over local candles.
    SynapsePaper {
        #[arg(long)]
        candles: std::path::PathBuf,
        #[arg(long, default_value = "BTC")]
        symbol: String,
        #[arg(long, default_value = "10000")]
        equity: String,
        #[arg(long)]
        json: bool,
    },
    /// Manage independent durable SQLite paper-trading sessions and headless engine service.
    PaperService {
        #[command(subcommand)]
        action: PaperServiceAction,
    },
    /// Inspect the authoritative FANG versioned strategy registry (`fang-registry-v1`).
    FangRegistry {
        #[arg(long)]
        json: bool,
    },
    /// Run COGNITION Bayesian shrinkage, empirical calibration, and Page-Hinkley drift diagnostics.
    CognitionStats {
        #[arg(long)]
        samples: std::path::PathBuf,
        #[arg(long, default_value = "*")]
        regime: String,
        #[arg(long, default_value_t = 200.0)]
        stop_loss_bps: f64,
        #[arg(long, default_value_t = 14.0)]
        cost_bps: f64,
    },
    /// Run ORACLE multi-market portfolio simulation and purged embargoed walk-forward CV.
    OraclePortfolio {
        #[arg(long)]
        dataset: std::path::PathBuf,
        #[arg(long, default_value_t = 3)]
        folds: usize,
        #[arg(long, default_value_t = 4)]
        embargo_bars: usize,
    },
    /// Run deterministic multi-worker GENESIS NSGA-II multi-objective campaign with embargoed OOS validation.
    GenesisNsga2 {
        #[arg(long)]
        dataset: std::path::PathBuf,
        #[arg(long, default_value_t = 40)]
        train_bars: usize,
        #[arg(long, default_value_t = 4)]
        embargo_bars: usize,
        #[arg(long, default_value_t = 8)]
        population: usize,
        #[arg(long, default_value_t = 3)]
        generations: usize,
        #[arg(long, default_value_t = 2)]
        workers: usize,
        #[arg(long, default_value_t = 7)]
        seed: u64,
    },
}

#[derive(Subcommand, Debug)]
pub enum PaperServiceAction {
    /// Create a durable isolated paper-trading session.
    Create {
        #[arg(long, default_value = "Public Market Paper")]
        label: String,
        #[arg(long, default_value = "public_paper")]
        kind: String,
        #[arg(long, default_value = "10000")]
        equity: String,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
    /// List persisted paper sessions.
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
    /// Run a paper session foreground service until Ctrl+C or authenticated IPC stop.
    Run {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 5)]
        interval_s: u64,
        #[arg(long)]
        ws_url: Option<String>,
        #[arg(long)]
        info_url: Option<String>,
    },
    /// Launch a detached background paper engine service process.
    Start {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 5)]
        interval_s: u64,
    },
    /// Query status (SQLite + live authenticated IPC heartbeat if running).
    Status {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
    /// Pause an active paper service via authenticated local IPC.
    Pause {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
    },
    /// Resume a paused paper service via authenticated local IPC.
    Resume {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
    },
    /// Stop an active paper service gracefully via authenticated local IPC.
    Stop {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
    },
    /// Archive a stopped paper session cleanly.
    Archive {
        #[arg(long)]
        session: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
    /// Reset a non-running paper session (requires `--confirm CONFIRM_RESET:<session_id>`).
    Reset {
        #[arg(long)]
        session: String,
        #[arg(long)]
        confirm: String,
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(long)]
        db: Option<std::path::PathBuf>,
    },
}

pub async fn dispatch(cli: Cli) -> Result<()> {
    if let Command::Diagnose { kind, snapshot } = &cli.cmd {
        println!(
            "{}",
            serde_json::to_string(&crate::diagnostics::evaluate(*kind, snapshot)?)?
        );
        return Ok(());
    }
    if let Command::Hunter { snapshot } = &cli.cmd {
        let request = crate::research::read_json::<crate::hunter::HunterRequest>(snapshot)?;
        println!(
            "{}",
            serde_json::to_string(&crate::hunter::evaluate(&request)?)?
        );
        return Ok(());
    }
    if let Command::SynapsePaper {
        candles,
        symbol,
        equity,
        json,
    } = &cli.cmd
    {
        let bars = crate::research::read_json::<Vec<crate::core::Candle>>(candles)?;
        anyhow::ensure!(!bars.is_empty(), "candles input is empty");
        let start_eq: rust_decimal::Decimal = equity
            .parse()
            .with_context(|| format!("invalid starting equity '{equity}'"))?;
        let mut pipe = crate::paper_pipeline::SynapsePipeline::new(
            crate::paper_pipeline::SynapsePipelineConfig {
                starting_equity: start_eq,
                ..Default::default()
            },
        )?;
        let sym = symbol.trim().to_ascii_uppercase();
        let mut hist = Vec::with_capacity(bars.len());
        let mut last_marks = std::collections::BTreeMap::new();
        for bar in bars {
            let now_ms = bar.ts.timestamp_millis();
            hist.push(bar.clone());
            let mut candle_map = std::collections::BTreeMap::new();
            candle_map.insert(sym.clone(), hist.clone());
            let mut marks = std::collections::BTreeMap::new();
            if let Ok(d) = rust_decimal::Decimal::try_from(bar.close) {
                if d > rust_decimal::Decimal::ZERO {
                    marks.insert(sym.clone(), d.round_dp(6));
                }
            }
            last_marks = marks.clone();
            let obs = crate::hunter::MarketObservation {
                symbol: sym.clone(),
                observed_at_ms: now_ms,
                volume_24h_usd: Some((bar.volume * bar.close).max(2_000_000.0)),
                depth_usd: Some(250_000.0),
                spread_bps: Some(3.0),
                volatility_pct: Some(2.5),
                directionality: Some(0.68),
                funding_bps: Some(1.0),
                open_interest_usd: Some(8_000_000.0),
                coverage_fraction: Some(1.0),
                age_days: Some(90.0),
                observed_history_minutes: None,
                ema_ready: hist.len() >= 12,
            };
            pipe.step_cycle_with_provenance(
                now_ms,
                crate::synapse_bus::DataProvenance::HistoricalValidation,
                &[obs],
                &candle_map,
                &marks,
            )?;
        }
        let report = serde_json::json!({
            "schema_version": crate::synapse_bus::SYNAPSE_SCHEMA_VERSION,
            "simulation_only": crate::paper_pipeline::SIMULATION_ONLY,
            "provenance": "historical_validation_simulation",
            "trading_authorized": false,
            "symbol": sym,
            "cycles": pipe.cycle_count,
            "events_emitted": pipe.bus.total_published(),
            "starting_equity_usd": pipe.ledger.starting_equity.to_string(),
            "mtm_equity_usd": pipe.ledger.mark_to_market_equity(&last_marks).round_dp(2).to_string(),
            "realized_net_pnl_usd": pipe.ledger.realized_net_pnl_usd.round_dp(4).to_string(),
            "fees_usd": pipe.ledger.total_fees_usd.round_dp(4).to_string(),
            "closed_trades": pipe.ledger.closed_trades.len(),
            "open_positions": pipe.ledger.open_positions.len(),
            "breaker_reason": pipe.ledger.breaker_reason,
        });
        if *json {
            println!("{}", serde_json::to_string(&report)?);
        } else {
            println!(
                "SYNAPSE PAPER (SIMULATION ONLY) | {} cycles={} events={} equity=${} net_pnl=${} fees=${} closed={} open={}",
                sym,
                pipe.cycle_count,
                pipe.bus.total_published(),
                report["mtm_equity_usd"].as_str().unwrap_or("0"),
                report["realized_net_pnl_usd"].as_str().unwrap_or("0"),
                report["fees_usd"].as_str().unwrap_or("0"),
                pipe.ledger.closed_trades.len(),
                pipe.ledger.open_positions.len(),
            );
        }
        return Ok(());
    }
    if let Command::PaperService { action } = &cli.cmd {
        return handle_paper_service_cli(action).await;
    }
    if let Command::FangRegistry { json } = &cli.cmd {
        let reg = crate::fang_engine::default_strategy_registry();
        if *json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "registry_version": crate::fang_engine::REGISTRY_VERSION,
                    "strategies": reg,
                }))?
            );
        } else {
            println!(
                "FANG Strategy Registry ({}) — {} registered strategies",
                crate::fang_engine::REGISTRY_VERSION,
                reg.len()
            );
            for d in &reg {
                println!(
                    "- {} [{:?}] regimes={} | {}",
                    d.id,
                    d.family,
                    d.supported_regimes
                        .iter()
                        .map(|r| r.label())
                        .collect::<Vec<_>>()
                        .join(","),
                    d.activation_requirement
                );
            }
        }
        return Ok(());
    }
    if let Command::CognitionStats {
        samples,
        regime,
        stop_loss_bps,
        cost_bps,
    } = &cli.cmd
    {
        let items =
            crate::research::read_json::<Vec<crate::cognition_shadow::ShadowSample>>(samples)?;
        let strategy_ids: Vec<String> = crate::fang_engine::default_strategy_registry()
            .into_iter()
            .map(|d| d.id)
            .collect();
        let ranking = crate::cognition_stats::rank_and_weight_ensemble(
            chrono::Utc::now().timestamp_millis(),
            regime,
            crate::cognition_shadow::OutcomeSource::Simulated,
            &strategy_ids,
            &items,
            &crate::cognition_shadow::ShadowPolicy::default(),
            *stop_loss_bps,
            *cost_bps,
        )?;
        let drift: Vec<_> = strategy_ids
            .iter()
            .map(|s| {
                let seq: Vec<&crate::cognition_shadow::ShadowSample> =
                    items.iter().filter(|x| &x.strategy == s).collect();
                crate::cognition_stats::detect_strategy_deterioration(s, &seq, 5.0, 0.25)
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ensemble": ranking,
                "drift_alerts": drift,
            }))?
        );
        return Ok(());
    }
    if let Command::OraclePortfolio {
        dataset,
        folds,
        embargo_bars,
    } = &cli.cmd
    {
        let ds = crate::research::read_json::<crate::oracle_research::MultiMarketDataset>(dataset)?;
        let (ts, _) = ds.validate_and_fingerprint()?;
        let fang_cfg = crate::fang_engine::UnifiedStrategyConfig::default();
        let exec = crate::oracle_research::ExecutionModelConfig::default();
        let portfolio = crate::oracle_research::simulate_portfolio(&ds, &fang_cfg, &exec)?;
        let fold_count = (*folds).max(1);
        let span = ts.len() / fold_count;
        let test_bars = (span / 3).max(exec.warmup_bars + exec.latency_bars + 2);
        let train_bars = ts
            .len()
            .saturating_sub(test_bars + *embargo_bars)
            .max(exec.warmup_bars + 2);
        let wf = if ts.len() >= train_bars + *embargo_bars + test_bars {
            Some(crate::oracle_research::walk_forward_purged_cv(
                &ds,
                train_bars,
                *embargo_bars,
                test_bars,
                test_bars,
                &fang_cfg,
                &exec,
            )?)
        } else {
            None
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "portfolio": portfolio,
                "walk_forward": wf,
            }))?
        );
        return Ok(());
    }
    if let Command::GenesisNsga2 {
        dataset,
        train_bars,
        embargo_bars,
        population,
        generations,
        workers,
        seed,
    } = &cli.cmd
    {
        let ds = crate::research::read_json::<crate::oracle_research::MultiMarketDataset>(dataset)?;
        let opts = crate::genesis_nsga2::Nsga2Options {
            seed: *seed,
            population: *population,
            generations: *generations,
            max_workers: *workers,
            ..Default::default()
        };
        let rep =
            crate::genesis_nsga2::run_nsga2_campaign(&ds, *train_bars, *embargo_bars, &opts, None)?;
        println!("{}", serde_json::to_string_pretty(&rep)?);
        return Ok(());
    }
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
    if let Command::ValidateResearch { candles, options } = &cli.cmd {
        let cfg = AppConfig::load_research(&cli.config, cli.log_level.as_deref())?;
        let candles = crate::research::read_json::<Vec<crate::core::Candle>>(candles)?;
        let options = options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::oracle_validation::ValidationOptions>(p))
            .transpose()?
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&crate::oracle_validation::evaluate(
                &cfg, &candles, &options
            )?)?
        );
        return Ok(());
    }
    if let Command::AssessCandidates {
        training,
        held_out,
        checkpoint,
        options,
        validation_options,
        limit,
    } = &cli.cmd
    {
        let cfg = AppConfig::load_research(&cli.config, cli.log_level.as_deref())?;
        let training = crate::research::read_json::<Vec<crate::core::Candle>>(training)?;
        let held_out = crate::research::read_json::<Vec<crate::core::Candle>>(held_out)?;
        let checkpoint =
            crate::research::read_json::<crate::genesis::SearchCheckpoint>(checkpoint)?;
        let options = options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::genesis::SearchOptions>(p))
            .transpose()?
            .unwrap_or_default();
        let validation = validation_options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::oracle_validation::ValidationOptions>(p))
            .transpose()?
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&crate::genesis_oos::evaluate(
                &cfg,
                &training,
                &held_out,
                &options,
                &checkpoint,
                &validation,
                *limit
            )?)?
        );
        return Ok(());
    }
    if let Command::OptimizeStep {
        candles,
        options,
        checkpoint,
        output,
        generations,
    } = &cli.cmd
    {
        let cfg = AppConfig::load_research(&cli.config, cli.log_level.as_deref())?;
        let bars = crate::research::read_json::<Vec<crate::core::Candle>>(candles)?;
        let options = options
            .as_ref()
            .map(|p| crate::research::read_json::<crate::genesis::SearchOptions>(p))
            .transpose()?
            .unwrap_or_default();
        let checkpoint = checkpoint
            .as_ref()
            .map(|p| crate::research::read_json::<crate::genesis::SearchCheckpoint>(p))
            .transpose()?;
        let progress =
            crate::genesis::advance(&cfg, &bars, &options, checkpoint.as_ref(), *generations)?;
        crate::genesis_checkpoint::save_new(output, &progress.checkpoint)?;
        println!("{}", serde_json::to_string(&progress)?);
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
    if let Command::IntentEvidence { db, intent, limit } = &cli.cmd {
        let evidence = crate::intent_evidence::inspect(db, intent, *limit)?;
        println!("{}", serde_json::to_string(&evidence)?);
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
        Command::Diagnose { .. }
        | Command::Hunter { .. }
        | Command::ValidateResearch { .. }
        | Command::Backtest { .. }
        | Command::Optimize { .. }
        | Command::OptimizeStep { .. }
        | Command::AssessCandidates { .. } => {
            unreachable!("offline research handled before engine storage setup")
        }
        Command::InitHistory { .. } => unreachable!("handled before configuration setup"),
        Command::Telemetry { .. } => unreachable!("handled before configuration and storage setup"),
        Command::StatusHistory { .. } => {
            unreachable!("handled before configuration and storage setup")
        }
        Command::IntentEvidence { .. } => {
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
        Command::SynapsePaper { .. }
        | Command::PaperService { .. }
        | Command::FangRegistry { .. }
        | Command::CognitionStats { .. }
        | Command::OraclePortfolio { .. }
        | Command::GenesisNsga2 { .. } => {
            unreachable!("handled above")
        }
    }
}

async fn handle_paper_service_cli(action: &PaperServiceAction) -> Result<()> {
    let open_store = |db: Option<&std::path::PathBuf>| -> Result<crate::paper_store::PaperStore> {
        match db {
            Some(p) => crate::paper_store::PaperStore::open(p),
            None => crate::paper_store::PaperStore::open_default(),
        }
    };
    let resolve_root = |root: Option<&std::path::PathBuf>| -> Result<std::path::PathBuf> {
        match root {
            Some(r) => Ok(r.clone()),
            None => crate::paper_store::PaperStore::default_root(),
        }
    };

    match action {
        PaperServiceAction::Create {
            label,
            kind,
            equity,
            db,
        } => {
            let store = open_store(db.as_ref())?;
            let k = crate::paper_store::PaperSessionKind::from_db(kind)?;
            let start_eq: rust_decimal::Decimal = equity
                .parse()
                .with_context(|| format!("invalid starting equity '{equity}'"))?;
            let cfg = crate::paper_pipeline::SynapsePipelineConfig {
                starting_equity: start_eq,
                ..Default::default()
            };
            let s = store.create_session(k, label, cfg)?;
            println!("{}", serde_json::to_string_pretty(&s)?);
        }
        PaperServiceAction::List { kind, db } => {
            let store = open_store(db.as_ref())?;
            let filter = kind
                .as_deref()
                .map(crate::paper_store::PaperSessionKind::from_db)
                .transpose()?;
            let list = store.list_sessions(filter)?;
            println!("{}", serde_json::to_string_pretty(&list)?);
        }
        PaperServiceAction::Run {
            session,
            root,
            db,
            interval_s,
            ws_url,
            info_url,
        } => {
            let store = open_store(db.as_ref())?;
            let svc_root = resolve_root(root.as_ref())?;
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stop_sig = stop.clone();
            let _ = ctrlc::set_handler(move || {
                stop_sig.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            let res = crate::paper_service::run_paper_session_service(
                crate::paper_service::PaperServiceConfig {
                    root: svc_root,
                    store,
                    session_id: session.clone(),
                    ws_url: ws_url
                        .clone()
                        .unwrap_or_else(|| crate::hunter_stream::HYPERLIQUID_PUBLIC_WS_URL.into()),
                    info_url: info_url
                        .clone()
                        .unwrap_or_else(|| crate::hunter::HYPERLIQUID_PUBLIC_INFO_URL.into()),
                    tick_interval_s: *interval_s,
                    max_ticks: None,
                },
                stop,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&res)?);
        }
        PaperServiceAction::Start {
            session,
            root,
            db,
            interval_s,
        } => {
            let store = open_store(db.as_ref())?;
            let svc_root = resolve_root(root.as_ref())?;
            let sid = match session {
                Some(s) => s.clone(),
                None => {
                    store
                        .get_or_create_session(
                            crate::paper_store::PaperSessionKind::PublicPaper,
                            "Public Market Paper",
                            Default::default(),
                        )?
                        .session_id
                }
            };
            let pid = crate::paper_service::PaperServiceClient::spawn_detached(
                &svc_root,
                store.db_path(),
                &sid,
                *interval_s,
            )?;
            println!(
                "{}",
                serde_json::json!({"started": true, "session_id": sid, "pid": pid, "simulation_only": true})
            );
        }
        PaperServiceAction::Status { session, root, db } => {
            let store = open_store(db.as_ref())?;
            let svc_root = resolve_root(root.as_ref())?;
            let summary = store.get_session(session)?;
            let live = crate::paper_service::PaperServiceClient::send_command(
                &svc_root,
                session,
                crate::paper_service::PaperControlCommand::Status,
            )
            .ok();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "session": summary,
                    "live_ipc": live,
                }))?
            );
        }
        PaperServiceAction::Pause { session, root } => {
            let svc_root = resolve_root(root.as_ref())?;
            let resp = crate::paper_service::PaperServiceClient::send_command(
                &svc_root,
                session,
                crate::paper_service::PaperControlCommand::Pause,
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        PaperServiceAction::Resume { session, root } => {
            let svc_root = resolve_root(root.as_ref())?;
            let resp = crate::paper_service::PaperServiceClient::send_command(
                &svc_root,
                session,
                crate::paper_service::PaperControlCommand::Resume,
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        PaperServiceAction::Stop { session, root } => {
            let svc_root = resolve_root(root.as_ref())?;
            let resp = crate::paper_service::PaperServiceClient::send_command(
                &svc_root,
                session,
                crate::paper_service::PaperControlCommand::Stop,
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        PaperServiceAction::Archive { session, root, db } => {
            let store = open_store(db.as_ref())?;
            let svc_root = resolve_root(root.as_ref())?;
            store.archive_session_guarded(&svc_root, session)?;
            println!("{}", serde_json::json!({"archived": session}));
        }
        PaperServiceAction::Reset {
            session,
            confirm,
            root,
            db,
        } => {
            let store = open_store(db.as_ref())?;
            let svc_root = resolve_root(root.as_ref())?;
            let reset = store.reset_session_confirmed_guarded(&svc_root, session, confirm)?;
            println!("{}", serde_json::to_string_pretty(&reset)?);
        }
    }
    Ok(())
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

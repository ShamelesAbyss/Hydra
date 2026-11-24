//! Application configuration: merges hardcoded defaults + env + TOML strategy config.

use anyhow::{bail, Context, Result};
use owo_colors::OwoColorize;
use serde::Deserialize;
use std::path::PathBuf;

pub mod defaults;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub live_trading: bool,

    pub hl_private_key: Option<String>,
    pub hl_account_address: Option<String>,

    pub risk: RiskConfig,
    pub compound: CompoundConfig,

    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_level: String,
    pub poll_interval_ms: u64,
    pub shutdown_mode: ShutdownMode,

    pub strategies_path: PathBuf,
    pub strategies: StrategyConfig,
}

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub default_risk_pct: f64,
    pub max_daily_loss_pct: f64,
    pub max_weekly_loss_pct: f64,
    pub max_drawdown_pct: f64,
    pub max_consecutive_losses: u32,
    pub max_open_exposure_pct: f64,
    pub max_notional_per_market_usd: f64,
    pub min_liquidity_usd: f64,
    pub max_spread_bps: f64,
    pub max_slippage_bps: f64,
    /// v1.3.4: rolling window for the consecutive-loss circuit breaker.
    /// Losses older than this many hours age out automatically. Default 4.
    pub consecutive_loss_window_hours: i64,
}

#[derive(Debug, Clone)]
pub struct CompoundConfig {
    pub reinvest_pct: f64,
    pub lock_pct: f64,
    pub min_profit_usd: f64,
    pub drawdown_reduce_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownMode {
    KeepOpen,
    Reduce,
    Flatten,
}

impl ShutdownMode {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "keep_open" | "keep" | "hold" => Self::KeepOpen,
            "reduce" => Self::Reduce,
            "flatten" | "close" => Self::Flatten,
            other => bail!("unknown SHUTDOWN_MODE '{other}'"),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub global: GlobalStrategyCfg,
    pub divergence: DivergenceCfg,
    pub momentum: MomentumCfg,
    pub mean_reversion: MeanReversionCfg,
    pub regime: RegimeCfg,
    pub position_management: PositionMgmtCfg,
    #[serde(default)]
    pub filters: FiltersCfg,
    /// v1.5.0: adaptive learning configuration. All defaults are sane —
    /// the entire system can be turned off with `enabled = false` for the
    /// rare case the user wants the legacy fixed-rule behaviour back.
    #[serde(default)]
    pub learning: LearningCfg,
    /// v1.4.0: per-symbol override map. Symbols not listed inherit the
    /// global defaults. Lets us size BTC bigger (best win-rate) and HYPE
    /// smaller (newer / wider spreads) without changing global config.
    #[serde(default)]
    pub per_symbol: std::collections::HashMap<String, SymbolOverride>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SymbolOverride {
    /// Multiplier on the final position-size after risk sizing has run.
    /// 1.0 = no change, 1.2 = +20% size, 0.8 = −20% size.
    /// Clamped to [0.1, 2.0] for safety.
    pub position_frac_mult: Option<f64>,
    /// Per-symbol override for `max_spread_bps`. Useful for tokens like HYPE
    /// where the typical spread is wider than BTC/ETH.
    pub max_spread_bps: Option<f64>,
    /// Optional per-symbol cap on notional. Falls back to the global
    /// `max_notional_per_market_usd` if unset.
    pub max_notional_usd: Option<f64>,
    /// Optional per-symbol risk percentage. Defaults to global
    /// `default_risk_pct` if unset.
    pub default_risk_pct: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalStrategyCfg {
    pub enabled_strategies: Vec<String>,
    pub enabled_venues: Vec<String>,
    pub default_symbols: Vec<String>,
    pub candle_interval: String,
    pub lookback_candles: usize,
    pub regime_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DivergenceCfg {
    pub enabled: bool,
    pub rsi_period: usize,
    pub rsi_threshold: f64,
    pub min_edge_bps: f64,
    pub position_frac: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MomentumCfg {
    pub enabled: bool,
    pub fast_ema: usize,
    pub slow_ema: usize,
    pub atr_period: usize,
    pub breakout_lookback: usize,
    pub min_atr_pct: f64,
    pub position_frac: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeanReversionCfg {
    pub enabled: bool,
    pub bb_period: usize,
    pub bb_std: f64,
    pub rsi_floor: f64,
    pub rsi_ceiling: f64,
    pub position_frac: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegimeCfg {
    pub trend_ema_period: usize,
    pub chop_atr_threshold: f64,
    pub panic_atr_threshold: f64,
    pub vol_spike_mult: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionMgmtCfg {
    pub stop_loss_atr_mult: f64,
    pub take_profit_atr_mult: f64,
    pub trailing_enabled: bool,
    pub trailing_atr_mult: f64,
    pub time_stop_minutes: i64,
    pub partial_tp_enabled: bool,
    pub partial_tp_fraction: f64,
    /// v1.3.2 chandelier trailing: once unrealized profit reaches this many
    /// multiples of the initial risk R, the stop is pulled to entry
    /// (break-even) and thereafter trails at `peak − chandelier_trail_r × R`.
    /// Set to 0 to disable.
    #[serde(default = "default_breakeven_trigger_r")]
    pub breakeven_trigger_r: f64,
    /// How many R below the peak the chandelier stop trails at.
    #[serde(default = "default_chandelier_trail_r")]
    pub chandelier_trail_r: f64,
    /// v1.6.1: Absolute minimum stop distance in basis points of entry
    /// price. Stop will be placed at `max(atr_mult × ATR, 4 × spread, this)`.
    /// Prevents bid/ask noise from taking out trades on low-ATR markets
    /// like HYPE (where ATR can be 5-10× tighter than BTC/ETH).
    /// Default 25 bps. Set to 0 to revert to pure-ATR stops.
    #[serde(default = "default_min_stop_bps")]
    pub min_stop_bps: f64,
    /// v1.6.1: After a stop-out close, block re-entries on the SAME
    /// (symbol, side) for this many minutes. Stacks on top of the global
    /// `[filters] reentry_cooldown_minutes` (which fires on any losing
    /// close). This specifically prevents the "chop death" pattern where
    /// a stop-out gets immediately followed by another entry that
    /// stops out again 3 min later. Default 45 min.
    #[serde(default = "default_stopout_cooldown_minutes")]
    pub stopout_cooldown_minutes: i64,
}

fn default_breakeven_trigger_r() -> f64 {
    0.8
}
fn default_chandelier_trail_r() -> f64 {
    0.8
}
fn default_min_stop_bps() -> f64 {
    25.0
}
fn default_stopout_cooldown_minutes() -> i64 {
    45
}

/// v1.3.3 strategy-gating filters. Sit in front of the risk engine and block
/// counter-trend signals + immediate re-entries on a losing setup.
#[derive(Debug, Clone, Deserialize)]
pub struct FiltersCfg {
    /// Minutes to wait before re-entering the same (symbol, side) after a
    /// losing close. 0 disables the cooldown. Default 45.
    #[serde(default = "default_reentry_cooldown")]
    pub reentry_cooldown_minutes: i64,
    /// Block mean-reversion signals that fight the long-period EMA direction.
    /// Default true.
    #[serde(default = "default_trend_filter_enabled")]
    pub trend_filter_enabled: bool,
    /// Period for the trend EMA on the strategy's candle interval. With 1m
    /// candles, 240 ≈ 4-hour smoothed level. Default 240.
    #[serde(default = "default_trend_ema_period")]
    pub trend_filter_ema_period: usize,
    /// v1.3.6 deadband: trend filter ignores diffs smaller than this percent.
    /// Mean-reversion is INHERENTLY counter-trend, so the binary above/below
    /// check disabled the strategy entirely. The deadband lets the filter
    /// only fire on real directional drifts (e.g. >0.3% from EMA), not noise.
    #[serde(default = "default_trend_band_pct")]
    pub trend_filter_band_pct: f64,
    /// v1.4.4 minimum net edge in basis points required AFTER fees + spread.
    /// HL taker round-trip fees are ~7bps; with default 20bps, we require
    /// signals to predict at least ~13bps gross edge over costs. Filters
    /// out the "penny win, fee loss" trades that quietly bleed the account.
    /// Set 0 to disable.
    #[serde(default = "default_min_net_edge_bps")]
    pub min_net_edge_bps: f64,
    /// v1.4.4 minimum notional per trade in USD. Below this, the bot skips
    /// the signal entirely. Default $50 — at that size, 7bps fees = $0.035,
    /// so a $0.30 win is still 8.5x the fee cost. Lower for very small
    /// accounts; higher for fee discipline. Was effectively $10 in v1.4.3.
    #[serde(default = "default_min_notional_usd")]
    pub min_notional_usd: f64,
}

fn default_reentry_cooldown() -> i64 {
    30
}
fn default_trend_filter_enabled() -> bool {
    true
}
fn default_trend_ema_period() -> usize {
    240
}
fn default_trend_band_pct() -> f64 {
    0.4
}
fn default_min_net_edge_bps() -> f64 {
    10.0
}
fn default_min_notional_usd() -> f64 {
    30.0
}

/// v1.5.0 adaptive learning config. The framework reads expectancy +
/// regime + memory and produces a `SignalDecision` with size_mult and an
/// optional edge-threshold delta. Everything here is tunable from the user's
/// `strategies.toml` so logs can be matched against the same numbers later.
#[derive(Debug, Clone, Deserialize)]
pub struct LearningCfg {
    /// Master switch. False = pure legacy v1.4.x behaviour, no adaptive
    /// adjustments at all. Default true.
    #[serde(default = "default_learning_enabled")]
    pub enabled: bool,
    /// Whether `SimilarityVerdict::is_blocking` is allowed to hard-refuse
    /// a signal. Set false to let memory be informational-only and still
    /// take the trade (great for collecting more data after a config change).
    /// Default true.
    #[serde(default = "default_memory_block_enabled")]
    pub memory_block_enabled: bool,
    /// Minimum number of similar past trades required before the memory
    /// verdict is allowed to block. Default 10.
    #[serde(default = "default_memory_min_matches")]
    pub memory_min_matches: usize,
    /// Block threshold: similar past trades' win-rate must be BELOW this
    /// (and net_pnl < 0) to block. Default 30 (i.e. 30%).
    #[serde(default = "default_memory_block_winrate_pct")]
    pub memory_block_winrate_pct: f64,
    /// Apply size_mult from `AdaptiveContext`. False = use multiplier of
    /// 1.0 always (size never adjusted by learning). Default true.
    #[serde(default = "default_size_mult_enabled")]
    pub size_mult_enabled: bool,
    /// Apply the adaptive edge-threshold delta. False = always use the
    /// configured `min_net_edge_bps`. Default true.
    #[serde(default = "default_edge_override_enabled")]
    pub edge_override_enabled: bool,

    // ── v1.6.0 Adaptive Strictness ───────────────────────────────────────
    /// Master switch for the strictness layer. False = legacy v1.5.x gate
    /// behaviour (no ATR scaling, no drought detector, no exploration).
    /// Each subsystem can also be toggled individually below.
    #[serde(default = "default_strictness_enabled")]
    pub strictness_enabled: bool,
    /// Scale `min_net_edge_bps` and trend-filter band based on current ATR%.
    /// Sleeping markets (ATR < 0.10%) loosen by 5bps + 1.0% band; calm
    /// markets (ATR < 0.20%) loosen by 3bps + 0.8% band; hot markets
    /// (ATR > 0.50%) tighten by 2bps. Default true.
    #[serde(default = "default_atr_aware_scaling")]
    pub atr_aware_scaling: bool,
    /// Track per-symbol trend-filter blocks. After `drought_block_threshold`
    /// blocks within `drought_window_hours` and zero fills, auto-bypass the
    /// trend filter for the symbol for `drought_bypass_hours`. Cleared on
    /// any fill. Default true.
    #[serde(default = "default_drought_detector")]
    pub drought_detector: bool,
    #[serde(default = "default_drought_block_threshold")]
    pub drought_block_threshold: usize,
    #[serde(default = "default_drought_window_hours")]
    pub drought_window_hours: i64,
    #[serde(default = "default_drought_bypass_hours")]
    pub drought_bypass_hours: i64,
    /// ε-greedy exploration: every Nth signal that *only* fails the trend
    /// filter is taken anyway, downsized to 0.5×. Pure data collection;
    /// logged distinctly as "explored:". Default true with N=8.
    #[serde(default = "default_explore_enabled")]
    pub explore_enabled: bool,
    #[serde(default = "default_explore_one_in_n")]
    pub explore_one_in_n: u64,
    #[serde(default = "default_explore_size_factor")]
    pub explore_size_factor: f64,
}

fn default_learning_enabled() -> bool {
    true
}
fn default_memory_block_enabled() -> bool {
    true
}
fn default_memory_min_matches() -> usize {
    10
}
fn default_memory_block_winrate_pct() -> f64 {
    30.0
}
fn default_size_mult_enabled() -> bool {
    true
}
fn default_edge_override_enabled() -> bool {
    true
}

// v1.6.0 Adaptive Strictness defaults
fn default_strictness_enabled() -> bool {
    true
}
fn default_atr_aware_scaling() -> bool {
    true
}
fn default_drought_detector() -> bool {
    true
}
fn default_drought_block_threshold() -> usize {
    5
}
fn default_drought_window_hours() -> i64 {
    4
}
fn default_drought_bypass_hours() -> i64 {
    2
}
fn default_explore_enabled() -> bool {
    true
}
fn default_explore_one_in_n() -> u64 {
    8
}
fn default_explore_size_factor() -> f64 {
    0.5
}

impl Default for LearningCfg {
    fn default() -> Self {
        Self {
            enabled: default_learning_enabled(),
            memory_block_enabled: default_memory_block_enabled(),
            memory_min_matches: default_memory_min_matches(),
            memory_block_winrate_pct: default_memory_block_winrate_pct(),
            size_mult_enabled: default_size_mult_enabled(),
            edge_override_enabled: default_edge_override_enabled(),
            strictness_enabled: default_strictness_enabled(),
            atr_aware_scaling: default_atr_aware_scaling(),
            drought_detector: default_drought_detector(),
            drought_block_threshold: default_drought_block_threshold(),
            drought_window_hours: default_drought_window_hours(),
            drought_bypass_hours: default_drought_bypass_hours(),
            explore_enabled: default_explore_enabled(),
            explore_one_in_n: default_explore_one_in_n(),
            explore_size_factor: default_explore_size_factor(),
        }
    }
}

impl Default for FiltersCfg {
    fn default() -> Self {
        Self {
            reentry_cooldown_minutes: default_reentry_cooldown(),
            trend_filter_enabled: default_trend_filter_enabled(),
            trend_filter_ema_period: default_trend_ema_period(),
            trend_filter_band_pct: default_trend_band_pct(),
            min_net_edge_bps: default_min_net_edge_bps(),
            min_notional_usd: default_min_notional_usd(),
        }
    }
}

fn env_opt(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty())
}
fn env_f(k: &str, d: f64) -> f64 {
    env_opt(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn env_u64(k: &str, d: u64) -> u64 {
    env_opt(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn env_u32(k: &str, d: u32) -> u32 {
    env_opt(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn env_bool(k: &str, d: bool) -> bool {
    match env_opt(k).map(|v| v.to_lowercase()) {
        Some(v) if ["1", "true", "yes", "on", "y"].contains(&v.as_str()) => true,
        Some(v) if ["0", "false", "no", "off", "n"].contains(&v.as_str()) => false,
        _ => d,
    }
}

impl AppConfig {
    pub fn load(strategies_path: &str, log_level_override: Option<&str>) -> Result<Self> {
        let strat_raw = std::fs::read_to_string(strategies_path)
            .with_context(|| format!("reading strategy config at {strategies_path}"))?;
        let strategies: StrategyConfig =
            toml::from_str(&strat_raw).context("parsing strategies.toml")?;

        // v1.4.3: ETH RPC pool removed — single-venue Hyperliquid only.

        let risk = RiskConfig {
            default_risk_pct: env_f("DEFAULT_RISK_PCT", 0.5),
            max_daily_loss_pct: env_f("MAX_DAILY_LOSS_PCT", 3.0),
            max_weekly_loss_pct: env_f("MAX_WEEKLY_LOSS_PCT", 8.0),
            max_drawdown_pct: env_f("MAX_DRAWDOWN_PCT", 15.0),
            max_consecutive_losses: env_u32("MAX_CONSECUTIVE_LOSSES", 4),
            max_open_exposure_pct: env_f("MAX_OPEN_EXPOSURE_PCT", 40.0),
            max_notional_per_market_usd: env_f("MAX_NOTIONAL_PER_MARKET_USD", 2500.0),
            min_liquidity_usd: env_f("MIN_LIQUIDITY_USD", 50000.0),
            max_spread_bps: env_f("MAX_SPREAD_BPS", 25.0),
            max_slippage_bps: env_f("MAX_SLIPPAGE_BPS", 30.0),
            consecutive_loss_window_hours: env_opt("CONSECUTIVE_LOSS_WINDOW_HOURS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        };
        let compound = CompoundConfig {
            reinvest_pct: env_f("COMPOUND_REINVEST_PCT", 50.0),
            lock_pct: env_f("COMPOUND_LOCK_PCT", 25.0),
            min_profit_usd: env_f("COMPOUND_MIN_PROFIT_USD", 25.0),
            drawdown_reduce_pct: env_f("COMPOUND_DRAWDOWN_REDUCE_PCT", 40.0),
        };

        let data_dir: PathBuf = env_opt("DATA_DIR")
            .unwrap_or_else(|| "./data".into())
            .into();
        let log_dir: PathBuf = env_opt("LOG_DIR").unwrap_or_else(|| "./logs".into()).into();
        let db_path: PathBuf = env_opt("DB_PATH")
            .unwrap_or_else(|| data_dir.join("hydra.db").to_string_lossy().into_owned())
            .into();
        std::fs::create_dir_all(&data_dir).ok();
        std::fs::create_dir_all(&log_dir).ok();

        let cfg = AppConfig {
            live_trading: env_bool("ENABLE_LIVE_TRADING", false),
            hl_private_key: env_opt("HYPERLIQUID_PRIVATE_KEY"),
            hl_account_address: env_opt("HYPERLIQUID_ACCOUNT_ADDRESS"),

            risk,
            compound,
            data_dir,
            log_dir,
            db_path,
            log_level: log_level_override
                .map(|s| s.into())
                .or_else(|| env_opt("LOG_LEVEL"))
                .unwrap_or_else(|| "info".into()),
            poll_interval_ms: env_u64("POLL_INTERVAL_MS", 1500),
            shutdown_mode: ShutdownMode::parse(
                &env_opt("SHUTDOWN_MODE").unwrap_or_else(|| "keep_open".into()),
            )?,

            strategies_path: PathBuf::from(strategies_path),
            strategies,
        };
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.live_trading && self.hl_private_key.is_none() {
            bail!("live trading enabled but HYPERLIQUID_PRIVATE_KEY is not configured");
        }
        if self.risk.default_risk_pct <= 0.0 || self.risk.default_risk_pct > 5.0 {
            bail!("DEFAULT_RISK_PCT must be in (0, 5]");
        }
        if self.compound.reinvest_pct + self.compound.lock_pct > 100.0 {
            bail!("COMPOUND_REINVEST_PCT + COMPOUND_LOCK_PCT cannot exceed 100%");
        }
        if self.strategies.global.enabled_strategies.is_empty() {
            bail!("no strategies enabled in strategies.toml");
        }
        println!("{}", "✔ config OK".bright_green().bold());
        Ok(())
    }

    pub fn print_summary(&self) {
        use owo_colors::OwoColorize;
        let h = |s: &str| format!("{}", s.bright_cyan().bold());
        println!();
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━ HYDRA · configuration ━━━━━━━━━━━━━━━━━━━"
                .bright_magenta()
                .bold()
        );
        println!(
            " {:<22} {}",
            h("live_trading"),
            if self.live_trading {
                "TRUE  (real orders will be submitted)"
                    .bright_red()
                    .bold()
                    .to_string()
            } else {
                "false (dry-run)".yellow().to_string()
            }
        );
        println!(
            " {:<22} {}",
            h("hl_account"),
            self.hl_account_address
                .clone()
                .unwrap_or_else(|| "(derived from key)".into())
        );
        println!(" {:<22} {}", h("db_path"), self.db_path.display());
        println!(" {:<22} {}", h("data_dir"), self.data_dir.display());
        println!(" {:<22} {}", h("log_dir"), self.log_dir.display());
        println!(" {:<22} {}ms", h("poll_interval"), self.poll_interval_ms);
        println!(" {:<22} {:?}", h("shutdown_mode"), self.shutdown_mode);
        println!(
            " {:<22} {:?}",
            h("strategies"),
            self.strategies.global.enabled_strategies
        );
        println!(
            " {:<22} {:?}",
            h("venues"),
            self.strategies.global.enabled_venues
        );
        println!(
            " {:<22} {:?}",
            h("symbols"),
            self.strategies.global.default_symbols
        );
        println!();
        println!("{}", "─── risk caps ─".bright_magenta());
        println!(
            "   default_risk_pct   : {}",
            format!("{}%", self.risk.default_risk_pct).bright_yellow()
        );
        println!(
            "   max_daily_loss     : {}",
            format!("{}%", self.risk.max_daily_loss_pct).bright_yellow()
        );
        println!(
            "   max_weekly_loss    : {}",
            format!("{}%", self.risk.max_weekly_loss_pct).bright_yellow()
        );
        println!(
            "   max_notional/mkt   : {}",
            format!("${:.0}", self.risk.max_notional_per_market_usd).bright_yellow()
        );
        println!(
            "   max_open_exposure  : {}",
            format!("{}%", self.risk.max_open_exposure_pct).bright_yellow()
        );
        println!("{}", "─── compounding ─".bright_magenta());
        println!(
            "   reinvest_pct       : {}",
            format!("{}%", self.compound.reinvest_pct).bright_yellow()
        );
        println!(
            "   lock_pct           : {}",
            format!("{}%", self.compound.lock_pct).bright_yellow()
        );
        println!(
            "   min_profit_usd     : {}",
            format!("${}", self.compound.min_profit_usd).bright_yellow()
        );
        println!();
    }
}

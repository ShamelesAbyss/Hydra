//! Deterministic single-instrument candle research. Signals see completed bars only.
use crate::{
    cognition_shadow::{infer_regime, RegimeInferenceConfig},
    config::AppConfig,
    core::{Candle, MarketSnapshot, Side, Signal},
    fang_engine::{FangMarketInput, FangUnifiedPlanner, UnifiedStrategyConfig},
    fang_planner::PlanningBudget,
    strategies::{make_strategies, regime::classify_regime, Strategy},
};
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayOptions {
    pub symbol: String,
    pub initial_equity: f64,
    pub risk_fraction: f64,
    pub max_exposure_fraction: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub warmup_bars: usize,
    pub max_bars: usize,
    pub max_trades: usize,
}
impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            symbol: "ETH".into(),
            initial_equity: 1000.0,
            risk_fraction: 0.005,
            max_exposure_fraction: 0.5,
            fee_bps: 4.5,
            slippage_bps: 5.0,
            warmup_bars: 240,
            max_bars: 100_000,
            max_trades: 10_000,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SimulatedTrade {
    pub strategy: String,
    pub side: Side,
    pub signal_time_ms: i64,
    pub entry_time_ms: i64,
    pub exit_time_ms: i64,
    pub quantity: f64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub gross_pnl: f64,
    pub fees: f64,
    pub net_pnl: f64,
    pub exit_reason: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct EquityPoint {
    pub time_ms: i64,
    pub equity: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReplayReport {
    pub schema_version: u32,
    pub simulated: bool,
    pub symbol: String,
    pub options: ReplayOptions,
    pub data_start_ms: i64,
    pub data_end_ms: i64,
    pub bar_count: usize,
    pub initial_equity: f64,
    pub final_equity: f64,
    pub max_drawdown_fraction: f64,
    pub total_fees: f64,
    pub trades: Vec<SimulatedTrade>,
    pub equity: Vec<EquityPoint>,
    pub rejected_entries: usize,
    pub model_limitations: Vec<String>,
}
struct Position {
    signal: Signal,
    quantity: f64,
    entry: f64,
    entry_ms: i64,
    entry_fee: f64,
}

pub fn replay(
    cfg: &AppConfig,
    candles: &[Candle],
    options: &ReplayOptions,
) -> Result<ReplayReport> {
    let strategies = make_strategies(&cfg.strategies)?;
    replay_with_strategies(cfg, candles, options, &strategies)
}

/// Explicit strategy injection supports deterministic fault and lookahead tests.
pub fn replay_with_strategies(
    cfg: &AppConfig,
    candles: &[Candle],
    o: &ReplayOptions,
    strategies: &[Box<dyn Strategy>],
) -> Result<ReplayReport> {
    ensure!(
        !o.symbol.is_empty() && o.symbol.len() <= 64,
        "invalid symbol"
    );
    ensure!(
        o.max_bars > 0 && o.max_bars <= 1_000_000 && candles.len() <= o.max_bars,
        "bar resource limit exceeded"
    );
    ensure!(
        o.max_trades > 0 && o.max_trades <= 100_000,
        "invalid trade resource budget"
    );
    ensure!(
        o.warmup_bars > 0 && candles.len() > o.warmup_bars,
        "insufficient warmup/data"
    );
    ensure!(
        o.initial_equity.is_finite() && o.initial_equity > 0.0,
        "invalid starting equity"
    );
    ensure!(
        o.risk_fraction.is_finite() && o.risk_fraction > 0.0 && o.risk_fraction <= 0.1,
        "invalid risk fraction"
    );
    ensure!(
        o.max_exposure_fraction.is_finite()
            && o.max_exposure_fraction > 0.0
            && o.max_exposure_fraction <= 1.0,
        "unleveraged exposure must be in (0,1]"
    );
    ensure!(
        [o.fee_bps, o.slippage_bps]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1000.0),
        "invalid execution costs"
    );
    let lookback = cfg.strategies.global.lookback_candles;
    ensure!(
        lookback > 0 && lookback <= 10_000,
        "invalid lookback resource budget"
    );
    ensure!(!strategies.is_empty(), "no strategies supplied");
    let evaluation_budget = (candles.len() - o.warmup_bars)
        .checked_mul(lookback.min(candles.len()))
        .and_then(|work| work.checked_mul(strategies.len()));
    ensure!(
        evaluation_budget.is_some_and(|work| work <= 100_000_000),
        "strategy history evaluation resource budget exceeded"
    );
    for (index, c) in candles.iter().enumerate() {
        ensure!(
            [c.open, c.high, c.low, c.close]
                .iter()
                .all(|p| p.is_finite() && *p > 0.0)
                && c.volume.is_finite()
                && c.volume >= 0.0
                && c.low <= c.open.min(c.close)
                && c.high >= c.open.max(c.close)
                && c.low <= c.high,
            "invalid OHLCV at bar {index}"
        );
        ensure!(
            index == 0 || candles[index - 1].ts < c.ts,
            "timestamps must increase strictly"
        );
    }
    let mut report = ReplayReport {
        schema_version: 1,
        simulated: true,
        symbol: o.symbol.clone(),
        options: o.clone(),
        data_start_ms: candles.first().unwrap().ts.timestamp_millis(),
        data_end_ms: candles.last().unwrap().ts.timestamp_millis(),
        bar_count: candles.len(),
        initial_equity: o.initial_equity,
        final_equity: o.initial_equity,
        max_drawdown_fraction: 0.0,
        total_fees: 0.0,
        trades: Vec::new(),
        equity: Vec::new(),
        rejected_entries: 0,
        model_limitations: vec![
            "Single instrument, full next-bar fills, fixed fee/slippage assumptions".into(),
            "Stop wins when stop and target both touch; gaps use adverse open".into(),
            "Funding, order-book queue, partial fills, liquidation, adaptive learning, trailing/time stops and portfolio margin are not modeled".into(),
            "Existing signal strategies are reused; live risk/execution parity is not certified".into(),
        ],
    };
    let mut cash = o.initial_equity;
    let mut peak = cash;
    let mut position: Option<Position> = None;
    let mut pending: Option<Signal> = None;
    for index in o.warmup_bars..candles.len() {
        let bar = &candles[index];
        if let Some(signal) = pending.take() {
            let entry = adverse(bar.open, signal.side.is_buy(), o.slippage_bps);
            let distance = (entry - signal.stop_price).abs();
            let directional = if signal.side.is_buy() {
                signal.stop_price < entry && signal.take_profit > entry
            } else {
                signal.stop_price > entry && signal.take_profit < entry
            };
            if cash > 0.0 && directional && distance > 0.0 {
                let quantity = (cash * o.risk_fraction / distance)
                    .min(cash * o.max_exposure_fraction / (entry * (1.0 + o.fee_bps / 10_000.0)));
                let entry_fee = quantity * entry * o.fee_bps / 10_000.0;
                ensure!(
                    quantity.is_finite() && entry_fee.is_finite(),
                    "sizing overflow"
                );
                cash -= entry_fee;
                position = Some(Position {
                    signal,
                    quantity,
                    entry,
                    entry_ms: bar.ts.timestamp_millis(),
                    entry_fee,
                });
            } else {
                report.rejected_entries += 1;
            }
        }
        if let Some(p) = &position {
            let long = p.signal.side.is_buy();
            let stop = p.signal.stop_price;
            let target = p.signal.take_profit;
            let stop_hit = if long {
                bar.low <= stop
            } else {
                bar.high >= stop
            };
            let target_hit = if long {
                bar.high >= target
            } else {
                bar.low <= target
            };
            let exit = if stop_hit {
                Some((
                    if long {
                        bar.open.min(stop)
                    } else {
                        bar.open.max(stop)
                    },
                    "stop",
                ))
            } else if target_hit {
                Some((target, "target"))
            } else if index + 1 == candles.len() {
                Some((bar.close, "end_of_data"))
            } else {
                None
            };
            if let Some((price, reason)) = exit {
                let p = position.take().unwrap();
                let price = adverse(price, !long, o.slippage_bps);
                let gross = (price - p.entry) * p.quantity * if long { 1.0 } else { -1.0 };
                let fee = price * p.quantity * o.fee_bps / 10_000.0;
                cash += gross - fee;
                let fees = fee + p.entry_fee;
                ensure!(
                    cash.is_finite() && gross.is_finite() && fees.is_finite(),
                    "accounting overflow"
                );
                report.total_fees += fees;
                ensure!(report.total_fees.is_finite(), "cumulative fee overflow");
                report.trades.push(SimulatedTrade {
                    strategy: p.signal.strategy,
                    side: p.signal.side,
                    signal_time_ms: p.signal.created_at.timestamp_millis(),
                    entry_time_ms: p.entry_ms,
                    exit_time_ms: bar.ts.timestamp_millis(),
                    quantity: p.quantity,
                    entry_price: p.entry,
                    exit_price: price,
                    gross_pnl: gross,
                    fees,
                    net_pnl: gross - fees,
                    exit_reason: reason.into(),
                });
                ensure!(
                    report.trades.len() <= o.max_trades,
                    "trade resource budget exceeded"
                );
            }
        }
        let equity = cash
            + position
                .as_ref()
                .map(|p| {
                    (bar.close - p.entry)
                        * p.quantity
                        * if p.signal.side.is_buy() { 1.0 } else { -1.0 }
                })
                .unwrap_or(0.0);
        ensure!(equity.is_finite(), "equity overflow");
        peak = peak.max(equity);
        report.max_drawdown_fraction = report.max_drawdown_fraction.max((peak - equity) / peak);
        report.equity.push(EquityPoint {
            time_ms: bar.ts.timestamp_millis(),
            equity,
        });
        if position.is_none() && cash > 0.0 && index + 1 < candles.len() {
            let start = (index + 1).saturating_sub(lookback);
            let history = &candles[start..=index];
            let regime = classify_regime(history, &cfg.strategies.regime);
            if !cfg.strategies.global.regime_required || regime.allows_trading() {
                let snapshot = MarketSnapshot {
                    venue: "hyperliquid".into(),
                    symbol: o.symbol.clone(),
                    mid: bar.close,
                    bid: None,
                    ask: None,
                    spread_bps: Some(0.0),
                    last_update: bar.ts,
                    candles: history.to_vec(),
                };
                for strategy in strategies {
                    if let Some(mut signal) = strategy.evaluate(&snapshot, regime, cfg) {
                        ensure!(
                            signal.symbol == o.symbol
                                && signal.venue == "hyperliquid"
                                && signal.stop_price.is_finite()
                                && signal.stop_price > 0.0
                                && signal.take_profit.is_finite()
                                && signal.take_profit > 0.0,
                            "invalid strategy signal"
                        );
                        signal.created_at = bar.ts;
                        pending = Some(signal);
                        break;
                    }
                }
            }
        }
    }
    report.final_equity = cash;
    Ok(report)
}
fn adverse(price: f64, buy: bool, bps: f64) -> f64 {
    price
        * if buy {
            1.0 + bps / 10_000.0
        } else {
            1.0 - bps / 10_000.0
        }
}

/// Evaluates `FangUnifiedPlanner` deterministically on a single-instrument candle slice,
/// bridging legacy `oracle::replay` consumers onto the shared FANG strategy architecture.
pub fn plan_unified_oracle_candidates(
    candles: &[Candle],
    symbol: &str,
    equity_usd: Decimal,
    cfg: &UnifiedStrategyConfig,
    budget: &PlanningBudget,
) -> Result<crate::fang_engine::UnifiedPlanOutput> {
    ensure!(!candles.is_empty(), "candles required");
    let as_of_ms = candles.last().unwrap().ts.timestamp_millis();
    let reg = infer_regime(
        candles,
        &RegimeInferenceConfig {
            lookback_bars: cfg.slow_ema.max(12),
            ..Default::default()
        },
    )
    .map(|a| a.regime)
    .unwrap_or(crate::cognition_shadow::InferredRegime::InsufficientHistory);
    let mark = Decimal::try_from(candles.last().unwrap().close)
        .unwrap_or(Decimal::ONE)
        .round_dp(6);
    let input = FangMarketInput {
        as_of_ms,
        correlation_id: format!("oracle-unified-{as_of_ms}"),
        watchlist: vec![symbol.to_string()],
        candles: BTreeMap::from([(symbol.to_string(), candles.to_vec())]),
        marks: BTreeMap::from([(symbol.to_string(), mark)]),
        regimes: BTreeMap::from([(symbol.to_string(), reg)]),
        microstructure: BTreeMap::new(),
        portfolio_equity_usd: equity_usd,
    };
    let _ = BTreeSet::<String>::new();
    FangUnifiedPlanner::evaluate_and_plan(&input, budget, cfg, None)
}

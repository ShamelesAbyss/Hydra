//! Extended Qualification, Fault-Injection Battery, Scientific Strategy Acceptance,
//! and Priority-Isolated Resource Budgeting (`v1.15.0 | PROJECT SOVEREIGNTY`).
//!
//! Safety & truthfulness invariants:
//! - Never fabricates elapsed duration (`actual_elapsed_seconds` comes strictly from `Instant::elapsed()`).
//! - Rejects weak/cost-negative/insufficient-sample strategies (`RejectedKeepDisabled`).
//! - Ensures background research (ORACLE/GENESIS) never starves `ProtectiveCritical` (AEGIS/NEXUS) work.

use crate::cognition_shadow::OutcomeSource;
use crate::cognition_stats::bayesian_strategy_expectancy;
use crate::core::{Candle, Side};
use crate::engine_ipc::{
    AuthenticatedEngineRequest, EngineControlCommand, EngineIpcClient, EngineIpcServer,
};
use crate::execution_recovery::{
    reconcile_execution_state, ExecutionJournalRecord, OrderLifecycleState, RecoveryAction,
};
use crate::hunter::MarketObservation;
use crate::oracle_research::{BootstrapInterval, PortfolioResearchReport, PurgedWalkForwardReport};
use crate::oracle_validation::audit_candle_dataset;
use crate::paper_pipeline::{SynapsePipeline, SynapsePipelineConfig};
use crate::paper_service::PaperSessionLock;
use crate::paper_store::{PaperSessionKind, PaperStore};
use crate::synapse_bus::DataProvenance;
use crate::watchdog::{WatchdogPolicy, WatchdogSupervisor};
use anyhow::{ensure, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenanceLabel {
    SyntheticFixtureSnapshot,
    OfflineIntegratedMock,
    PublicMarketPaperSession,
    HistoricalOutOfSampleReplay,
    HyperliquidTestnetSandbox,
    ProductionMainnetPilot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualificationCampaignReport {
    pub schema_version: u32,
    pub software_version: String,
    pub evidence_label: EvidenceProvenanceLabel,
    pub strategy_config_sha3: String,
    pub risk_policy_sha3: String,
    pub dataset_provenance: String,
    pub actual_elapsed_seconds: f64,
    pub cycles_completed: u64,
    pub market_availability_ratio: f64,
    pub stream_reconnects: u64,
    pub recovery_incidents: u64,
    pub signals_evaluated: usize,
    pub plans_accepted: usize,
    pub plans_rejected: usize,
    pub simulated_executions: usize,
    pub total_fees_usd: String,
    pub total_funding_usd: String,
    pub open_positions: usize,
    pub equity_usd: String,
    pub max_drawdown_ratio: String,
    pub risk_violations: usize,
    pub unknown_or_incomplete_measurements: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultInjectionKind {
    NetworkLoss,
    ExchangeUnavailable,
    StaleAndMalformedData,
    UnexpectedProcessTermination,
    DatabaseLocking,
    DiskExhaustion,
    InterruptedPersistence,
    IpcAuthAndFrameFailure,
    ResourceContention,
    RestartContinuity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultInjectionOutcome {
    pub scenario: FaultInjectionKind,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultInjectionReport {
    pub total_scenarios: usize,
    pub passed_count: usize,
    pub all_passed: bool,
    pub outcomes: Vec<FaultInjectionOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadPriority {
    ProtectiveCritical,
    ResearchBackground,
}

/// Ensures critical risk & protective workloads are never starved by bounded research pools.
#[derive(Debug, Clone)]
pub struct ResourceBudgetScheduler {
    pub total_worker_slots: usize,
    pub reserved_protective_slots: usize,
    active_protective: usize,
    active_research: usize,
}

impl ResourceBudgetScheduler {
    pub fn new(total_worker_slots: usize, reserved_protective_slots: usize) -> Result<Self> {
        ensure!(
            total_worker_slots >= 2
                && reserved_protective_slots >= 1
                && reserved_protective_slots < total_worker_slots,
            "scheduler must reserve >=1 slot exclusively for protective workloads"
        );
        Ok(Self {
            total_worker_slots,
            reserved_protective_slots,
            active_protective: 0,
            active_research: 0,
        })
    }

    pub fn try_acquire(&mut self, prio: WorkloadPriority) -> bool {
        let used = self.active_protective + self.active_research;
        match prio {
            WorkloadPriority::ProtectiveCritical => {
                if used < self.total_worker_slots {
                    self.active_protective += 1;
                    true
                } else {
                    false
                }
            }
            WorkloadPriority::ResearchBackground => {
                let max_research = self.total_worker_slots - self.reserved_protective_slots;
                if self.active_research < max_research && used < self.total_worker_slots {
                    self.active_research += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn release(&mut self, prio: WorkloadPriority) {
        match prio {
            WorkloadPriority::ProtectiveCritical => {
                self.active_protective = self.active_protective.saturating_sub(1);
            }
            WorkloadPriority::ResearchBackground => {
                self.active_research = self.active_research.saturating_sub(1);
            }
        }
    }

    pub fn available_protective_slots(&self) -> usize {
        self.total_worker_slots
            .saturating_sub(self.active_protective + self.active_research)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScientificAcceptanceCriteria {
    pub min_trades: usize,
    pub min_net_return_pct: f64,
    pub min_excess_over_buy_hold_pct: f64,
    pub max_drawdown_pct: f64,
    pub min_walk_forward_folds: usize,
    pub min_mean_oos_return_pct: f64,
    pub min_bootstrap_p05_return_pct: f64,
}

impl Default for ScientificAcceptanceCriteria {
    fn default() -> Self {
        Self {
            min_trades: 5,
            min_net_return_pct: 0.5,
            min_excess_over_buy_hold_pct: 0.0,
            max_drawdown_pct: 20.0,
            min_walk_forward_folds: 2,
            min_mean_oos_return_pct: 0.0,
            min_bootstrap_p05_return_pct: -2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum StrategyAcceptanceVerdict {
    EligibleForMonitoredPilot { summary: String },
    RejectedKeepDisabled { reasons: Vec<String> },
}

pub fn evaluate_scientific_strategy_acceptance(
    portfolio: &PortfolioResearchReport,
    walk_forward: &PurgedWalkForwardReport,
    bootstrap: &BootstrapInterval,
    criteria: &ScientificAcceptanceCriteria,
) -> StrategyAcceptanceVerdict {
    let mut reasons = Vec::new();
    let metrics_finite = portfolio.metrics.net_return_pct.is_finite()
        && portfolio.excess_over_buy_and_hold_pct.is_finite()
        && portfolio.metrics.max_drawdown_pct.is_finite()
        && walk_forward.mean_oos_return_pct.is_finite()
        && walk_forward.worst_oos_drawdown_pct.is_finite()
        && bootstrap.p05_mean_return_pct.is_finite();
    if !metrics_finite {
        reasons.push("Non-finite NaN/Inf metric observed in strategy acceptance report".into());
    }
    if portfolio.metrics.trades_count < criteria.min_trades {
        reasons.push(format!(
            "Insufficient simulated trades ({} < min {})",
            portfolio.metrics.trades_count, criteria.min_trades
        ));
    }
    if portfolio.metrics.net_return_pct < criteria.min_net_return_pct {
        reasons.push(format!(
            "After-cost net return {:.2}% below {:.2}% threshold",
            portfolio.metrics.net_return_pct, criteria.min_net_return_pct
        ));
    }
    if portfolio.excess_over_buy_and_hold_pct < criteria.min_excess_over_buy_hold_pct {
        reasons.push(format!(
            "Failed to outperform equal-weight buy-and-hold baseline ({:+.2}% excess)",
            portfolio.excess_over_buy_and_hold_pct
        ));
    }
    if portfolio.metrics.max_drawdown_pct > criteria.max_drawdown_pct {
        reasons.push(format!(
            "Max drawdown {:.2}% exceeds {:.2}% cap",
            portfolio.metrics.max_drawdown_pct, criteria.max_drawdown_pct
        ));
    }
    if walk_forward.worst_oos_drawdown_pct > criteria.max_drawdown_pct {
        reasons.push(format!(
            "Worst OOS fold drawdown {:.2}% exceeds {:.2}% cap",
            walk_forward.worst_oos_drawdown_pct, criteria.max_drawdown_pct
        ));
    }
    if walk_forward.folds.len() < criteria.min_walk_forward_folds {
        reasons.push(format!(
            "Walk-forward fold count {} < minimum {}",
            walk_forward.folds.len(),
            criteria.min_walk_forward_folds
        ));
    }
    if walk_forward.folds.iter().any(|f| f.test_trades == 0) {
        reasons.push("Walk-forward validation contains zero-trade OOS fold".into());
    }
    if walk_forward.mean_oos_return_pct < criteria.min_mean_oos_return_pct {
        reasons.push(format!(
            "Mean OOS return {:.2}% below {:.2}% floor",
            walk_forward.mean_oos_return_pct, criteria.min_mean_oos_return_pct
        ));
    }
    if bootstrap.p05_mean_return_pct < criteria.min_bootstrap_p05_return_pct {
        reasons.push(format!(
            "Bootstrap 5th percentile return {:.2}% breaches conservative downside bound {:.2}%",
            bootstrap.p05_mean_return_pct, criteria.min_bootstrap_p05_return_pct
        ));
    }

    if reasons.is_empty() {
        StrategyAcceptanceVerdict::EligibleForMonitoredPilot {
            summary: format!(
                "Passed all scientific gates: {} trades, net={:+.2}%, excess_vs_BH={:+.2}%, worst_OOS_dd={:.2}%, bootstrap_p05={:+.2}%",
                portfolio.metrics.trades_count,
                portfolio.metrics.net_return_pct,
                portfolio.excess_over_buy_and_hold_pct,
                walk_forward.worst_oos_drawdown_pct,
                bootstrap.p05_mean_return_pct
            ),
        }
    } else {
        StrategyAcceptanceVerdict::RejectedKeepDisabled { reasons }
    }
}

/// Genuine in-process historical OOS strategy acceptance producer: evaluates portfolio replay,
/// purged embargoed walk-forward CV, and block bootstrap CI, then records the authenticated
/// result (honestly `passed=false` when rejected) into `TrustStore`.
pub fn evaluate_and_record_historical_oos(
    ds: &crate::oracle_research::MultiMarketDataset,
    fang_cfg: &crate::fang_engine::UnifiedStrategyConfig,
    folds: usize,
    embargo_bars: usize,
    trust_store: Option<&crate::trusted_evidence::TrustStore>,
) -> Result<(StrategyAcceptanceVerdict, serde_json::Value)> {
    let (ts, dataset_sha3) = ds.validate_and_fingerprint()?;
    let config_sha3 = fang_cfg.config_sha3()?;
    let exec = crate::oracle_research::ExecutionModelConfig::default();
    let portfolio = crate::oracle_research::simulate_portfolio(ds, fang_cfg, &exec)?;
    let fold_count = folds.max(2);
    let min_test = exec.warmup_bars + exec.latency_bars + 2;
    let test_bars = (ts.len() / (fold_count + 2)).max(min_test);
    let train_bars = ts
        .len()
        .saturating_sub(fold_count * test_bars + embargo_bars)
        .max(exec.warmup_bars + 2);
    let wf = crate::oracle_research::walk_forward_purged_cv(
        ds,
        train_bars,
        embargo_bars,
        test_bars,
        test_bars,
        fang_cfg,
        &exec,
    )?;
    let mut oos_bar_returns: Vec<f64> = Vec::new();
    for f in &wf.folds {
        let fold_ds = ds.slice_range(f.test_range.0, f.test_range.1)?;
        let (_, fold_bars) =
            crate::oracle_research::simulate_portfolio_with_bar_returns(&fold_ds, fang_cfg, &exec)?;
        oos_bar_returns.extend(fold_bars);
    }
    let boot = if oos_bar_returns.len() < 10 {
        BootstrapInterval {
            samples: oos_bar_returns.len(),
            block_len: 1,
            resamples: 100,
            p05_mean_return_pct: -100.0,
            median_mean_return_pct: 0.0,
            p95_mean_return_pct: 0.0,
        }
    } else {
        let block_len = 6.min(oos_bar_returns.len() / 2).max(1);
        crate::oracle_research::block_bootstrap_ci(&oos_bar_returns, block_len, 200, 7)?
    };
    let verdict = evaluate_scientific_strategy_acceptance(
        &portfolio,
        &wf,
        &boot,
        &ScientificAcceptanceCriteria::default(),
    );
    let eligible = matches!(
        verdict,
        StrategyAcceptanceVerdict::EligibleForMonitoredPilot { .. }
    );
    let mut payload = serde_json::json!({
        "verdict": verdict,
        "config_sha3": config_sha3,
        "dataset_sha3_256": dataset_sha3,
        "registry_version": crate::fang_engine::REGISTRY_VERSION,
        "portfolio": portfolio,
        "walk_forward": wf,
        "bootstrap_ci": boot,
    });
    if let Some(tstore) = trust_store {
        if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
            let summary = format!(
                "Historical OOS strategy-accept verdict={} trades={} net={:+.2}%",
                if eligible {
                    "eligible_for_monitored_pilot"
                } else {
                    "rejected_keep_disabled"
                },
                portfolio.metrics.trades_count,
                portfolio.metrics.net_return_pct
            );
            if let Ok(rec) = tstore.record(
                crate::trusted_evidence::TrustedEvidenceKind::HistoricalOosQualification,
                &bytes,
                &config_sha3,
                &dataset_sha3,
                eligible,
                &summary,
                Utc::now().timestamp_millis(),
            ) {
                payload["trusted_evidence_record_id"] = serde_json::json!(rec.record_id);
                payload["trusted_evidence_passed"] = serde_json::json!(rec.passed);
            }
        }
    }
    Ok((verdict, payload))
}

fn sha3_hex(bytes: &[u8]) -> String {
    let mut h = Sha3_256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Runs a deterministic 10-scenario Section 21 fault-injection battery exercising real HYDRA modules.
pub fn run_fault_injection_campaign(work_root: &Path) -> Result<FaultInjectionReport> {
    std::fs::create_dir_all(work_root)?;
    let mut outcomes = Vec::new();
    let base_ts = 1_700_100_000_000_i64;

    // 1. NetworkLoss -> missing mark causes incomplete equity flag & refuses unsafe sizing
    {
        let mut pipe = SynapsePipeline::new(SynapsePipelineConfig::default())?;
        let bars: Vec<Candle> = (0..18)
            .map(|i| {
                let px = 60_000.0 + i as f64 * 100.0;
                Candle {
                    ts: chrono::DateTime::from_timestamp_millis(base_ts + i * 60_000).unwrap(),
                    open: px,
                    high: px * 1.001,
                    low: px * 0.999,
                    close: px,
                    volume: 1000.0,
                }
            })
            .collect();
        let now_ms = bars.last().unwrap().ts.timestamp_millis();
        let obs = MarketObservation {
            symbol: "BTC".into(),
            observed_at_ms: now_ms,
            volume_24h_usd: Some(50_000_000.0),
            depth_usd: Some(500_000.0),
            spread_bps: Some(2.0),
            volatility_pct: Some(2.0),
            directionality: Some(0.8),
            funding_bps: Some(1.0),
            open_interest_usd: Some(10_000_000.0),
            coverage_fraction: Some(1.0),
            age_days: Some(365.0),
            observed_history_minutes: None,
            ema_ready: true,
        };
        let marks = BTreeMap::from([("BTC".into(), Decimal::from(61_700))]);
        let candles = BTreeMap::from([("BTC".into(), bars.clone())]);
        pipe.step_cycle(now_ms, &[obs], &candles, &marks)?;
        // Next cycle simulates network loss: empty marks while BTC position open
        let s2 = pipe.step_cycle(now_ms + 60_000, &[], &candles, &BTreeMap::new())?;
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::NetworkLoss,
            passed: !s2.equity_complete && s2.unavailable_mark_symbols.contains(&"BTC".to_string()),
            detail: format!(
                "equity_complete={} missing={:?}",
                s2.equity_complete, s2.unavailable_mark_symbols
            ),
        });
    }

    // 2. ExchangeUnavailable -> ambiguous order holds and refuses blind retry
    {
        let rec = reconcile_execution_state(
            &[ExecutionJournalRecord {
                cloid: "ambig-fi".into(),
                exchange_oid: None,
                symbol: "ETH".into(),
                side: Side::Long,
                owner_strategy: "mom".into(),
                requested_qty: Decimal::ONE,
                limit_price: None,
                filled_qty: Decimal::ZERO,
                state: OrderLifecycleState::Submitted,
                submitted_at_ms: 100,
            }],
            &[],
            &[],
            &[],
        )?;
        let refused = rec
            .actions
            .iter()
            .any(|a| matches!(a, RecoveryAction::RefuseBlindRetry { .. }));
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::ExchangeUnavailable,
            passed: refused && rec.unresolved_holds == 1,
            detail: format!(
                "unresolved_holds={} refused_blind_retry={refused}",
                rec.unresolved_holds
            ),
        });
    }

    // 3. StaleAndMalformedData -> inverted candle fails dataset audit
    {
        let bad_candle = vec![Candle {
            ts: chrono::DateTime::from_timestamp_millis(base_ts).unwrap(),
            open: 100.0,
            high: 90.0, // invalid < open!
            low: 105.0,
            close: 100.0,
            volume: 10.0,
        }];
        let audit_res = audit_candle_dataset(&bad_candle, 60_000);
        let ok = match audit_res {
            Err(_) => true,
            Ok(rep) => !rep.integrity_ok,
        };
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::StaleAndMalformedData,
            passed: ok,
            detail: "Malformed OHLC bar deterministically rejected".into(),
        });
    }

    // 4. UnexpectedProcessTermination -> real child failure triggers watchdog restart cap + escalation
    {
        let wdir = work_root.join("wd-proc");
        let pol = WatchdogPolicy {
            max_restarts_in_window: 1,
            ..Default::default()
        };
        let mut sup = WatchdogSupervisor::acquire(&wdir, pol)?;
        let shell = if cfg!(windows) {
            Path::new("cmd")
        } else {
            Path::new("sh")
        };
        let args: &[&str] = if cfg!(windows) {
            &["/C", "exit 1"]
        } else {
            &["-c", "exit 1"]
        };
        let sum = sup.supervise_bounded_process(shell, args, 3)?;
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::UnexpectedProcessTermination,
            passed: sum.escalated && sum.restarts == 1,
            detail: sum.detail,
        });
    }

    // 5. DatabaseLocking -> concurrent PaperSessionLock is blocked
    {
        let ldir = work_root.join("locks");
        let _hold = PaperSessionLock::acquire(&ldir, "fi-sess")?;
        let blocked = PaperSessionLock::assert_unowned(&ldir, "fi-sess").is_err();
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::DatabaseLocking,
            passed: blocked,
            detail: format!("concurrent_lock_blocked={blocked}"),
        });
    }

    // 6. DiskExhaustion -> excessive free-space requirement flagged
    {
        let drep = WatchdogSupervisor::check_disk_space(work_root, u64::MAX / 2);
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::DiskExhaustion,
            passed: !drep.sufficient,
            detail: format!("disk_sufficient={}", drep.sufficient),
        });
    }

    // 7. InterruptedPersistence -> corrupt DB caught by PRAGMA quick_check
    {
        let corrupt = work_root.join("corrupt-fi.db");
        std::fs::write(&corrupt, b"corrupted-wal-frame-bytes")?;
        let rep = WatchdogSupervisor::check_database_integrity(&corrupt, 10_000_000);
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::InterruptedPersistence,
            passed: !rep.quick_check_ok,
            detail: format!("quick_check_ok={}", rep.quick_check_ok),
        });
    }

    // 8. IpcAuthAndFrameFailure -> bad token & replay rejected by EngineIpcServer
    {
        let ipc_dir = work_root.join("ipc");
        let mut srv = EngineIpcServer::start(&ipc_dir, "fi-eng")?;
        let bad = EngineIpcClient::send_raw(
            srv.descriptor.port,
            &AuthenticatedEngineRequest {
                token_hex: "badtoken".into(),
                nonce: 1,
                issued_at_ms: Utc::now().timestamp_millis(),
                command: EngineControlCommand::Heartbeat,
            },
        )?;
        let ok1 =
            EngineIpcClient::send_command(&ipc_dir, "fi-eng", 1, EngineControlCommand::Heartbeat)?;
        let rep =
            EngineIpcClient::send_command(&ipc_dir, "fi-eng", 1, EngineControlCommand::GetState)?;
        srv.stop();
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::IpcAuthAndFrameFailure,
            passed: !bad.ok && ok1.ok && !rep.ok,
            detail: "Bad token and replayed nonce refused".into(),
        });
    }

    // 9. ResourceContention -> ProtectiveCritical never starved when ResearchBackground saturates its cap
    {
        let mut sched = ResourceBudgetScheduler::new(4, 1)?;
        assert!(sched.try_acquire(WorkloadPriority::ResearchBackground));
        assert!(sched.try_acquire(WorkloadPriority::ResearchBackground));
        assert!(sched.try_acquire(WorkloadPriority::ResearchBackground));
        let research_capped = !sched.try_acquire(WorkloadPriority::ResearchBackground);
        let protective_admitted = sched.try_acquire(WorkloadPriority::ProtectiveCritical);
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::ResourceContention,
            passed: research_capped && protective_admitted,
            detail: format!(
                "research_capped={research_capped} protective_admitted={protective_admitted}"
            ),
        });
    }

    // 10. RestartContinuity -> PaperStore reload restores exact SynapseBus next_event_id & samples
    {
        let sdir = work_root.join("restart");
        std::fs::create_dir_all(&sdir)?;
        let dbp = sdir.join("paper.db");
        let store = PaperStore::open(&dbp)?;
        let sess = store.create_session(
            PaperSessionKind::OfflineDemo,
            "fi-restart",
            SynapsePipelineConfig::default(),
        )?;
        let mut pipe = store.load_pipeline(&sess.session_id)?;
        let cyc = pipe.run_deterministic_demo_step()?;
        let evs: Vec<_> = pipe.bus.all_chronological().cloned().collect();
        let marks = BTreeMap::from([("BTC".into(), Decimal::from(60_000))]);
        store.commit_cycle(
            &sess.session_id,
            &pipe,
            &cyc,
            0,
            &evs,
            &marks,
            DataProvenance::SyntheticTestFixture,
            None,
        )?;
        let before_id = pipe.bus.next_event_id();
        drop(store);

        let store2 = PaperStore::open(&dbp)?;
        let reloaded = store2.load_pipeline(&sess.session_id)?;
        let preserved = reloaded.bus.next_event_id() == before_id && reloaded.cycle_count == 1;
        outcomes.push(FaultInjectionOutcome {
            scenario: FaultInjectionKind::RestartContinuity,
            passed: preserved,
            detail: format!(
                "reloaded_cycle={} next_event_id={before_id}",
                reloaded.cycle_count
            ),
        });
    }

    let passed_count = outcomes.iter().filter(|o| o.passed).count();
    let total_scenarios = outcomes.len();
    Ok(FaultInjectionReport {
        total_scenarios,
        passed_count,
        all_passed: passed_count == total_scenarios,
        outcomes,
    })
}

/// Runs an honest local qualification campaign snapshot recording real elapsed seconds & metrics.
pub fn run_local_qualification_snapshot(cycles: usize) -> Result<QualificationCampaignReport> {
    ensure!(
        (1..=50).contains(&cycles),
        "local snapshot cycles must be 1..=50"
    );
    let started = Instant::now();
    let cfg = SynapsePipelineConfig::default();
    let cfg_bytes = serde_json::to_vec(&cfg)?;
    let mut pipe = SynapsePipeline::new(cfg)?;
    let mut t_ms = 1_700_200_000_000_i64;
    let mut total_events = 0usize;
    let mut last_marks: BTreeMap<String, Decimal> = BTreeMap::new();
    for _ in 0..cycles {
        let sum = pipe.run_deterministic_demo_step()?;
        total_events += sum.events_emitted;
        last_marks.insert("BTC".into(), Decimal::from(60_000));
        t_ms += 60_000;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let _ = bayesian_strategy_expectancy(
        "adaptive_ema_anchor",
        "TrendingUp",
        OutcomeSource::Simulated,
        t_ms,
        &pipe.simulated_samples,
        &crate::cognition_shadow::ShadowPolicy::default(),
        200.0,
        14.0,
    );

    Ok(QualificationCampaignReport {
        schema_version: 2,
        software_version: env!("CARGO_PKG_VERSION").to_string(),
        evidence_label: EvidenceProvenanceLabel::SyntheticFixtureSnapshot,
        strategy_config_sha3: sha3_hex(&cfg_bytes),
        risk_policy_sha3: sha3_hex(b"aegis-sovereign-v1"),
        dataset_provenance: format!("{:?}", DataProvenance::SyntheticTestFixture),
        actual_elapsed_seconds: elapsed,
        cycles_completed: pipe.cycle_count,
        market_availability_ratio: 1.0,
        stream_reconnects: 0,
        recovery_incidents: 0,
        signals_evaluated: total_events,
        plans_accepted: pipe.ledger.open_positions.len() + pipe.ledger.closed_trades.len(),
        plans_rejected: 0,
        simulated_executions: pipe.ledger.open_positions.len() + pipe.ledger.closed_trades.len(),
        total_fees_usd: pipe.ledger.total_fees_usd.to_string(),
        total_funding_usd: pipe.ledger.total_funding_usd.to_string(),
        open_positions: pipe.ledger.open_positions.len(),
        equity_usd: pipe.ledger.cash_equity.to_string(),
        max_drawdown_ratio: pipe.ledger.max_drawdown_ratio.to_string(),
        risk_violations: usize::from(pipe.ledger.breaker_reason.is_some()),
        unknown_or_incomplete_measurements: vec![
            "SyntheticTestFixture snapshot only; Accelerated Awakening monitored-pilot qualification requires separate historical OOS replay, public-market paper lifecycle, and Hyperliquid Testnet evidence".into(),
        ],
        note: "Honest measured runtime of SyntheticTestFixture only; does not substitute for historical OOS or public-market paper qualification".into(),
    })
}

// Re-use BTreeSet transparently if callers need unique tags
#[allow(dead_code)]
fn _keep_set(_s: &BTreeSet<String>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle_research::{ExecutionModelConfig, MultiMarketDataset};

    #[test]
    fn fault_injection_battery_passes_all_ten_scenarios_and_scientific_gate_enforces_standards() {
        let dir = tempfile::tempdir().unwrap();
        let rep = run_fault_injection_campaign(dir.path()).unwrap();
        assert_eq!(rep.total_scenarios, 10);
        assert!(
            rep.all_passed,
            "failed scenarios: {:?}",
            rep.outcomes
                .iter()
                .filter(|o| !o.passed)
                .collect::<Vec<_>>()
        );

        let snap = run_local_qualification_snapshot(3).unwrap();
        assert_eq!(snap.cycles_completed, 3);
        assert!(snap.actual_elapsed_seconds >= 0.0 && snap.actual_elapsed_seconds < 60.0);

        // Scientific gate rejects failing/under-traded report and keeps disabled
        let bars: Vec<Candle> = (0..45)
            .map(|i| {
                let px = 100.0 + i as f64;
                Candle {
                    ts: chrono::DateTime::from_timestamp_millis(1_700_000_000_000 + i * 3_600_000)
                        .unwrap(),
                    open: px,
                    high: px * 1.01,
                    low: px * 0.99,
                    close: px,
                    volume: 10_000.0,
                }
            })
            .collect();
        let ds = MultiMarketDataset {
            bars_by_symbol: BTreeMap::from([("BTC".into(), bars)]),
            specs: BTreeMap::new(),
            funding_bps_1h: BTreeMap::new(),
            outage_windows: vec![],
        };
        let p_rep = crate::oracle_research::simulate_portfolio(
            &ds,
            &Default::default(),
            &ExecutionModelConfig {
                warmup_bars: 12,
                latency_bars: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let wf = crate::oracle_research::walk_forward_purged_cv(
            &ds,
            20,
            2,
            15,
            15,
            &Default::default(),
            &ExecutionModelConfig {
                warmup_bars: 12,
                latency_bars: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let boot = BootstrapInterval {
            samples: 10,
            block_len: 3,
            resamples: 50,
            p05_mean_return_pct: -5.0,
            median_mean_return_pct: 0.1,
            p95_mean_return_pct: 2.0,
        };
        let verdict = evaluate_scientific_strategy_acceptance(
            &p_rep,
            &wf,
            &boot,
            &ScientificAcceptanceCriteria::default(),
        );
        assert!(matches!(
            verdict,
            StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
        ));

        // Passing baseline report then test NaN / Inf / worst_oos_drawdown_pct / zero-trade fold rejections.
        let mut good_p = p_rep.clone();
        good_p.metrics.trades_count = 12;
        good_p.metrics.net_return_pct = 5.0;
        good_p.excess_over_buy_and_hold_pct = 2.0;
        good_p.metrics.max_drawdown_pct = 4.0;
        let mut good_wf = wf.clone();
        good_wf.mean_oos_return_pct = 3.0;
        good_wf.worst_oos_drawdown_pct = 5.0;
        good_wf.folds = vec![
            crate::oracle_research::PurgedFoldResult {
                fold_index: 0,
                train_range: (0, 20),
                embargo_range: (20, 22),
                test_range: (22, 37),
                train_net_return_pct: 2.0,
                test_net_return_pct: 3.0,
                test_max_drawdown_pct: 4.0,
                test_trades: 3,
                excess_over_buy_hold_pct: 1.0,
            },
            crate::oracle_research::PurgedFoldResult {
                fold_index: 1,
                train_range: (10, 30),
                embargo_range: (30, 32),
                test_range: (32, 47),
                train_net_return_pct: 2.0,
                test_net_return_pct: 3.0,
                test_max_drawdown_pct: 5.0,
                test_trades: 4,
                excess_over_buy_hold_pct: 1.0,
            },
        ];
        let good_boot = BootstrapInterval {
            samples: 10,
            block_len: 3,
            resamples: 50,
            p05_mean_return_pct: 0.5,
            median_mean_return_pct: 2.0,
            p95_mean_return_pct: 4.0,
        };
        let criteria = ScientificAcceptanceCriteria::default();
        assert!(matches!(
            evaluate_scientific_strategy_acceptance(&good_p, &good_wf, &good_boot, &criteria),
            StrategyAcceptanceVerdict::EligibleForMonitoredPilot { .. }
        ));

        let mut nan_p = good_p.clone();
        nan_p.metrics.net_return_pct = f64::NAN;
        assert!(matches!(
            evaluate_scientific_strategy_acceptance(&nan_p, &good_wf, &good_boot, &criteria),
            StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
        ));

        let mut inf_boot = good_boot.clone();
        inf_boot.p05_mean_return_pct = f64::INFINITY;
        assert!(matches!(
            evaluate_scientific_strategy_acceptance(&good_p, &good_wf, &inf_boot, &criteria),
            StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
        ));

        let mut dd_wf = good_wf.clone();
        dd_wf.worst_oos_drawdown_pct = 25.0;
        assert!(matches!(
            evaluate_scientific_strategy_acceptance(&good_p, &dd_wf, &good_boot, &criteria),
            StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
        ));

        let mut zero_wf = good_wf.clone();
        zero_wf.folds[1].test_trades = 0;
        assert!(matches!(
            evaluate_scientific_strategy_acceptance(&good_p, &zero_wf, &good_boot, &criteria),
            StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
        ));
    }
}

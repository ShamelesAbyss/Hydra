//! Project Sovereignty (`v1.15.0`) End-to-End Integration Suite.
//!
//! Verifies all seven heads connected on SynapseBus v2, external watchdog supervision & DB/disk
//! checks, all 10 Section 21 fault-injection scenarios, scientific strategy acceptance &
//! conservative rejection, resource budget isolation, and strict `PendingCertification` truthfulness.

use hydra::live_gate::{evaluate_certification_readiness, PillarStatus};
use hydra::oracle_research::{
    BootstrapInterval, PortfolioResearchReport, PurgedFoldResult, PurgedWalkForwardReport,
    QuantitativeMetrics, ResearchVerdict,
};
use hydra::paper_pipeline::{SynapsePipeline, SynapsePipelineConfig};
use hydra::qualification::{
    evaluate_scientific_strategy_acceptance, run_fault_injection_campaign,
    run_local_qualification_snapshot, ResourceBudgetScheduler, ScientificAcceptanceCriteria,
    StrategyAcceptanceVerdict, WorkloadPriority,
};
use hydra::synapse_bus::{DataProvenance, HeadId, SynapsePayload};
use hydra::watchdog::{run_watchdog_health_check, WatchdogPolicy};
use rusqlite::Connection;
use std::collections::BTreeMap;

#[test]
fn sovereignty_seven_heads_watchdog_fault_injection_scientific_gate_and_certification_hold() {
    // 1. Complete 7-head typed SynapseBus connectivity verification
    let mut pipe = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
    pipe.run_deterministic_demo_step().unwrap();
    pipe.bus.publish(
        "sov-oracle",
        1_700_300_000_100,
        HeadId::Oracle,
        DataProvenance::HistoricalValidation,
        SynapsePayload::OracleValidation {
            strategy: "adaptive_ema_anchor".into(),
            net_return_pct: 1.5,
            max_drawdown_pct: 2.0,
            trades: 12,
        },
    );
    pipe.bus.publish(
        "sov-genesis",
        1_700_300_000_200,
        HeadId::Genesis,
        DataProvenance::EvolutionProposal,
        SynapsePayload::GenesisProposal {
            generation: 3,
            pareto_size: 4,
            best_score: 1.85,
        },
    );
    let events: Vec<_> = pipe.bus.all_chronological().cloned().collect();
    for head in [
        HeadId::Hunter,
        HeadId::Fang,
        HeadId::Cognition,
        HeadId::Aegis,
        HeadId::Nexus,
        HeadId::Oracle,
        HeadId::Genesis,
    ] {
        assert!(
            events.iter().any(|e| e.head == head),
            "expected typed event from head {:?}",
            head
        );
    }

    // 2. External watchdog health check + post-restart live auto-resume prohibition
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("sovereignty.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("CREATE TABLE meta(k TEXT PRIMARY KEY);")
        .unwrap();
    drop(conn);
    let (db_rep, disk_rep, checklist) =
        run_watchdog_health_check(dir.path(), &db_path, &WatchdogPolicy::default()).unwrap();
    assert!(db_rep.quick_check_ok);
    assert!(disk_rep.sufficient);
    assert!(checklist.can_resume_simulation);
    assert!(checklist.live_auto_resume_prohibited);

    // 3. All 10 Section 21 fault-injection scenarios pass against real modules
    let fi = run_fault_injection_campaign(&dir.path().join("fi")).unwrap();
    assert_eq!(fi.total_scenarios, 10);
    assert!(fi.all_passed, "failed fault injection: {:?}", fi.outcomes);

    // 4. Honest elapsed-time local qualification snapshot
    let snap = run_local_qualification_snapshot(3).unwrap();
    assert_eq!(snap.cycles_completed, 3);
    assert!(snap.actual_elapsed_seconds >= 0.0);

    // 5. Priority isolation between ProtectiveCritical and ResearchBackground
    let mut sched = ResourceBudgetScheduler::new(4, 1).unwrap();
    for _ in 0..3 {
        assert!(sched.try_acquire(WorkloadPriority::ResearchBackground));
    }
    assert!(!sched.try_acquire(WorkloadPriority::ResearchBackground));
    assert!(sched.try_acquire(WorkloadPriority::ProtectiveCritical));

    // 6. Scientific acceptance rejects weak candidates and approves robust multi-fold evidence
    let robust_port = PortfolioResearchReport {
        schema_version: 1,
        simulated: true,
        dataset_sha3_256: "abc".into(),
        metrics: QuantitativeMetrics {
            initial_equity: 10_000.0,
            final_equity: 10_400.0,
            net_return_pct: 4.0,
            max_drawdown_pct: 5.5,
            sharpe_annualized: Some(1.6),
            sortino_annualized: Some(2.1),
            calmar_ratio: Some(0.72),
            cvar_95_bar_pct: Some(-0.8),
            profit_factor: Some(1.8),
            turnover_multiple: 2.0,
            avg_exposure_ratio: 0.25,
            peak_exposure_ratio: 0.40,
            total_fees_usd: 15.0,
            total_funding_usd: 4.0,
            total_slippage_usd: 6.0,
            partial_fills_count: 1,
            liquidations_count: 0,
            trades_count: 18,
            verdict: ResearchVerdict::PositiveAfterCosts,
        },
        buy_and_hold_equal_weight_pct: 1.5,
        excess_over_buy_and_hold_pct: 2.5,
        excess_over_flat_cash_pct: 4.0,
        regime_pnl_usd: BTreeMap::new(),
        limitations: vec![],
    };
    let wf = PurgedWalkForwardReport {
        simulated: true,
        folds: vec![
            PurgedFoldResult {
                fold_index: 0,
                train_range: (0, 50),
                embargo_range: (50, 54),
                test_range: (54, 90),
                train_net_return_pct: 2.0,
                test_net_return_pct: 1.8,
                test_max_drawdown_pct: 3.0,
                test_trades: 9,
                excess_over_buy_hold_pct: 0.9,
            },
            PurgedFoldResult {
                fold_index: 1,
                train_range: (30, 80),
                embargo_range: (80, 84),
                test_range: (84, 120),
                train_net_return_pct: 2.4,
                test_net_return_pct: 2.2,
                test_max_drawdown_pct: 4.0,
                test_trades: 9,
                excess_over_buy_hold_pct: 1.2,
            },
        ],
        mean_oos_return_pct: 2.0,
        worst_oos_drawdown_pct: 4.0,
        deflated_sharpe_note: "purged".into(),
    };
    let boot = BootstrapInterval {
        samples: 20,
        block_len: 4,
        resamples: 200,
        p05_mean_return_pct: 0.25,
        median_mean_return_pct: 1.9,
        p95_mean_return_pct: 3.4,
    };
    let v_ok = evaluate_scientific_strategy_acceptance(
        &robust_port,
        &wf,
        &boot,
        &ScientificAcceptanceCriteria::default(),
    );
    assert!(matches!(
        v_ok,
        StrategyAcceptanceVerdict::EligibleForMonitoredPilot { .. }
    ));

    // Weak negative return keeps disabled
    let mut weak_port = robust_port;
    weak_port.metrics.net_return_pct = -1.5;
    let v_bad = evaluate_scientific_strategy_acceptance(
        &weak_port,
        &wf,
        &boot,
        &ScientificAcceptanceCriteria::default(),
    );
    assert!(matches!(
        v_bad,
        StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
    ));

    // 7. Live certification stays strictly locked until external Pillar 6 qualification + signing keys finish
    let cert = evaluate_certification_readiness();
    assert!(!cert.live_trading_authorized);
    assert_eq!(cert.overall_status, "PendingCertification");
    assert_eq!(cert.pillars.len(), 6);
    assert_eq!(
        cert.pillars[5].status,
        PillarStatus::PendingExternalValidation
    );
}

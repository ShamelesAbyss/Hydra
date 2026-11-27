//! Project Immortality (`v1.14.0`) End-to-End Integration Verification Suite.
//!
//! Verifies all non-live financial truth, recovery, protective stop, AEGIS sovereign risk,
//! Treasury credential isolation, authenticated Engine IPC, and fail-closed `PendingCertification`
//! invariants using offline fixtures and loopback sockets only.

use chrono::Utc;
use hydra::aegis_sovereign::{
    evaluate_sovereign_admission, SovereignAdmissionProposal, SovereignPortfolioState,
    SovereignPositionInput, SovereignRiskDecision, SovereignRiskPolicy,
};
use hydra::core::Side;
use hydra::engine_ipc::{
    AuthenticatedEngineRequest, EngineControlCommand, EngineIpcClient, EngineIpcServer,
};
use hydra::execution_recovery::{
    reconcile_execution_state, ExchangeOpenPositionSnapshot, ExchangeOrderSnapshot,
    ExecutionJournalRecord, OrderLifecycleState, RecoveryAction,
};
use hydra::ledger::{
    EvidenceSource, ExchangeFill, FillSide, FundingPayment, PnlConvention, TokenAmount,
};
use hydra::live_gate::{
    evaluate_certification_readiness, require_certified_live_execution, ExecutionChannel,
    LiveAuthorizationRequest, PillarStatus,
};
use hydra::protective_orders::{
    OpenProtectedPosition, ProtectiveOrderManager, ProtectiveOrderSpec, ProtectiveReconcileAction,
};
use hydra::settlement::{
    ExternalCashFlow, ExternalCashFlowKind, FundingCoverageWindow, SettlementEngine,
    SettlementStatus,
};
use hydra::treasury::{SigningAccessScope, TreasuryStore};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn immortality_end_to_end_settlement_recovery_protection_sovereign_risk_ipc_and_gate() {
    // 1. Authoritative settlement: partial fill + maker rebate + funding + deposit separation + exactly-once learning
    let mut se = SettlementEngine::new();
    se.record_funding_coverage(FundingCoverageWindow {
        symbol: "ETH".into(),
        start_ms: 1_000,
        end_ms: 20_000,
        complete: true,
    })
    .unwrap();
    se.ingest_external_cash_flow(ExternalCashFlow {
        cash_flow_id: "dep-eth".into(),
        kind: ExternalCashFlowKind::Deposit,
        token: "USDC".into(),
        signed_amount: Decimal::from(1_000),
        timestamp_ms: 1_500,
    })
    .unwrap();
    se.ingest_fill(
        ExchangeFill {
            fill_id: "in-1".into(),
            order_id: 10,
            client_order_id: Some("cl-in-1".into()),
            symbol: "ETH".into(),
            side: FillSide::Buy,
            quantity: Decimal::from(2),
            price: Decimal::from(2_000),
            closed_pnl_usdc: None,
            pnl_convention: PnlConvention::BeforeFees,
            fee: Some(TokenAmount {
                token: "USDC".into(),
                amount: Decimal::from(4),
            }),
            timestamp_ms: 2_000,
        },
        Some("adaptive_ema_anchor"),
    )
    .unwrap();
    se.ingest_funding(FundingPayment {
        funding_id: "f-eth-1".into(),
        symbol: "ETH".into(),
        payment: Some(TokenAmount {
            token: "USDC".into(),
            amount: Decimal::from(2),
        }),
        timestamp_ms: 3_000,
    })
    .unwrap();
    se.ingest_fill(
        ExchangeFill {
            fill_id: "out-1".into(),
            order_id: 11,
            client_order_id: Some("cl-out-1".into()),
            symbol: "ETH".into(),
            side: FillSide::Sell,
            quantity: Decimal::from(2),
            price: Decimal::from(2_100),
            closed_pnl_usdc: Some(Decimal::from(200)),
            pnl_convention: PnlConvention::BeforeFees,
            fee: Some(TokenAmount {
                token: "USDC".into(),
                amount: Decimal::from(-1), // maker rebate
            }),
            timestamp_ms: 4_000,
        },
        None,
    )
    .unwrap();

    // Gross 200 - 4 entry - (-1) exit + 2 funding = 199 verified net PnL
    // Start 5000 + 199 + 1000 deposit = 6199
    let eq = se
        .reconcile_account_equity(
            Decimal::from(5_000),
            Decimal::from(6_199),
            Decimal::ZERO,
            Decimal::new(1, 2),
        )
        .unwrap();
    assert!(eq.verified);
    assert_eq!(eq.verified_realized_net_pnl_usdc, Some(Decimal::from(199)));
    assert_eq!(
        se.round_trips()[0].status,
        SettlementStatus::VerifiedSettled
    );
    assert_eq!(se.emit_eligible_learning_outcomes().len(), 1);
    assert!(se.emit_eligible_learning_outcomes().is_empty());

    // 2. Execution recovery & protective reduce-only stop management while entries are blocked
    let journal = vec![
        ExecutionJournalRecord {
            cloid: "cl-open".into(),
            exchange_oid: Some(50),
            symbol: "BTC".into(),
            side: Side::Long,
            owner_strategy: "mom".into(),
            requested_qty: Decimal::from(2),
            limit_price: Some(Decimal::from(50_000)),
            filled_qty: Decimal::ZERO,
            state: OrderLifecycleState::Acknowledged,
            submitted_at_ms: 100,
        },
        ExecutionJournalRecord {
            cloid: "cl-ambig".into(),
            exchange_oid: None,
            symbol: "SOL".into(),
            side: Side::Long,
            owner_strategy: "mom".into(),
            requested_qty: Decimal::from(10),
            limit_price: None,
            filled_qty: Decimal::ZERO,
            state: OrderLifecycleState::Submitted,
            submitted_at_ms: 110,
        },
    ];
    let rest = vec![ExchangeOrderSnapshot {
        source: EvidenceSource::RestSnapshot,
        cloid: Some("cl-open".into()),
        exchange_oid: 50,
        symbol: "BTC".into(),
        side: Side::Long,
        state: OrderLifecycleState::PartiallyFilled,
        filled_qty: Decimal::ONE,
        avg_fill_px: Some(Decimal::from(50_000)),
        observed_at_ms: 150,
    }];
    let pos = vec![ExchangeOpenPositionSnapshot {
        symbol: "BTC".into(),
        side: Side::Long,
        qty: Decimal::ONE,
    }];
    let rec = reconcile_execution_state(&journal, &rest, &[], &pos).unwrap();
    assert_eq!(rec.unresolved_holds, 1);
    assert!(rec.actions.iter().any(|a| matches!(
        a,
        RecoveryAction::RefuseBlindRetry { cloid, .. } if cloid == "cl-ambig"
    )));

    let stops = ProtectiveOrderManager::reconcile_positions_and_stops(
        &[OpenProtectedPosition {
            symbol: "BTC".into(),
            owner_strategy: "mom".into(),
            side: Side::Long,
            qty: Decimal::ONE,
            entry_price: Decimal::from(50_000),
            stop_price: Decimal::from(49_000),
        }],
        &[ProtectiveOrderSpec {
            symbol: "BTC".into(),
            owner_strategy: "mom".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: Decimal::from(2), // mismatched quantity requires replacement
            trigger_price: Decimal::from(49_000),
            reduce_only: true,
            cloid: "old-stop".into(),
            exchange_oid: Some(99),
        }],
        &BTreeSet::new(),
        true, // entries blocked still manages stop cleanly!
    )
    .unwrap();
    assert!(stops.protection_management_permitted);
    assert!(stops.actions.iter().any(|a| matches!(
        a,
        ProtectiveReconcileAction::ReplaceReduceOnlyStop { new_spec, .. }
            if new_spec.qty == Decimal::ONE && new_spec.reduce_only
    )));

    // 3. AEGIS sovereign risk fail-closed & approval + Treasury isolation
    let mut state = SovereignPortfolioState {
        account_equity_usd: Some(Decimal::from(20_000)),
        available_collateral_usd: Some(Decimal::from(15_000)),
        reserved_notional_usd: Decimal::ZERO,
        open_positions: vec![SovereignPositionInput {
            symbol: "ETH".into(),
            owner_strategy: "mom".into(),
            side: Side::Long,
            notional_usd: Decimal::from(2_000),
            liquidation_distance_pct: Some(Decimal::from(40)),
            funding_bps_1h: Some(Decimal::ONE),
        }],
        daily_pnl_usd: Some(Decimal::ZERO),
        weekly_pnl_usd: Some(Decimal::ZERO),
        high_water_equity_usd: Some(Decimal::from(20_000)),
        pairwise_correlations: BTreeMap::new(),
        emergency_suspended: false,
        breaker_reason: None,
    };
    let prop = SovereignAdmissionProposal {
        symbol: "BTC".into(),
        owner_strategy: "adaptive_ema_anchor".into(),
        side: Side::Long,
        requested_notional_usd: Decimal::from(2_000),
        estimated_fee_usd: Decimal::from(2),
        estimated_liquidation_distance_pct: Some(Decimal::from(30)),
        market_depth_usd: Some(Decimal::from(200_000)),
        market_volatility_pct: Some(Decimal::from(2)),
        funding_bps_1h: Some(Decimal::ONE),
        instrument_max_leverage: Some(Decimal::from(20)),
        requested_leverage: Decimal::from(5),
        rearm_authorization: None,
    };
    assert!(matches!(
        evaluate_sovereign_admission(&SovereignRiskPolicy::default(), &state, &prop),
        SovereignRiskDecision::Approved { .. }
    ));
    state.available_collateral_usd = None;
    assert!(matches!(
        evaluate_sovereign_admission(&SovereignRiskPolicy::default(), &state, &prop),
        SovereignRiskDecision::Rejected { .. }
    ));

    let tdir = tempfile::tempdir().unwrap();
    let tstore = TreasuryStore::new(tdir.path().to_path_buf());
    let key = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let addr = hydra::hyperliquid_wire::derive_address(key).unwrap();
    tstore.save_offering(&addr, key, true, true).unwrap();
    assert!(tstore
        .load_secret_with_scope(SigningAccessScope::UiReadOnly)
        .is_err());

    // 4. Authenticated Engine IPC + PendingCertification enforcement
    let ipc_dir = tempfile::tempdir().unwrap();
    let mut srv = EngineIpcServer::start(ipc_dir.path(), "immortality").unwrap();
    let port = srv.descriptor.port;
    let bad_resp = EngineIpcClient::send_raw(
        port,
        &AuthenticatedEngineRequest {
            token_hex: "bad".into(),
            nonce: 1,
            issued_at_ms: Utc::now().timestamp_millis(),
            command: EngineControlCommand::Heartbeat,
        },
    )
    .unwrap();
    assert!(!bad_resp.ok);
    let susp_resp = EngineIpcClient::send_command(
        ipc_dir.path(),
        "immortality",
        1,
        EngineControlCommand::EmergencySuspend {
            reason: "integration".into(),
        },
    )
    .unwrap();
    assert!(susp_resp.ok && susp_resp.emergency_suspended);
    srv.stop();

    let checklist = evaluate_certification_readiness();
    assert!(!checklist.live_trading_authorized);
    assert_eq!(checklist.overall_status, "PendingCertification");
    assert_eq!(
        checklist.pillars.last().unwrap().status,
        PillarStatus::PendingExternalValidation
    );
    assert!(require_certified_live_execution(&LiveAuthorizationRequest {
        channel: ExecutionChannel::IpcControl,
        configured_live_trading: true,
        cli_live_requested: true,
        treasury_authorized: true,
        emergency_suspended: false,
    })
    .is_err());
}

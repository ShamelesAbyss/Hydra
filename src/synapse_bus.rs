//! Versioned typed internal Nervous System event bus connecting all seven heads
//! (**HUNTER**, **FANG**, **COGNITION**, **AEGIS**, **NEXUS**, **ORACLE**, **GENESIS**).
//!
//! Safety & truthfulness:
//! - Bounded FIFO buffer prevents unconstrained memory growth under bursty streams.
//! - Every event carries explicit provenance (`PublicMarket`, `CausalInference`,
//!   `DeterministicPlan`, `IndependentRisk`, `SimulatedPaperExecution`, etc.), correlation ID,
//!   and monotonic event sequence ID so simulated paper events can never impersonate
//!   certified live execution.

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const SYNAPSE_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_BUS_CAPACITY: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadId {
    Hunter,
    Fang,
    Cognition,
    Aegis,
    Nexus,
    Oracle,
    Genesis,
}

impl HeadId {
    pub const fn index(self) -> usize {
        match self {
            Self::Hunter => 0,
            Self::Fang => 1,
            Self::Cognition => 2,
            Self::Aegis => 3,
            Self::Nexus => 4,
            Self::Oracle => 5,
            Self::Genesis => 6,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Hunter => "HUNTER",
            Self::Fang => "FANG",
            Self::Cognition => "COGNITION",
            Self::Aegis => "AEGIS",
            Self::Nexus => "NEXUS",
            Self::Oracle => "ORACLE",
            Self::Genesis => "GENESIS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataProvenance {
    AuthoritativeExchange,
    DerivedMarketStat,
    SimulationAssumption,
    SyntheticTestFixture,
    PublicMarket,
    CausalInference,
    DeterministicPlan,
    IndependentRisk,
    SimulatedPaperExecution,
    HistoricalValidation,
    EvolutionProposal,
    OperationalFault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SynapsePayload {
    HunterObservation {
        watchlist: Vec<String>,
        eligible_count: usize,
        top_symbol: Option<String>,
        top_score: Option<f64>,
        stale_rejected: usize,
        stream_healthy: bool,
    },
    CognitionAssessment {
        symbol: String,
        regime: String,
        confidence_lb: Option<f64>,
        sample_count: usize,
        simulated_only: bool,
    },
    FangPlanEvaluated {
        selected_count: usize,
        rejected_count: usize,
        top_candidate_id: Option<String>,
        top_symbol: Option<String>,
        rejection_reasons: Vec<String>,
    },
    AegisDecision {
        candidate_id: String,
        symbol: String,
        approved: bool,
        reason: String,
        exposure_ratio: String,
        breaker_latched: bool,
    },
    NexusPaperExecution {
        intent_id: String,
        symbol: String,
        action: String,
        price: String,
        qty: String,
        notional: String,
        fee_usd: String,
        realized_pnl_usd: Option<String>,
        open_positions: usize,
    },
    OracleValidation {
        strategy: String,
        net_return_pct: f64,
        max_drawdown_pct: f64,
        trades: usize,
    },
    GenesisProposal {
        generation: u64,
        pareto_size: usize,
        best_score: f64,
    },
    PipelineFault {
        stage: String,
        message: String,
    },
}

impl SynapsePayload {
    pub fn summary(&self) -> String {
        match self {
            Self::HunterObservation {
                watchlist,
                eligible_count,
                top_symbol,
                stale_rejected,
                ..
            } => format!(
                "watchlist={} ({eligible_count} eligible, {stale_rejected} stale) top={}",
                watchlist.join(","),
                top_symbol.as_deref().unwrap_or("none")
            ),
            Self::CognitionAssessment {
                symbol,
                regime,
                sample_count,
                ..
            } => format!("{symbol} regime={regime} samples={sample_count}"),
            Self::FangPlanEvaluated {
                selected_count,
                rejected_count,
                top_symbol,
                ..
            } => format!(
                "selected={selected_count} rejected={rejected_count} top={}",
                top_symbol.as_deref().unwrap_or("none")
            ),
            Self::AegisDecision {
                symbol,
                approved,
                reason,
                exposure_ratio,
                ..
            } => format!(
                "{symbol} {} ({reason}, exp_ratio={exposure_ratio})",
                if *approved { "APPROVED" } else { "BLOCKED" }
            ),
            Self::NexusPaperExecution {
                symbol,
                action,
                price,
                notional,
                fee_usd,
                realized_pnl_usd,
                ..
            } => match realized_pnl_usd {
                Some(pnl) => format!(
                    "PAPER {action} {symbol} @{price} notional=${notional} fee=${fee_usd} net_pnl=${pnl}"
                ),
                None => format!(
                    "PAPER {action} {symbol} @{price} notional=${notional} fee=${fee_usd}"
                ),
            },
            Self::OracleValidation {
                strategy,
                net_return_pct,
                max_drawdown_pct,
                trades,
            } => format!(
                "{strategy} ret={net_return_pct:.2}% dd={max_drawdown_pct:.2}% trades={trades}"
            ),
            Self::GenesisProposal {
                generation,
                pareto_size,
                best_score,
            } => format!("gen={generation} pareto={pareto_size} score={best_score:.4}"),
            Self::PipelineFault { stage, message } => format!("FAULT[{stage}]: {message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynapseEvent {
    pub schema_version: u32,
    pub event_id: u64,
    #[serde(default)]
    pub session_id: Option<String>,
    pub correlation_id: String,
    pub timestamp_ms: i64,
    pub head: HeadId,
    pub provenance: DataProvenance,
    pub payload: SynapsePayload,
}

impl SynapseEvent {
    /// Returns `true` for durable decision/execution/fault events that must be persisted
    /// rather than treated as purely lossy decorative telemetry.
    pub fn is_critical(&self) -> bool {
        matches!(
            self.payload,
            SynapsePayload::FangPlanEvaluated { .. }
                | SynapsePayload::AegisDecision { .. }
                | SynapsePayload::NexusPaperExecution { .. }
                | SynapsePayload::OracleValidation { .. }
                | SynapsePayload::GenesisProposal { .. }
                | SynapsePayload::PipelineFault { .. }
        )
    }
}

#[derive(Debug, Clone)]
pub struct SynapseBus {
    capacity: usize,
    session_id: Option<String>,
    next_event_id: u64,
    dropped_events: u64,
    events: VecDeque<SynapseEvent>,
    latest_by_head: [Option<SynapseEvent>; 7],
}

impl Default for SynapseBus {
    fn default() -> Self {
        Self::new(DEFAULT_BUS_CAPACITY)
    }
}

impl SynapseBus {
    pub fn new(capacity: usize) -> Self {
        Self::with_session(capacity, None)
    }

    pub fn with_session(capacity: usize, session_id: Option<String>) -> Self {
        let cap = capacity.clamp(16, 8192);
        Self {
            capacity: cap,
            session_id,
            next_event_id: 1,
            dropped_events: 0,
            events: VecDeque::with_capacity(cap.min(256)),
            latest_by_head: Default::default(),
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn set_session_id(&mut self, session_id: Option<String>) {
        self.session_id = session_id;
    }

    pub fn publish(
        &mut self,
        correlation_id: impl Into<String>,
        timestamp_ms: i64,
        head: HeadId,
        provenance: DataProvenance,
        payload: SynapsePayload,
    ) -> &SynapseEvent {
        let event_id = self.next_event_id;
        self.next_event_id = self.next_event_id.saturating_add(1);
        let ev = SynapseEvent {
            schema_version: SYNAPSE_SCHEMA_VERSION,
            event_id,
            session_id: self.session_id.clone(),
            correlation_id: correlation_id.into(),
            timestamp_ms,
            head,
            provenance,
            payload,
        };
        if self.events.len() == self.capacity {
            self.events.pop_front();
            self.dropped_events = self.dropped_events.saturating_add(1);
        }
        self.latest_by_head[head.index()] = Some(ev.clone());
        self.events.push_back(ev);
        self.events.back().expect("just pushed event")
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn dropped_events(&self) -> u64 {
        self.dropped_events
    }

    pub fn total_published(&self) -> u64 {
        self.next_event_id.saturating_sub(1)
    }

    pub fn latest_for_head(&self, head: HeadId) -> Option<&SynapseEvent> {
        self.latest_by_head[head.index()].as_ref()
    }

    pub fn recent(&self, limit: usize) -> Vec<SynapseEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn all_chronological(&self) -> impl Iterator<Item = &SynapseEvent> {
        self.events.iter()
    }

    pub fn to_jsonl(&self) -> Result<String> {
        let mut out = String::new();
        for ev in &self.events {
            let line = serde_json::to_string(ev).context("serialize synapse event")?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    pub fn from_jsonl(raw: &str) -> Result<Vec<SynapseEvent>> {
        let mut out = Vec::new();
        for (idx, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let ev: SynapseEvent = serde_json::from_str(trimmed)
                .with_context(|| format!("decode synapse event line {}", idx + 1))?;
            ensure!(
                (1..=SYNAPSE_SCHEMA_VERSION).contains(&ev.schema_version),
                "unsupported synapse schema version {} on line {}",
                ev.schema_version,
                idx + 1
            );
            out.push(ev);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synapse_bus_enforces_fifo_capacity_and_tracks_latest_per_head() {
        let mut bus = SynapseBus::new(16);
        for i in 0..24 {
            bus.publish(
                format!("cyc-{i}"),
                1_700_000_000_000 + i,
                if i % 2 == 0 {
                    HeadId::Hunter
                } else {
                    HeadId::Aegis
                },
                DataProvenance::PublicMarket,
                SynapsePayload::HunterObservation {
                    watchlist: vec!["BTC".into()],
                    eligible_count: 1,
                    top_symbol: Some("BTC".into()),
                    top_score: Some(80.0),
                    stale_rejected: 0,
                    stream_healthy: true,
                },
            );
        }
        assert_eq!(bus.len(), 16);
        assert_eq!(bus.dropped_events(), 8);
        assert_eq!(bus.total_published(), 24);
        assert_eq!(
            bus.latest_for_head(HeadId::Hunter).map(|e| e.event_id),
            Some(23)
        );
        assert_eq!(
            bus.latest_for_head(HeadId::Aegis).map(|e| e.event_id),
            Some(24)
        );
    }

    #[test]
    fn synapse_bus_jsonl_replay_roundtrip_is_deterministic() {
        let mut bus = SynapseBus::default();
        bus.publish(
            "corr-1",
            1_700_000_001_000,
            HeadId::Nexus,
            DataProvenance::SimulatedPaperExecution,
            SynapsePayload::NexusPaperExecution {
                intent_id: "paper-1".into(),
                symbol: "ETH".into(),
                action: "OPEN_LONG".into(),
                price: "3200.50".into(),
                qty: "0.25".into(),
                notional: "800.125".into(),
                fee_usd: "0.32005".into(),
                realized_pnl_usd: None,
                open_positions: 1,
            },
        );
        let jsonl = bus.to_jsonl().expect("serialize jsonl");
        let decoded = SynapseBus::from_jsonl(&jsonl).expect("decode jsonl");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].head, HeadId::Nexus);
        assert_eq!(decoded[0].correlation_id, "corr-1");
    }
}

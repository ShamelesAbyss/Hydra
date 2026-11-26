//! Truthful presentation of local observations. No execution or engine authority.
use std::{collections::VecDeque, time::Duration};

/// Inputs must describe facts the viewer actually observed. Availability means
/// a local source, never an authenticated engine connection or heartbeat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeadInputs {
    pub source_available: bool,
    pub observations_available: bool,
    pub waiting_for_data: bool,
    pub job_running: bool,
    pub last_result_simulated: bool,
    pub research_result_available: bool,
    pub warning: bool,
    pub blocked: bool,
    pub faulted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadState {
    Idle,
    WaitingForData,
    Researching,
    Simulation,
    ResearchResult,
    HistoricalObservation,
    Warning,
    Blocked,
    Faulted,
    Unavailable,
}
impl HeadState {
    /// Safeguards and errors take precedence over attractive activity states.
    pub fn classify(input: HeadInputs) -> Self {
        if input.faulted {
            Self::Faulted
        } else if input.blocked {
            Self::Blocked
        } else if input.warning {
            Self::Warning
        } else if input.job_running {
            Self::Researching
        } else if input.last_result_simulated {
            Self::Simulation
        } else if input.research_result_available {
            Self::ResearchResult
        } else if input.source_available && input.observations_available {
            Self::HistoricalObservation
        } else if input.waiting_for_data {
            Self::WaitingForData
        } else if input.source_available {
            Self::Idle
        } else {
            Self::Unavailable
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::WaitingForData => "WAITING FOR DATA",
            Self::Researching => "RESEARCHING",
            Self::Simulation => "SIMULATION",
            Self::ResearchResult => "RESEARCH RESULT",
            Self::HistoricalObservation => "LOCAL HISTORY",
            Self::Warning => "WARNING",
            Self::Blocked => "BLOCKED",
            Self::Faulted => "FAULTED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Idle=>"Local source available; no observed subsystem work is active.",
            Self::WaitingForData=>"Waiting for actual input; no observations or activity are inferred.",
            Self::Researching=>"An actual local research job is running; this does not authorize trading.",
            Self::Simulation=>"The last completed result is simulated research, not live performance.",
            Self::ResearchResult=>"Completed local research; caller provenance is unverified and no live execution or financial authority is implied.",
            Self::HistoricalObservation=>"Recorded local evidence only; engine heartbeat and live connectivity are unverified.",
            Self::Warning=>"Observed warning requires review; it is not evidence of a protective order or successful execution.",
            Self::Blocked=>"An observed safeguard or unresolved condition blocks readiness; visual activity cannot clear it.",
            Self::Faulted=>"An observed source or job failure is present; successful operation is not inferred.",
            Self::Unavailable=>"No usable local source is available; engine availability and execution readiness are unverified.",
        }
    }
    pub const fn is_alert(self) -> bool {
        matches!(self, Self::Warning | Self::Blocked | Self::Faulted)
    }
    /// This presentation layer never grants financial or execution authority.
    pub const fn execution_authorized(self) -> bool {
        false
    }
}

/// A lifecycle word in retained history is not a current venue observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleEvidence {
    Prepared,
    Submitted,
    Acknowledged,
    FillRecorded,
    Unresolved,
    Other,
}
impl LifecycleEvidence {
    pub fn from_recorded_status(status: &str) -> Self {
        match status {
            "prepared" | "prepared_not_sent" => Self::Prepared,
            "submitted" => Self::Submitted,
            "acknowledged" => Self::Acknowledged,
            "filled" | "fill_recorded" => Self::FillRecorded,
            "unresolved" | "unknown" | "ambiguous" => Self::Unresolved,
            _ => Self::Other,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Prepared => "RECORDED PREPARATION",
            Self::Submitted => "RECORDED SUBMISSION",
            Self::Acknowledged => "RECORDED ACKNOWLEDGMENT",
            Self::FillRecorded => "RECORDED FILL EVIDENCE",
            Self::Unresolved => "RECORDED UNRESOLVED STATE",
            Self::Other => "UNCLASSIFIED LOCAL EVIDENCE",
        }
    }
    pub const fn settlement_verified(self) -> bool {
        false
    }
}

/// Travelling energy is permitted only for an actual observed event age.
/// Reduced motion removes travel; the caller still renders static state/alerts.
pub fn event_pulse(age: Option<Duration>, reduced_motion: bool) -> f32 {
    if reduced_motion {
        return 0.0;
    }
    age.map(|age| (1.0 - age.as_secs_f32() / 2.0).clamp(0.0, 1.0))
        .unwrap_or(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSummary {
    pub samples: usize,
    pub cadence_p50_ms: f64,
    pub cadence_p95_ms: f64,
    pub ui_build_p50_ms: f64,
    pub ui_build_p95_ms: f64,
    /// Median cadence converted to an observed UI update rate, not GPU timing.
    pub median_updates_per_second: f64,
}
/// Bounded actual measurements. Cadence includes scheduling/presentation effects;
/// UI build duration excludes GPU execution and may exclude tessellation.
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    samples: VecDeque<(f64, f64)>,
}
impl FrameStats {
    pub const CAPACITY: usize = 256;
    /// Reject invalid measurements rather than producing invented FPS or zeros.
    pub fn record(&mut self, cadence: Duration, ui_build: Duration) -> bool {
        self.record_seconds(cadence.as_secs_f64(), ui_build.as_secs_f64())
    }
    pub fn record_seconds(&mut self, cadence: f64, ui_build: f64) -> bool {
        if !cadence.is_finite()
            || !ui_build.is_finite()
            || cadence < 1e-9
            || ui_build < 0.0
            || !(cadence * 1000.0).is_finite()
            || !(ui_build * 1000.0).is_finite()
        {
            return false;
        }
        if self.samples.len() == Self::CAPACITY {
            self.samples.pop_front();
        }
        self.samples
            .push_back((cadence * 1000.0, ui_build * 1000.0));
        true
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    pub fn clear(&mut self) {
        self.samples.clear();
    }
    pub fn summary(&self) -> Option<FrameSummary> {
        if self.samples.is_empty() {
            return None;
        }
        let mut cadence: Vec<_> = self.samples.iter().map(|s| s.0).collect();
        let mut build: Vec<_> = self.samples.iter().map(|s| s.1).collect();
        cadence.sort_by(f64::total_cmp);
        build.sort_by(f64::total_cmp);
        // Nearest-rank percentiles retain observed values, without interpolation.
        let percentile =
            |v: &[f64], percent: usize| v[(v.len() * percent).div_ceil(100).saturating_sub(1)];
        let median = percentile(&cadence, 50);
        Some(FrameSummary {
            samples: self.samples.len(),
            cadence_p50_ms: median,
            cadence_p95_ms: percentile(&cadence, 95),
            ui_build_p50_ms: percentile(&build, 50),
            ui_build_p95_ms: percentile(&build, 95),
            median_updates_per_second: 1000.0 / median,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alerts_cannot_be_hidden_by_research_or_simulation() {
        let mut i = HeadInputs {
            source_available: true,
            observations_available: true,
            job_running: true,
            last_result_simulated: true,
            research_result_available: true,
            warning: true,
            blocked: true,
            faulted: true,
            ..Default::default()
        };
        assert_eq!(HeadState::classify(i), HeadState::Faulted);
        i.faulted = false;
        assert_eq!(HeadState::classify(i), HeadState::Blocked);
        i.blocked = false;
        assert_eq!(HeadState::classify(i), HeadState::Warning);
        i.warning = false;
        assert_eq!(HeadState::classify(i), HeadState::Researching);
        i.job_running = false;
        assert_eq!(HeadState::classify(i), HeadState::Simulation);
        i.last_result_simulated = false;
        assert_eq!(HeadState::classify(i), HeadState::ResearchResult);
        i.research_result_available = false;
        assert_eq!(HeadState::classify(i), HeadState::HistoricalObservation);
    }
    #[test]
    fn local_history_never_claims_live_connectivity_or_authority() {
        let state = HeadState::classify(HeadInputs {
            source_available: true,
            observations_available: true,
            ..Default::default()
        });
        assert_eq!(state, HeadState::HistoricalObservation);
        assert!(state.detail().contains("unverified"));
        for state in [
            state,
            HeadState::Researching,
            HeadState::Simulation,
            HeadState::ResearchResult,
            HeadState::Idle,
            HeadState::Warning,
            HeadState::Blocked,
            HeadState::Faulted,
            HeadState::Unavailable,
            HeadState::WaitingForData,
        ] {
            assert!(!state.execution_authorized());
        }
        assert_eq!(
            HeadState::classify(Default::default()),
            HeadState::Unavailable
        );
        assert_eq!(
            HeadState::classify(HeadInputs {
                waiting_for_data: true,
                ..Default::default()
            }),
            HeadState::WaitingForData
        );
    }
    #[test]
    fn retained_lifecycle_words_do_not_establish_fill_or_settlement() {
        assert_ne!(
            LifecycleEvidence::from_recorded_status("submitted"),
            LifecycleEvidence::FillRecorded
        );
        assert_ne!(
            LifecycleEvidence::from_recorded_status("acknowledged"),
            LifecycleEvidence::FillRecorded
        );
        for status in [
            "prepared_not_sent",
            "submitted",
            "acknowledged",
            "filled",
            "unresolved",
            "complete",
            "resting",
        ] {
            let evidence = LifecycleEvidence::from_recorded_status(status);
            assert!(!evidence.settlement_verified());
            assert!(!evidence.label().contains("LIVE"));
            assert!(!evidence.label().contains("SUCCESS"));
        }
        assert_eq!(
            LifecycleEvidence::from_recorded_status("complete"),
            LifecycleEvidence::Other
        );
    }
    #[test]
    fn absent_or_old_events_have_no_operational_energy() {
        assert_eq!(event_pulse(None, false), 0.0);
        assert_eq!(event_pulse(Some(Duration::from_secs(5)), false), 0.0);
        assert_eq!(event_pulse(Some(Duration::ZERO), false), 1.0);
        assert!(
            event_pulse(Some(Duration::from_millis(500)), false)
                > event_pulse(Some(Duration::from_millis(1500)), false)
        );
        assert_eq!(event_pulse(Some(Duration::ZERO), true), 0.0);
        assert!(HeadState::Blocked.is_alert());
        assert_eq!(HeadState::Blocked.label(), "BLOCKED");
    }
    #[test]
    fn frame_statistics_measure_observed_cadence_and_discard_old_samples() {
        let mut s = FrameStats::default();
        assert!(s.summary().is_none());
        for _ in 0..256 {
            assert!(s.record_seconds(1.0, 0.5));
        }
        for _ in 0..256 {
            assert!(s.record(Duration::from_millis(20), Duration::from_millis(2)));
        }
        let a = s.summary().unwrap();
        assert_eq!(a.samples, 256);
        assert_eq!(a.cadence_p50_ms, 20.0);
        assert_eq!(a.ui_build_p95_ms, 2.0);
        assert_eq!(a.median_updates_per_second, 50.0);
        for bad in [
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            f64::MIN_POSITIVE,
            -1.0,
            0.0,
        ] {
            assert!(!s.record_seconds(bad, 0.0));
        }
        assert!(!s.record_seconds(0.01, f64::NAN));
        assert_eq!(s.len(), 256);
        s.clear();
        assert!(s.is_empty());
    }
    #[test]
    fn percentile_tail_does_not_disappear_into_average() {
        let mut s = FrameStats::default();
        for _ in 0..90 {
            s.record_seconds(0.01, 0.001);
        }
        for _ in 0..10 {
            s.record_seconds(0.1, 0.02);
        }
        let a = s.summary().unwrap();
        assert_eq!(a.cadence_p50_ms, 10.0);
        assert_eq!(a.cadence_p95_ms, 100.0);
        assert_eq!(a.ui_build_p95_ms, 20.0);
    }
}

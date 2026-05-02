//! v1.5.0 Adaptive Learning System.
//!
//! Three coordinated subsystems:
//!  1. **Self-Tuning** (Tier A) — rolling expectancy per symbol / strategy
//!     drives `position_frac_mult` in the [0.5, 1.5] band and an adaptive
//!     edge-threshold. The bot tightens or loosens its own gates from real
//!     outcomes, no more hand-tuning every 48 hours.
//!  2. **Regime Brain** (Option 1) — multi-timeframe regime classifier
//!     (1m + 5m + 15m on resampled candles) plus per-(regime, symbol,
//!     strategy) expectancy table. The bot learns *which strategy works
//!     when* and weights signals by current regime confidence.
//!  3. **Memory Bot** (Option 3) — every trade closes with a full feature
//!     vector. At signal time we run a similarity search ("I've seen this
//!     setup 47 times before; win rate was 31%; SKIP") and combine it with
//!     time-of-day / day-of-week patterns.
//!
//! All three feed into one `AdaptiveContext` consumed by the risk engine.
//! Recomputed cheaply at the end of every closed trade.

pub mod context;
pub mod expectancy;
pub mod memory;
pub mod regime;
pub mod strictness;

pub use context::{AdaptiveContext, SignalDecision, SignalDecisionKind};
pub use expectancy::Expectancy;
pub use strictness::{compose as compose_strictness, DroughtState, StrictnessAdjustment};

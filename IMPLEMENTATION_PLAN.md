# Project Leviathan implementation plan

Development preserves the repository history, MIT license, Rust engine, terminal
interface and Hyperliquid mainnet integration. Shipping defaults remain disarmed.
No development milestone may submit live exchange orders.

## Milestone 1 — durable execution foundation (prerelease)

Implement a common submission boundary, configuration authorization, persistent
intent IDs, typed responses, metadata/slippage validation, confirmed residuals,
conservative startup checks and fault-injection tests. Keep unknown net PnL out
of learning. Publish verified Windows CLI artifacts and a recovery handoff.

## Milestone 2 — authoritative account ledger (stable-live prerequisite)

1. Model signer/account identity, margin/account modes and approved instruments.
2. Introduce fixed-point quantities/price validation using authoritative metadata.
3. Fetch order status by cloid and deduplicate fills by exchange identity; persist
   fees and funding in their native denomination and document conversion policy.
4. Reconcile outstanding intents, open orders and signed net positions at startup
   and periodically. Handle snapshot/stream race boundaries explicitly.
5. Apply position, learning context and net outcome updates atomically and exactly
   once; preserve original risk distance through restart.
6. Remove guessed/stale equity sizing; validate balance composition and freshness.
7. Add account-wide process exclusion, secure signing storage and durable breakers.
8. Verify crash windows, disconnections, partial IOC, unknown positions, manual
   exchange intervention, fee corrections and database failures with recorded fixtures.

Publish a stable milestone only when these contracts pass and supported execution
modes are accurately documented. Existing historical estimates require provenance;
do not retroactively relabel them as authoritative.

## Milestone 3 — independent engine and native desktop

Choose Rust-native eframe/WGPU after validating current APIs, Windows GPU and
software fallback behavior. Add authenticated local IPC and versioned redacted
telemetry. GUI reconnects without owning signing or engine lifetime. Build the
command center from real snapshots and persistent events; unavailable values
are visibly unavailable. Preserve Ratatui and headless builds. Verify resize,
DPI, minimize/reconnect and keyboard navigation before adding visual effects.

## Milestone 4 — shared planning and risk authority

Extract deterministic planning shared by live/paper/replay. Preserve existing
strategies. Add explicit net-position ownership/arbitration before combining
strategies per instrument. Port selected Passivbot math with provenance and
fixtures; add EMA Anchor, trailing/dynamic grids and bounded scaling incrementally.
AEGIS evaluates pending exposure and has sole authority over hard limits.

## Milestone 5 — discovery and cognition

Use approved markets, authoritative metadata, spread/depth/cost filters,
correlations and budgets for Hunter. Persist normalized entry context, confidence,
recency/sample-aware statistics and strategy attribution for Cognition. Shadow
candidate policies; risk constraints remain independent.

## Milestone 6 — Oracle and Genesis

Implement dataset provenance/integrity and conservative execution simulation with
fees, funding, partial fills, latency and walk-forward scenarios. Demonstrate
live/planner parity before optimizer integration. Add seeded CPU population
evaluation, NSGA-II, Pareto metrics, checkpoints and out-of-sample acceptance.
GPU acceleration is optional and requires CPU parity and reproducible benchmarks.

## Milestone 7 — visual workspaces and distribution

Build genuine workspace functionality on verified telemetry/research services.
Use emerald/cyan/ultraviolet selectively over graphite, clear mode labels and
readable numerical hierarchy. Analytical geometry has documented mappings;
animations correspond to actual events. Create original branding, icons,
screenshots, onboarding and a portable distribution before an installer.

Every coherent verified checkpoint updates status/tests/changelog, commits,
pushes, tags and publishes artifacts. Experimental incomplete work stays marked
prerelease. Never overwrite releases or rewrite history.

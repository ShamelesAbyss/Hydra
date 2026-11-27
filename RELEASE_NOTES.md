# v1.13.0 | PROJECT OMNISCIENCE — UNIFIED QUANTITATIVE INTELLIGENCE

> **Sovereign Trilogy Milestone I (`v1.13.0`).** Continuing directly from verified `v1.12.0`, **Project Omniscience** upgrades the intelligence, statistical discipline, cross-consumer strategy parity, multi-market portfolio simulation, constraint-aware NSGA-II evolution, and `v1.12.0` audit remediation across all seven heads of HYDRA Project Leviathan while keeping unattended live execution strictly locked (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`) and preserving the 3D WGPU PBR Leviathan sanctuary and Glow fallback unchanged.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Omniscience command center (with embedded `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.13.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.13.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (`hydra fang-registry`, `hydra cognition-stats`, `hydra oracle-portfolio`, `hydra genesis-nsga2`, `hydra paper-service`, `hydra synapse-paper`).
- **`hydra-1.13.0-source.zip`** — Exact-tag source export.
- **`hydra-1.13.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.13.0-SHA256SUMS.txt`** & **`hydra-1.13.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Project Omniscience & Audit Remediation

- **HUNTER Genuine Microstructure & Cross-Market Correlation (`src/hunter_microstructure.rs`, `src/hunter_stream.rs`, `src/hunter_ui.rs`):**
  - Derived causal point-in-time `MicrostructureSnapshot` analysis over genuine L2 book depth/imbalance, Kyle impact (`depth_impact_buy_bps` / `depth_impact_sell_bps`), trade-flow imbalance, spread widening ratios, OI delta (`open_interest_delta_pct`), and adverse-selection scores strictly tagged `DataProvenance::DerivedMarketStat` (never fabricating unset fields).
  - Timestamp-aligned Pearson cross-market correlation matrices (`CorrelationMatrix`), microservice signal utility lift evaluation, and deterministic empty-watchlist liquid universe bootstrap & resubscribe diffing (`HunterStreamClient::resolve_bootstrap_universe`).
- **COGNITION Statistical Intelligence, Calibration & Drift Detection (`src/cognition_stats.rs`, `src/diagnostics_ui.rs`, `src/cli/mod.rs`):**
  - Conjugate Bayesian expectancy shrinkage dampening low-sample luck (`BayesianExpectancyEstimate`), empirical Brier & ECE probability calibration (`evaluate_empirical_calibration`), explicit cost-adjusted expected value (`cost_adjusted_ev_bps`), Page-Hinkley strategy deterioration alerts (`detect_strategy_deterioration`), and conservative bounded `rank_and_weight_ensemble` weighting.
  - Strict separation between `OutcomeSource::Simulated` and uncertified `OutcomeSource::VerifiedNet`.
- **FANG Unified Strategy Engine & Cross-Consumer Parity (`src/fang_engine.rs`, `src/oracle.rs`, `src/core/engine.rs`, `src/paper_pipeline.rs`, `tests/fang_parity.rs`):**
  - Seven-strategy versioned registry (`fang-registry-v1`: `adaptive_ema_anchor`, `momentum_breakout`, `mean_reversion_bb`, `rsi_divergence`, `trend_continuation`, `volatility_breakout`, `bounded_grid_planner`), `TrailingEntryState`, `TrailingExitState`, and `compute_adaptive_take_profit`.
  - Authoritative cross-consumer parity across direct `FangUnifiedPlanner`, ORACLE `plan_unified_oracle_candidates`, Engine `plan_engine_candidates`, and `SynapsePipeline` verified bit-for-bit in `tests/fang_parity.rs`.
- **ORACLE Multi-Market Portfolio Simulation & Purged Walk-Forward (`src/oracle_research.rs`, `src/research_ui.rs`, `src/cli/mod.rs`):**
  - Synchronized multi-market replay modeling `>= 1`-bar execution latency, bid/ask spread, liquidity-sensitive square-root slippage, max-bar partial fills, exchange outage windows, conservative gap stops, hourly historical funding, Sharpe/Sortino/Calmar/CVaR-95, and buy-and-hold excess.
  - Strict train -> purge/embargo -> test `walk_forward_purged_cv` and deterministic circular block-bootstrap confidence intervals.
- **GENESIS Constraint-Aware Multi-Objective NSGA-II (`src/genesis_nsga2.rs`, `src/cockpit.rs`, `src/cli/mod.rs`):**
  - Deb constraint-dominated fast non-dominated sorting, crowding distance, bounded `std::thread::scope` deterministic worker evaluation, checkpoint continuation, and embargoed held-out validation with `auto_deployment_authorized: false`.
  - Recorded `OracleValidation` and `GenesisProposal` typed `SynapseEvent`s on completion in `Cockpit`, unifying all 7 heads on `SynapseBus`.
- **`v1.12.0` Operational & Accounting Audit Remediation (`src/paper_pipeline.rs`, `src/paper_store.rs`, `src/paper_service.rs`):**
  - Attributed funding cash flows onto per-position `accumulated_funding_usd` / `PaperClosedTrade::funding_usd` and `ShadowSample::normalized_r`; added `settle_funding_with_history`, `checked_current_drawdown_ratio`, `update_drawdown_checked`, `equity_complete` / `unavailable_mark_symbols` / `unattributed_funding_hours` completeness fields on `CycleSummary` & `paper_equity_history`, TOCTOU-free lock-held `archive_session_guarded` & `reset_session_confirmed_guarded`, and post-fill `(cur_eq - fee_usd)` exposure checks.

---

## Testing & Validation Results

- **517 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **35 test suites** (`302` lib unit tests + integration/doc suites), plus **471 passing offline headless tests** (`cargo test --locked --all-targets`) across **34 test suites** (`256` lib unit tests + integration/doc suites, including `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.13.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.13.0-linux-SHA256SUMS.txt
mkdir hydra-1.13.0 && tar -xzf hydra-1.13.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.13.0
cd hydra-1.13.0 && ./hydra --version && ./hydra fang-registry --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

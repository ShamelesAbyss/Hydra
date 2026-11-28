## 1.16.0 — v1.16.0 | PROJECT CONVERGENCE — UNIFIED EXECUTION & FINANCIAL TRUTH

- **Windows IPC 10054 Fix & Real Engine Authority (`src/engine_ipc.rs`, `src/paper_service.rs`):**
  Cleared inherited non-blocking mode on accepted sockets (`stream.set_nonblocking(false)`), added chunked bounded line reads for fragmented/slow frames, flushed & shutdown Write-half cleanly, bounded concurrent clients (`MAX_CONCURRENT_IPC_CLIENTS=8`), enforced fail-closed error fields (`mode: "UNKNOWN"`, `protective_stops_verified: false`, `maintenance_ready: false`), applied Windows owner ACLs (`icacls`) + `engine-ipc-audit.jsonl`, and wired `EngineControlAuthority` directly to live `Engine` state & persistent `meta_kv`.
- **Engine Convergence, Durable `SettlementStore`, Treasury Signer & Native `reduce_only` Stops (`src/core/engine.rs`, `src/fang_engine.rs`, `src/settlement.rs`, `src/treasury.rs`, `src/hyperliquid_wire.rs`, `src/adapters/mod.rs`, `src/adapters/hyperliquid.rs`, `src/live_gate.rs`, `src/protective_orders.rs`):**
  Replaced legacy `strat.evaluate()` with shared `FangUnifiedPlanner` execution (`BoundedGrid` marked `research_only: true`), wired fail-closed `evaluate_sovereign_admission`, `InstrumentOrderConstraints` precision rounding, `reconcile_startup_and_recovery`, place-new-then-cancel-old native `reduce_only` protective stops (`TriggerOrderAction` / `CancelAction`), `VenueTransportClass::TestnetSandbox`, `EngineSignerAuthority`, durable SQLite `SettlementStore` + `PendingFundingObligation`, ORACLE/GENESIS full input/config fingerprinting & SQLite `research_events`, plus `tests/convergence_e2e.rs` and actual-engine `tests/fang_parity.rs` (**535** desktop-wgpu / **489** headless tests, 0 `v1.16.0` clippy warnings).

## 1.15.0 — v1.15.0 | PROJECT SOVEREIGNTY — AUTONOMOUS QUANTITATIVE INTELLIGENCE

- **External OS Watchdog Supervision (`src/watchdog.rs`, `src/cli/mod.rs`):**
  Added exclusive `watchdog.lock` process supervision, SQLite `PRAGMA quick_check(1)` + DB byte-budget validation (`DbIntegrityReport`), real Win32 `GetDiskFreeSpaceExW` / POSIX `df -Pk` free-disk floor checks (`DiskSpaceReport`), bounded restart escalation logging (`watchdog-alerts.jsonl`), `PostRestartRevalidationChecklist` (`live_auto_resume_prohibited: true`), and `hydra watchdog-check`.
- **10-Scenario Fault Injection, Scientific Strategy Gating & Priority Isolation (`src/qualification.rs`, `src/cli/mod.rs`, `tests/sovereignty_suite.rs`):**
  Added 10-scenario fault-injection campaign (`run_fault_injection_campaign` — 10/10 pass across network loss, exchange outage, stale/malformed data, crash recovery, DB locking, disk exhaustion, corrupt DB, IPC tampering/oversized frames, resource contention, and restart continuity), honest real-clock `QualificationCampaignReport`, `evaluate_scientific_strategy_acceptance` (`EligibleForMonitoredPilot` vs. `RejectedKeepDisabled`), `ResourceBudgetScheduler`, and CLI commands (`qualify-campaign`, `strategy-accept`).
- **6-Pillar `PendingCertification`, 7-Head `SynapseBus` Verification & Section 26 `v2.0.0` Matrix (`src/live_gate.rs`, `READINESS_V2.md`, `tests/sovereignty_suite.rs`):**
  Expanded `evaluate_certification_readiness` with Pillar 5 (`ExternalWatchdog`: `OfflineFixtureVerified`) and Pillar 6 (`ExtendedMultiWeekPublicPaperAndOwnerSigning`: `PendingExternalValidation`), verified all seven `HeadId`s connected across `SynapseBus`, and published `READINESS_V2.md` holding `v2.0.0` strictly until multi-week public paper soak & owner `PINNED_RELEASE_SIGNING_PUBKEY_HEX` complete. Verified across **530** desktop-wgpu (`37` suites) / **484** headless (`36` suites) tests while keeping `MODE: DORMANT`, Glow fallback, GitHub compliance, and Ashlan's permanent memorial unchanged.

## 1.14.0 — v1.14.0 | PROJECT IMMORTALITY — FINANCIAL TRUTH & SOVEREIGN PROTECTION

- **NEXUS Authoritative Financial Settlement (`src/settlement.rs`, `src/cli/mod.rs`):**
  Added exact `Decimal` FIFO partial-fill round-trip settlement attributing entry fees, exit fees, maker rebates, and hourly funding windows (`FundingCoverageWindow`), retaining explicit `SettlementHoldReason`s rather than inferring zeros, reconciling external deposits/withdrawals cleanly, enforcing at-most-once `emit_eligible_learning_outcomes`, and exposing `hydra settlement-audit`.
- **NEXUS Execution Recovery & Exchange-Native Protective Orders (`src/execution_recovery.rs`, `src/protective_orders.rs`):**
  Added full `OrderLifecycleState` separation, `InstrumentOrderConstraints` tick/lot rounding, duplicate-`cloid` blocking, `check_rest_ws_consistency`, blind-retry refusal on ambiguous submissions, startup ownership recovery, and `ProtectiveOrderManager::reconcile_positions_and_stops` managing `reduce_only: true` stops even when new entries are blocked.
- **AEGIS Sovereign Risk, Treasury Isolation & Authenticated Engine IPC (`src/aegis_sovereign.rs`, `src/treasury.rs`, `src/engine_ipc.rs`, `src/live_gate.rs`, `tests/immortality_suite.rs`):**
  Added fail-closed `evaluate_sovereign_admission` (post-fee exposure, correlated clusters, liquidation buffer, funding drag, daily/weekly loss breakers), `SigningAccessScope` (`EngineExclusive` vs `UiReadOnly`) + encrypted-backend guard + `revoke_live_authorization`, OS-scoped replay/nonce/token-protected `EngineIpcServer`, and 5-pillar `evaluate_certification_readiness` (`hydra certification-status`), verified across **527** desktop-wgpu / **481** headless tests while keeping `MODE: DORMANT`, Glow fallback, GitHub compliance, and Ashlan's permanent memorial unchanged.

## 1.13.0 — v1.13.0 | PROJECT OMNISCIENCE — UNIFIED QUANTITATIVE INTELLIGENCE

- **HUNTER Microstructure & Cross-Market Correlation (`src/hunter_microstructure.rs`, `src/hunter_stream.rs`, `src/hunter_ui.rs`):**
  Added causal point-in-time `MicrostructureSnapshot` metrics (L2 depth/imbalance, Kyle impact, trade-flow imbalance, spread widening, OI delta, adverse selection tagged `DataProvenance::DerivedMarketStat`), timestamp-aligned Pearson `CorrelationMatrix`, signal lift evaluation, and empty-watchlist bootstrap universe resolution (`HunterStreamClient::resolve_bootstrap_universe`).
- **COGNITION Statistical Intelligence (`src/cognition_stats.rs`, `src/diagnostics_ui.rs`, `src/cli/mod.rs`):**
  Added conjugate Bayesian R shrinkage (`BayesianExpectancyEstimate`), empirical Brier/ECE probability calibration, `cost_adjusted_ev_bps`, Page-Hinkley strategy deterioration alerts, and conservative `rank_and_weight_ensemble` weighting with strict `OutcomeSource::Simulated` vs. `VerifiedNet` separation.
- **FANG Unified Engine & Cross-Consumer Parity (`src/fang_engine.rs`, `src/oracle.rs`, `src/core/engine.rs`, `src/paper_pipeline.rs`, `tests/fang_parity.rs`):**
  Added 7-strategy versioned registry (`fang-registry-v1`), `TrailingEntryState`, `TrailingExitState`, `compute_adaptive_take_profit`, and bit-for-bit cross-consumer parity across `FangUnifiedPlanner`, ORACLE, Engine, and `SynapsePipeline`.
- **ORACLE Multi-Market & GENESIS Constraint-Aware NSGA-II (`src/oracle_research.rs`, `src/genesis_nsga2.rs`, `src/research_ui.rs`, `src/cockpit.rs`, `src/cli/mod.rs`):**
  Added synchronized multi-market portfolio simulation (latency, spread, sqrt impact, partial fills, outage windows, hourly funding, Sharpe/Sortino/Calmar/CVaR-95, buy-and-hold excess), purged/embargoed walk-forward CV with circular block bootstrap CI, deterministic multi-worker NSGA-II (`auto_deployment_authorized: false`), `OracleValidation` / `GenesisProposal` bus events, and offline CLI commands (`fang-registry`, `cognition-stats`, `oracle-portfolio`, `genesis-nsga2`).
- **`v1.12.0` Audit Remediation & Test Suites (`src/paper_pipeline.rs`, `src/paper_store.rs`, `src/paper_service.rs`):**
  Attributed per-position & closed-trade funding into `normalized_r`, persisted `equity_complete` nullable columns, enforced lock-held `archive_session_guarded` / `reset_session_confirmed_guarded`, and verified **517** desktop-wgpu / **471** headless tests while preserving Glow fallback, `MODE: DORMANT`, GitHub compliance, and Ashlan's permanent memorial.

## 1.12.0 — v1.12.0 | PROJECT ASCENDANCY — OPERATIONAL RELIABILITY & TRANSACTIONAL TRUTH

- **Self-Contained Standalone Detached Paper Service (`src/bin/hydra-cockpit.rs`, `src/paper_service.rs`):**
  Added internal windowless `--paper-service-worker` mode to `hydra-cockpit` so `PaperServiceClient::spawn_detached` spawns the published standalone executable directly without needing a sibling `hydra.exe`.
- **Transactional Service Loop, Stream Watchdog & Bounded IPC (`src/paper_service.rs`, `src/cockpit.rs`):**
  Staged pipeline cycles before `commit_cycle`, halted cleanly into `PaperSessionStatus::Faulted` with persisted `fault_reason` on heartbeat/pipeline/SQLite errors, persisted observation metadata to `paper_observations`, bounded local TCP IPC line reads via `Read::take(MAX_IPC_LINE_BYTES + 1)` before allocation, propagated IPC DB errors, and added stream-exit watchdog restarts.
- **Complete Session Checkpoint Recovery & Ownership Guards (`src/paper_store.rs`, `src/synapse_bus.rs`, `src/cli/mod.rs`):**
  Persisted `PipelineRecoveryCheckpoint` (`next_event_id`, `dropped_events`, `simulated_samples`, `last_funding_applied_ms`, `demo_candles`) inside atomic `IMMEDIATE` transactions and restored bus history on `load_pipeline`; added `archive_session_guarded` & `reset_session_confirmed_guarded` blocking active `Running`, `Paused`, and OS-locked sessions.
- **Independent Per-Field Freshness, History Duration & Funding Settlement (`src/hunter_stream.rs`, `src/hunter.rs`, `src/paper_pipeline.rs`):**
  Added `FieldFreshness` and `MarketFreshnessReport`, gated both simulated exits and entries on fresh marks, replaced synthetic `completed_candles.len() / 24.0` listing-age derivation with exact `observed_history_minutes` and `instrument_listing_age_days: None`, and settled `apply_simulated_funding` at crossed hourly boundaries without double-charging.
- **Structural Gate Isolation (`src/live_gate.rs`, `src/adapters/hyperliquid.rs`, `tests/live_gate_adversarial.rs`):**
  Added sealed `VenueTransportClass` (`ProductionMainnet`, `#[cfg(test)] LoopbackTestFixture`), verified across **504** desktop-wgpu / **458** headless tests while preserving Glow fallback, `MODE: DORMANT`, GitHub compliance, and Ashlan's permanent memorial.

## 1.11.0 — v1.11.0 | PROJECT ASCENDANCY — TRUSTWORTHY INGESTION, DURABLE PAPER & EXECUTION GATE

- **Authoritative `PendingCertification` Live Gate (`src/live_gate.rs`, `tests/live_gate_adversarial.rs`):**
  Enforced one fail-closed `PendingCertification` gate across `Engine::new`, `ExecutionEngine::submit`
  (when targeting a non-loopback production venue), `HyperliquidAdapter::new`,
  `HyperliquidAdapter::submit_order_impl`, `StateManager::emergency_flatten`, and
  `Treasury::ReadinessReport`, plus adversarial offline tests proving no flag or constructor bypasses it.
- **Genuine HUNTER WebSocket & REST Snapshot Ingestion (`src/hunter_stream.rs`):**
  Implemented `tokio-tungstenite` (`rustls-tls-webpki-roots`) public Hyperliquid ingestion
  (`l2Book`, `trades`, `candle`, `activeAssetCtx`, REST fallback during reconnects, heartbeat
  `ping`/`pong`, bounded duplicate `tid` filtering, out-of-order tracking, separated completed
  candles, explicit `exchange_ts_ms` vs `received_at_ms`, derived metrics returning `None` when
  `< 12` completed bars, and `watchlist ∪ held_positions` continuous monitoring).
- **Fabricated Input Elimination & Hardened `SynapseBus` / Paper Admission (`src/synapse_bus.rs`, `src/paper_pipeline.rs`):**
  Upgraded `SynapseBus` to schema v2 (`AuthoritativeExchange`, `DerivedMarketStat`,
  `SimulationAssumption`, `SyntheticTestFixture`, `session_id`, `is_critical()`), removed the
  hardcoded GUI paper worker approximations, added `checked_mark_to_market_equity` fail-closed
  missing-mark halts, post-fill notional/fee validation, and `apply_simulated_funding`.
- **Isolated Durable SQLite Paper Store & Independent Engine Service (`src/paper_store.rs`, `src/paper_service.rs`, `src/cli/mod.rs`, `src/cockpit.rs`):**
  Added dedicated SQLite `paper_sessions.db` separating `public_paper`, `offline_demo`, and
  `historical_replay` sessions with crash/restart recovery and confirmation-gated resets, OS-locked
  headless `paper-service` with authenticated local loopback IPC (`paper-control/`), `hydra paper-service`
  CLI, and persistent Cockpit client controls, verified across **496** desktop-wgpu / **450** headless tests
  while preserving Glow fallback, `MODE: DORMANT`, GitHub compliance, and Ashlan's permanent memorial.

## 1.10.0 — v1.10.0 | PROJECT SYNAPSE — CONNECTED INTELLIGENCE

- **Versioned Synapse Nervous System Event Bus (`src/synapse_bus.rs`):**
  Added schema-versioned (`SYNAPSE_SCHEMA_VERSION = 1`) bounded FIFO transport (capacity 512,
  explicit drop-oldest accounting), typed `HeadId` & `DataProvenance` tagging, correlation IDs,
  JSONL serialization/replay (`to_jsonl` / `from_jsonl`), and strongly typed cross-head payloads.
- **Connected `SIMULATION_ONLY` Multi-Head Paper Pipeline (`src/paper_pipeline.rs`):**
  Unified `HUNTER` (freshness & watchlist rotation) -> `COGNITION` (causal regime inference &
  simulated-outcome shadow stats) -> `FANG` (shared `plan` arbitration + `ema_anchor` & aligned
  trend structure gating) -> `AEGIS` (drawdown breaker, exposure ceilings, `-10%` margin stress,
  and protective exit priority) -> `NEXUS` Paper (`PaperLedger` checked-`Decimal` fills/exits,
  slippage & fees), plus `run_deterministic_demo_step` and bounded public-market worker
  `spawn_public_paper_worker`.
- **Native Overview Live Telemetry & Offline CLI (`src/cockpit.rs`, `src/cli/mod.rs`, `tests/synapse_pipeline.rs`):**
  Wired live `SYNAPSE NERVOUS SYSTEM` paper-simulation controls (`Run Paper Cycle (Offline)`,
  `Start / Stop Public Paper Stream`, `Reset Paper Ledger`), truthful metrics, and real bus-driven
  3D head activity pulses into `Workspace::Overview`, added offline `hydra synapse-paper`, and
  verified **485** desktop-wgpu / **439** headless offline tests while keeping `MODE: DORMANT`,
  `ENABLE_LIVE_TRADING=false`, Glow fallback, and Ashlan's permanent memorial unchanged.

## 1.9.0 — v1.9.0 | PROJECT SYNAPSE — SPATIAL CONSCIOUSNESS

- **Hybrid World-Space + Native `egui` Spatial Interface (`src/leviathan_spatial.rs`, `src/leviathan_layout.rs`, `src/cockpit.rs`):**
  Added interchangeable **`SPATIAL | CLASSIC`** modes, translucent obsidian-glass floating
  workspaces (`Head(0..=6)` + global panels) projected upright and back-to-front depth-ordered
  from world anchors, holographic energy conduits (`paint_spatial_conduits`), 3D drag-to-reposition
  via `LeviathanCamera::unproject_drag_delta`, pinning/collapse/close controls, persistent
  `%LOCALAPPDATA%\Hydra\spatial_prefs.json` settings, and permanent fixed top safety controls.
- **360° Orbital Camera, Collision Avoidance & Outer Cathedral (`src/leviathan_3d_math.rs`, `src/leviathan_mesh.rs`, `src/bin/hydra-cockpit.rs`):**
  Added pole-safe `CameraBasis`, `OrbitState` (`--mode`, `--orbit <deg>` CLI flags, primary orbit,
  secondary/middle pan, scroll zoom, `Reset Cam` / double-click / `Esc` overview return, floor &
  thorax collision avoidance), outer `ring_r = 11.2` cathedral colonnade for unobstructed 360°
  orbits, and a sculpted dorsal thoracic carapace ridge with 5 spine plates for rear coherence.
- **Creature Awareness, Seven Personalities & Whole-Body Compensation (`src/leviathan_awareness.rs`, `src/leviathan_scene.rs`):**
  Implemented critically damped tracking springs with anti-thrashing dwell hysteresis (`2.6–7.4s`)
  across all seven head personalities, open/dragged panel gaze and skull tracking, restrained
  thorax/wing/tail whole-body compensation, pairwise skull separation, and pointer-guarded canvas
  interaction while preserving Glow fallback (`--features desktop`), `MODE: DORMANT`, and Ashlan
  Haley Rapponotti's permanent memorial.

## 1.8.0 — v1.8.0 | PROJECT INCARNATION (The Living Sovereign — Articulated 3D Skeletal PBR Engine)

- **3D Skeletal & Procedural Dragon Anatomy (`src/leviathan_3d_math.rs`, `src/leviathan_mesh.rs`):**
  Upgraded the native WGPU centerpiece to a real-time 96-bone articulated 3D skinned-mesh system:
  sculpted thorax with sternal rib-prongs, faceted outer + inner-star **Amethyst Heart**, clawed
  forelimbs gripping a two-tier octagonal obsidian dais, scalloped backlit wings, 6-bone coiled
  dorsal-bladed tail, seven 6-bone necks with physical dorsal spines, seven distinct 3D dragon
  skulls with hinged jaws, twin nasal spurs, curved ivory fangs, 3D reptilian slit-pupil eyeballs,
  unique horn crowns, AEGIS 3D hex shield ring, ORACLE 3D astrolabe, and gothic sanctuary pillars.
- **4-Pass Native WGPU 3D PBR, Shadows, HDR Bloom & GPU Telemetry (`src/leviathan_renderer_3d.rs`, `src/desktop_renderer.rs`):**
  Implemented a 1536×1536 `Depth32Float` directional shadow pass with 3×3 PCF soft filtering,
  Cook-Torrance GGX PBR lighting over overlapping diamond dragon scales with amethyst sub-scale
  bioluminescence, translucent wing/structure blending, half-res separable HDR bloom (`Rgba16Float`),
  ACES filmic compositing, `SceneQuality::Cinematic` preset, and optional DX12/Vulkan
  `Features::TIMESTAMP_QUERY` live GPU pass execution timing in the cockpit HUD.
- **Smooth Camera Choreography & Workspace Upgrades (`src/leviathan_scene.rs`, `src/cockpit.rs`, `src/bin/hydra-cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`):**
  Added smooth Sovereign Overview ↔ heroic 3/4 Focused Head transitions (`Esc` to return,
  `--head <0..=6>` CLI flag, non-overlapping obsidian memorial plaque in close-ups), Overview
  7-head readiness strip + interactive equity crosshair, NEXUS lifecycle pipeline summary, and
  embedded horizontal bar charts in HUNTER, FANG, and AEGIS workspaces while preserving the Glow
  fallback (`--features desktop`), disabled live defaults (`MODE: DORMANT`), permanent GitHub
  compliance, and Ashlan Haley Rapponotti's permanent memorial dedication.

## 1.7.0 — v1.7.0 | PROJECT LEVIATHAN (Public Release Foundation & Rebirth of the Leviathan)

- **Rebirth of the Leviathan (`src/leviathan_shader.rs`, `src/leviathan_scene.rs`):** Replace
  the polygonal WGPU dragon with a real-time 2.5D PBR signed-distance-field WGSL shader
  featuring seven anatomically distinct sculpted obsidian dragon heads, imbricated overlapping
  dragon scales along articulated 3D cylindrical necks, deep amethyst sub-scale bioluminescence,
  curved organic keratin horn phenotypes, gleaming ivory-obsidian fangs, realistic slit-pupil
  eyes with independent cursor/attention tracking and smooth blinking eyelids, backlit membranous
  wings, dynamic Amethyst Heart point-light illumination, volumetric mist, floating embers, and
  ACES filmic tonemapping. Preserve the Glow fallback for non-WGPU hardware.
- **Performance & Adaptive Rendering (`src/cockpit.rs`, `src/leviathan_scene.rs`):** Add
  `SceneQuality::Ultra` preset (`Low`/`Medium`/`High`/`Ultra`) and rolling frame-cadence
  **Adaptive LOD** with tight GPU bounding-box culling so RTX 4070-class GPUs cruise smoothly
  while weaker GPUs automatically protect trading/research responsiveness.
- **Public Release Cycle & Manifests (`.github/workflows/release.yml`, `src/evolution.rs`):**
  Graduate to normal semantic versioning (`v1.7.0`, `draft: false`, `prerelease: false`,
  `make_latest: true`) with automated `/releases/latest` verification and structured
  `hydra-1.7.0-release-manifest.json`; internal prereleases explicitly pass `--latest=false`
  so they never displace the Latest public release.
- **Startup Database Health Check (`src/startup_diagnostics.rs`, `src/bin/hydra-cockpit.rs`):**
  Run a non-destructive read-only `PRAGMA quick_check(1)` at cockpit startup and log diagnostic
  status alongside clean-profile initialization and crash reporting.
- **Road to HYDRA 2.0 (`ROADMAP_V2.md`):** Publish the iterative `v1.x` → `v2.0.0` engineering
  and visual roadmap. Public distribution does not constitute certification of unattended live
  trading; all financial safeguards and Ashlan Haley Rapponotti's permanent memorial remain intact.

## 1.7.0-alpha.29 — Five-Head Project Awakening leap (HUNTER, FANG, COGNITION, AEGIS & ORACLE)

- **HUNTER (`src/hunter.rs`, `src/hunter_ui.rs`):** Add read-only public Hyperliquid perpetual
  market acquisition (`metaAndAssetCtxs`, 15s timeout, 4 MiB cap, zero wallet credentials),
  portfolio-aware diversification weighting for already-held symbols, deterministic watchlist
  hysteresis, loopback HTTP tests, and a live public radar table inside the unified cockpit.
- **FANG (`src/fang_planner.rs`, `src/diagnostics_ui.rs`):** Add bounded deterministic arithmetic/
  geometric grid planner enforcing strict level caps (`1..=20`), checked-Decimal cumulative
  notional ceilings, hard anti-Martingale multiplier bounds (`0.5..=1.5x`), weighted average
  entry ladders, take-profit targets, protective stop floors, and interactive cockpit planner UI.
- **COGNITION (`src/cognition_shadow.rs`, `src/diagnostics_ui.rs`):** Add zero-lookahead causal
  market-regime inference (`TRENDING_UP`, `TRENDING_DOWN`, `MEAN_REVERTING_RANGE`,
  `HIGH_VOLATILITY_TURBULENT`, `INSUFFICIENT_HISTORY`) from strictly completed candles with
  prefix causality tests and interactive cockpit inspection.
- **AEGIS (`src/portfolio_research.rs`, `src/diagnostics_ui.rs`):** Add checked-Decimal
  liquidation-distance estimator and simultaneous adverse-shock margin stress matrix
  (`-5%` to `-35%`) detecting maintenance-margin breaches, with interactive cockpit stress UI.
- **ORACLE (`src/oracle_validation.rs`, `src/research_ui.rs`):** Add historical dataset integrity
  auditor detecting timestamp reversals, missing-bar interval gaps, OHLC inversions, and extreme
  single-bar spikes alongside deterministic FIPS 180-4 SHA-256 dataset fingerprinting in cockpit UI.
- Preserve every financial safeguard, permanent GitHub compliance, disabled live defaults, and
  Ashlan Haley Rapponotti's permanent memorial dedication.

## 1.7.0-alpha.28 — Leviathan Evolution native updater, restored README artwork & permanent GitHub compliance

- Implement **LEVIATHAN EVOLUTION** integrated native update manager (`src/evolution.rs`,
  `src/evolution_ui.rs`) inside the unified cockpit with a prominent top-bar **`CHECK FOR UPDATES`**
  button, dedicated **`LEVIATHAN EVOLUTION`** (`Updates` / `Alt+U` / `--updates`) workspace tab,
  and genuine **GENESIS** head pulse during active checks and downloads.
- Support GitHub Releases API channel selection (`Development` / `Stable`, skipping drafts and
  never misusing `/releases/latest` for prereleases), full SemVer 2.0.0 comparison, OS/arch asset
  selection, release notes display, streaming download progress/cancellation, pure-Rust FIPS 180-4
  SHA-256 verification, cryptographic `secp256k1` signed-manifest provenance checks (keeping
  automatic installation strictly locked until pinned signing infrastructure is configured),
  trading-safe maintenance gating, verified backups, rollback, and persistent update diagnostics.
- Restore and redesign `README.md` with original obsidian-amethyst `docs/assets/leviathan-banner.svg`
  and `docs/assets/architecture.svg` artwork, genuine native WGPU screenshots, and GitHub-compliant
  distribution wording pointing exclusively to the repository's native GitHub Releases section.
- Add permanent GitHub compliance enforcement (`.github/scripts/verify-github-compliance.ps1`,
  `.cursor/rules/github-compliance.mdc`, `AGENTS.md`, and both CI workflows) preventing direct
  artifact download URLs, shorteners, mirrors, or broken/wrong-cased README image references.
- Preserve all financial safeguards, disabled live defaults, headless/Ratatui operation, and
  Ashlan Haley Rapponotti's permanent memorial dedication.

## 1.7.0-alpha.27 — standalone Windows graphical executable, startup diagnostics & automated releases

- Publish standalone double-clickable Windows graphical executable **`HYDRA-Leviathan-Windows-x64.exe`**
  as the primary Windows download alongside the advanced Windows portable ZIP, Linux CLI archive,
  exact-tag source ZIP, and SHA-256 checksum manifests.
- Add `src/startup_diagnostics.rs` for OS-native writable directory resolution
  (`%LOCALAPPDATA%\Hydra\data\hydra.db`, `logs\`, `treasury\`), clean-profile dormant database
  bootstrap, global panic hook, readable local crash reports, visible Win32 `MessageBoxW`
  startup error dialogs (never silent termination), and `catch_unwind` WGPU→Glow fallback.
- Adjust default initial cockpit window size to monitor-safe `1600 × 960` with wide 3-column
  command layout enabled at `>= 1480 px` for 1080p/1440p/4K displays.
- Add dedicated `.github/workflows/release.yml` verifying tag/version match, permanent memorial,
  tests, audit, binary smoke checks, non-draft GitHub Release creation, and post-publish
  unauthenticated HTTP 200 + SHA-256 integrity verification.
- Redesign `README.md` with prominent top-of-page Windows/Linux/Release Notes download links
  and embed the Permanent Definition of Done in `AGENTS.md`.

## 1.7.0-alpha.26 — living Leviathan WGPU sanctuary, Treasury & The Offering

- Transform the unified native cockpit with a GPU-accelerated WGSL volumetric
  sanctuary shader (`desktop-wgpu`) and layered Glow fallback (`src/leviathan_shader.rs`).
- Sculpt sweeping amethyst-veined cybernetic dragon wings, physiological breathing,
  smooth organic eyelid blinking, serpentine neck sway, seven distinct dragon
  phenotypes, dual-peak Amethyst Heart with 7 pylon conduits, and obsidian-glass rails.
- Add dedicated **LEVIATHAN TREASURY** tab (`Alt+T` / `--treasury`) and **THE OFFERING**
  onboarding altar with Windows DPAPI / Unix `0700`/`0600` OS-protected credential
  storage, zeroizing secret buffers, masked UI inputs, verbatim risk acknowledgment,
  read-only public-address Hyperliquid `/info` verification, truthful 12-metric
  portfolio view, six operational modes (`DORMANT`..`SUSPENDED`), and one-click
  `EMERGENCY SUSPEND`.
- Establish the 14-point Permanent Release Publication Protocol in `AGENTS.md` and
  automate exact-tag GitHub Release packaging/publication in CI.
- Preserve all financial safeguards, headless/Ratatui operation, and Ashlan Haley
  Rapponotti's permanent memorial dedication across the cockpit, README, CI, and packages.

## 1.7.0-alpha.25 — frozen-frontier held-out candidate assessment

- Freeze a completed training frontier before later held-out evaluation.
- Share candidate configuration and bound combined candidate/fold/scenario costs.
- Add offline CLI/native comparisons without deployment or evidence certification.
- Record the owner's next unified Leviathan visual campaign and post-campaign pause.

## 1.7.0-alpha.24 — GENESIS resumable generation checkpoints

- Share full and stepped search kernels; retain RNG/candidates/parents/history.
- Bind state to dataset/settings/options/build with bounded corruption checks.
- Publish synced immutable files without overwrite; preserve previous checkpoints.
- Add native generation-step controls and offline CLI; no automatic deployment.

## 1.7.0-alpha.23 — FANG / COGNITION / AEGIS research workspaces

- Add pure deterministic planning/EMA research, strict prior-outcome confidence and
  shadow attribution, and checked Decimal portfolio/correlation diagnostics.
- Add typed offline CLI and genuine native analytical panels; preserve engine history.
- Refuse planning-budget precision loss and unverified financial-evidence claims.
- Preserve all live controls, risk holds, disabled defaults and permanent memorial.

## 1.7.0-alpha.22 — ORACLE held-out cost scenarios

- Add fixed-configuration disjoint rolling validation with fresh state/capital.
- Compare bounded fee/slippage scenarios with full fold reports and clear summaries.
- Add wallet-independent `validate-research` and native scenario/fold workspaces.
- Preserve original simulation limitations and all live financial safeguards.

## 1.7.0-alpha.21 — balanced progression and HUNTER Radar

- Reassess all seven original milestones in an explicit acceptance/dependency matrix.
- Add bounded offline market eligibility, liquidity/cost ranking and watchlist hysteresis.
- Add wallet-independent CLI and native background Radar workspace, real observation
  plotting, filtering and Alt+1…7 subsystem navigation.
- Preserve execution/financial holds, disabled live defaults and memorial distribution.

# Changelog

## 1.7.0-alpha.20

- Add one-snapshot inspection-only intent evidence without repair, hold-release or
  settlement authorization.
- Bound native metadata before decoding and refuse unsupported fact/rowid schemas.

## 1.7.0-alpha.19

- Add bounded protocol-2 offline signing-preparation inspection and shared native
  evidence validation without inferred signing, submission or settlement.
- Preserve the permanent memorial dedication and include its rule in release packages.

## 1.7.0-alpha.18

- Integrate durable per-signer nonce reservation into native IOC sends on a blocking
  worker, replacing the process-local counter.
- Commit immutable engine signing-preparation metadata before signing; preserve
  nonce consumption and unresolved holds on preparation failure.

## 1.7.0-alpha.17

- Add the separate durable signer-nonce coordinator Rust foundation with atomic
  reservation, bounded drift/overflow refusal and process/restart tests.
- Keep the live sender unchanged pending explicit signing-preparation integration.

## 1.7.0-alpha.16

- Check retained rounded native IOC action bounds before exact receipt persistence
  and on subsequent native fact reads in the caller's SQLite transaction.
- Preserve historical absence without fabricated wire provenance; reject corrupt
  retained evidence and leave independent execution/risk/financial holds intact.

## 1.7.0-alpha.15

- Add bounded, read-only unsigned-action inspection through `wire-history`.
- Validate canonical encoded bytes and scoped local fields without wallet access,
  migrations, inferred submission or changes to independent holds.

## 1.7.0-alpha.14

- Persist actual native unsigned IOC actions before engine signing/network write.
- Bind the opaque capability to the unchanged submitted intent and account.
- Prevent duplicate attempts, persistence failures and scope mismatches from sending.
- Preserve all signing, receipt, financial and legacy/custom adapter behavior.

## 1.7.0-alpha.13

- Reconcile exact native acknowledged quantity against scoped ledger fills.
- Keep journal and fill evidence within one SQLite snapshot across connections.
- Bound indexed selection and verify payload/index provenance without duplicate matches.
- Preserve partial/unknown components and all independent settlement holds.

## 1.7.0-alpha.12

- Expose retained lifecycle evidence through a wallet-free offline inspector.
- Bound row windows, selected metadata and JSON; report incomplete history explicitly.
- Preserve read-only snapshots and reject unsupported/corrupt provenance without repair.
- Update CLI branding while keeping all trading and financial safeguards.

## 1.7.0-alpha.11

- Add bounded read-only native order-status observations through the Rust API.
- Retain immutable scoped lifecycle provenance with restart-safe replay semantics.
- Keep execution, financial and risk holds independent of lifecycle labels.
- Enforce a 64 KiB decoded status cap and cross-check canonical client identity.

## 1.7.0-alpha.10

- Enforce scoped native order identity uniqueness across intents and connections.
- Revalidate bounded durable intent/result evidence before using exact facts.
- Refuse duplicate historical identity migration without modifying records.
- Preserve all independent execution, precision and financial holds.

## 1.7.0-alpha.9

- Consolidate complete Leviathan history and flagship presentation on authoritative main.
- Retire original 1.6.2 and remove redundant development branches.
- Preserve exact post-receipt residual proof in the reduction transaction.
- Keep protective confirmation independent of equity, marks and open-order endpoints.
- Verify runtime partial/full reductions, no resend on failed proof and signed exact quantities.

## 1.7.0-alpha.8

- Preserve exact native fill facts with wire-bound validation and atomic provenance.
- Use exact new-position facts for ownership; retain explicit unknown residuals.
- Reject nonfinite optional receipt prices and preserve unresolved persistence failures.

## 1.7.0-alpha.7

- Commit live entry risk reservations and submitted intents atomically.
- Recheck scoped ownership, same-market orders and execution/settlement holds inside admission.
- Enforce shared storage identity and conservative bounded notional.
- Roll back partial admission failures; preserve breaker state on risk denial.
- Retain historical orphan/unknown holds and all independent settlement gates.

## 1.7.0-alpha.6

- Reconcile only proven terminal entry reservations against fresh scoped account evidence.
- Retain immutable batch evidence atomically with risk snapshots, holds and events.
- Preserve ambiguous, orphan, reduced/closed and unknown financial-settlement holds.
- Reuse validated snapshots for entries; bypass maintenance for protective reductions.
- Bound account acquisition, decoded responses and redacted diagnostic previews.
- Reject inconsistent zero-quantity exposure and unsupported contingent entry orders.

## 1.7.0-alpha.5

- Persist original entry risk/context and recover only proven durable entry applications.
- Reject unknown remote exposure before entry and explicitly release account guard locks.
- Remove the affected SDK/ethers graph; retain independent signing conformance on a
  narrow mainnet IOC client. Audit now reports zero vulnerability entries, one warning.
- Upgrade rust_decimal and Ratatui parents; preserve offline research and native cockpit.
- Redesign the repository presentation with original SVGs and actual application captures.


## 1.7.0-alpha.4 — research workspaces and execution isolation

- Add deterministic next-bar Oracle simulations using existing strategies with explicit costs.
- Add seeded bounded Genesis mutation/crossover and Pareto candidate research.
- Wire CLI and native research workspaces through bounded background execution.
- Continue protective exits after individual instrument close failures.
- Bind live databases to trading accounts and audit explicit legacy-history adoption.
- Reject decimal underflow/excess precision and make malformed response previews UTF-8 safe.
- Isolate research configuration from wallet fields and engine storage creation.

## 1.7.0-alpha.3 — native cockpit

- Add optional native read-only cockpit and seven subsystem workspaces over bounded SQLite snapshots.
- Add versioned telemetry JSON CLI and explicit safe local history initialization.
- Add eight official cross-language signing fixtures.
- Preserve headless builds through the optional desktop feature.
- Add desktop CI build/test coverage.

## 1.7.0-alpha.2 — account evidence and independent risk

- Add scoped decimal fill/funding ledger, exact-once replay/conflict handling, unknown-cost preservation, and raw Hyperliquid history parsers.
- Add read-only `sync-ledger` history retrieval with bounded timestamp-overlapping pagination.
- Add persistent Aegis drawdown breakers, immutable policy and transactional pending-exposure reservations; wire the live engine to fresh standard-mode account/open-order snapshots.
- Add same-user/host account ownership independent of database path.
- Validate agent-to-account binding before signing and reject unsupported collateral modes instead of combining unrelated balances.
- Require fresh risk equity; invalidate on retrieval/persistence errors.
- Block SDK signed-payload debug/trace logging; redact configuration Debug output.
- Upgrade compatible dependencies including bundled SQLite; audit improves 39 to 19 vulnerability entries, 20 to 17 warnings, with no suppressions.
- Make core schema migration transactional/versioned and stop swallowing ALTER failures.
- 162 offline tests pass. Unknown round-trip accounting remains blocked; this is not a stable-live release.

## 1.7.0-alpha.1 — Project Leviathan execution foundation

Experimental engineering checkpoint. Not a stable-live release.

- Make configuration the live authorization source; prevent CLI override.
- Persist UUID order intents before network writes; transmit client order IDs.
- Reject duplicate IDs and block new exposure on ambiguous/unapplied execution.
- Route shutdown and emergency reductions through the journaled submit path.
- Require typed confirmed fills and matching exchange residuals before changing positions.
- Persist partial remaining quantities and reduction evidence atomically.
- Keep unknown net PnL out of learning, compounding and realized-trade statistics.
- Refuse startup position mismatches instead of fabricating zero-PnL exits.
- Require instrument precision metadata and respect configured slippage/tick limits.
- Propagate corrupt position rows and critical storage read errors.
- Preserve existing strategy/learning tests and add offline execution/parser fault tests.
- Add Windows/Linux verification and portable packaging CI, source audits, roadmap,
  feature parity ledger and engineering handoff.
- Apply standard rustfmt formatting across the existing Rust source and tests.

Known incomplete areas: account/fill/open-order recovery; authoritative fees/funding;
equity/account-mode validation; account process ownership; dependency advisories;
protected credentials; native GUI; shared planner; historical research and optimization.

## 1.6.2 — baseline

Existing repository state at `5720ecde7565583e85dde3ebf48acafa71af35e0`.
No GitHub release/tag existed at initial inspection. See the archived original README
and Git history for earlier behavior; this project does not retroactively certify it.

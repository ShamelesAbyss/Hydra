# Project Leviathan milestone completion matrix

This is the current acceptance ledger, not a release counter. Reviewed against the
owner's original directive (sections 7–28 and strategic order in section 30), the
seven major milestones in IMPLEMENTATION_PLAN.md, actual source and tests through
alpha.25. **No original major milestone is fully certified complete.** Alpha.11–20
were predominantly NEXUS evidence increments; further optional forensic refinement
must not monopolize development while independent work remains.

## Essential gates versus independent work

- **Live certification blockers:** authoritative fee/funding/entry attribution and
  net settlement; complete restart/stream recovery and ownership; validated supported
  margin/protective-order behavior; secure credential storage; authenticated control
  IPC before writable GUI controls. Existing holds remain until genuine proof.
- **Development dependencies:** shared deterministic planning precedes claimed
  live/replay strategy parity and multi-strategy arbitration; point-in-time data and
  nonoverlapping OOS windows precede credible model selection; portfolio ownership
  precedes concurrent live grids/scaling. Historical net outcomes must be verified
  before feeding new live learning policies.
- **Independent now:** read-only market eligibility/ranking, offline strategy kernels,
  shadow statistics, portfolio-risk analysis, walk-forward/scenario research,
  optimizer checkpoints and OOS evaluation, native analytical workspaces and layout.
  These do not need settled live trades or signing/control access.
- **Defer without weakening controls:** full signed-envelope/database attestation,
  more forensic report variants, cross-host coordination beyond the supported
  single-user/host scope, installer/signing, bloom effects and GPU acceleration.
  Return to deferred requirements before certification of modes that need them.

## Seven original major milestones

| ID / acceptance requirements | Implemented capabilities | Verified evidence | Outstanding requirements | Dependencies / blockers | Deferred improvements | Next actionable objective |
|---|---|---|---|---|---|---|
| **M1 execution foundation** — preserve strategies; authorization; durable IDs; typed receipts; confirmed reductions; no invented PnL; recoverable executables | Durable intent/receipt boundary, fail-closed `PendingCertification` & `VenueTransportClass`, full `OrderLifecycleState`, `InstrumentOrderConstraints`, REST/WS consistency, blind-retry refusal & `reduce_only` protective stops (`execution_recovery`, `protective_orders`) | `execution_recovery` (2 tests), `protective_orders` (1 test), `execution_safety`, `execution_facts`, `immortality_suite`, `live_gate_adversarial` | Extended multi-week public paper soak & owner `PINNED_RELEASE_SIGNING_PUBKEY_HEX` (`Pillar 6`) | External elapsed public paper soak & owner release signing before `AWAKENED` (`READINESS_V2.md`) | Extra forensic formats / full signed-envelope attestation | Complete `>= 14–30` day public paper qualification campaign |
| **M2 authoritative ledger** — scoped identities; exact arithmetic; fill/fee/funding ledger; atomic outcomes; account ownership; secure signing | Exact `Decimal` FIFO settlement (`settlement.rs`) attributing entry/exit fees, rebates & hourly funding, retaining explicit holds vs zero-inference, external cash-flow equity reconciliation, at-most-once learning, DPAPI-encrypted `SigningAccessScope` & revocation | `settlement` (3 tests), `treasury` (6 tests), `ledger_hyperliquid`, `ledger_integration`, `immortality_suite` | Extended mainnet live-pilot small-fill reconciliation once Pillar 6 completes | Multi-week public paper qualification & owner release signing before `AWAKENED` | Cross-host coordination beyond single host/user | Preserve settlement truth during extended paper soak |
| **M3 independent engine / desktop** — native Windows; headless/TUI; genuine telemetry; reconnect; secure IPC; responsive rendering | eframe/WGPU 3D PBR cockpit, self-contained `--paper-service-worker`, isolated `paper_sessions.db`, replay/nonce/token-authenticated `EngineIpcServer` (`src/engine_ipc.rs`), external OS-locked `WatchdogSupervisor` (`src/watchdog.rs`), `watchdog-check`, `qualify-campaign`, `certification-status` & `settlement-audit` CLI | Native executable smoke/captures; `watchdog`; `qualification`; `sovereignty_suite`; `engine_ipc`; `paper_service`; `immortality_suite`; `tui_smoke` | Extended multi-week public-market runtime evidence (`actual_elapsed_seconds`) | Genuine elapsed multi-week soak + owner `secp256k1` signing key (`READINESS_V2.md`) | Installer packaging beyond portable/standalone `.exe` | Run continuous `paper-service` qualification across market regimes |
| **M4 shared planning / risk** — shared deterministic planner; Passivbot kernels; bounded grids/scaling; arbitration; independent Aegis | Authoritative unified 7-strategy `FangUnifiedPlanner` (`fang-registry-v1`), `TrailingEntryState`, `TrailingExitState`, adaptive TP, bounded grids (`levels <= 20`), fail-closed `evaluate_sovereign_admission` (post-fee exposure, correlated clusters, liq buffer, daily/weekly loss breakers) & 10-scenario fault-injection validation | `fang_engine`, `fang_parity`, `aegis_sovereign` (1 test), `protective_orders` (1 test), `qualification`, `sovereignty_suite`, `aegis_integration`, `atomic_admission` | Extended multi-day paper qualification & small-fill live pilot reconciliation | Multi-week public paper soak & scientific acceptance across real volatility regimes | Optional strategy variants and GPU acceleration | Validate candidate strategies with `hydra strategy-accept` over public data |
| **M5 discovery / cognition** — approved markets; liquidity/cost ranking; hysteresis; sample/recency confidence; attribution; shadow policies | Genuine causal HUNTER L2/trade-flow/OI microstructure & Pearson correlation + empty-watchlist liquid bootstrap (`hunter_microstructure`, `hunter_stream`); COGNITION Bayesian R shrinkage, empirical Brier/ECE calibration, cost-adjusted EV bps & Page-Hinkley drift detection (`cognition_stats`) | `hunter_microstructure` (3 tests), `hunter_stream` (6 tests), `cognition_stats` (2 tests), `cognition_shadow`, `hunter`, `learning_integration` | Autonomous live routing through certified Exchange & Aegis gates; `VerifiedNet` online learning from live fills | Positive multi-month OOS dataset evidence (`strategy-accept`) & `VerifiedNet` live pilot proof | Bandits/neural methods without measured OOS lift | Accumulate genuine public-market outcomes (`paper-service` -> `VerifiedNet`) |
| **M6 Oracle / Genesis** — historical integrity/provenance; realistic costs; shared parity; walk-forward/OOS; resumable multiobjective search | Synchronized multi-market portfolio simulator (latency >= 1 bar, spread, sqrt impact, partial fills, outage windows, hourly funding, Sharpe/Sortino/Calmar/CVaR-95, B&H excess); purged/embargoed walk-forward CV + circular block bootstrap CI (`oracle_research`); Deb constraint-aware multi-worker NSGA-II (`genesis_nsga2`), scientific acceptance gate & priority `ResourceBudgetScheduler` (`qualification.rs`) | `oracle_research` (2 tests), `genesis_nsga2` (1 test), `qualification`, `sovereignty_suite`, `oracle_integration`, `oracle_validation`, `genesis_integration`, `checkpoint_tests`, `genesis_oos` | Extended historical dataset library & multi-week paper-campaign cross-validation | Extended genuine historical + live paper telemetry (`READINESS_V2.md`) | Optional GPU candidate evaluation after CPU equivalence | Continuous paper-qualification feedback into ORACLE/GENESIS diagnostics |
| **M7 visual workspaces / distribution** — all seven functional workspaces; meaningful analytical geometry; true event animation; onboarding; original assets; packaging | Original SVG identity, articulated 96-bone WGPU 3D PBR sanctuary, `SPATIAL\|CLASSIC` modes, Glow fallback, self-contained `--paper-service-worker` standalone Windows executable, Omniscience HUNTER/FANG/COGNITION/ORACLE/GENESIS panels, all 7 `HeadId`s connected to typed `SynapseBus` events, offline CLI (`watchdog-check`, `qualify-campaign`, `strategy-accept`, `certification-status`, `settlement-audit`, `fang-registry`, `cognition-stats`, `oracle-portfolio`, `genesis-nsga2`) | Verified native WGPU captures; CLI/standalone `.exe` IPC smoke; 530 wgpu / 484 headless tests; CI & 3-way SHA-256 release verification | Owner cryptographic `secp256k1` release-signing key provisioning (`PINNED_RELEASE_SIGNING_PUBKEY_HEX`) & optional signed installers | Owner offline `secp256k1` key setup (`READINESS_V2.md` §3; `MODE: DORMANT` preserved) | Signed MSI/AppImage/deb installers | Owner provisioning of `PINNED_RELEASE_SIGNING_PUBKEY_HEX` for **LEVIATHAN EVOLUTION** |

## Head-by-head progression queue

| Head | Current boundary | Independent next delivery | Later / blocked live acceptance |
|---|---|---|---|
| HUNTER | Offline eligibility/ranking/hysteresis and native Radar | Point-in-time data acquisition and portfolio-aware research selection | Autonomous point-in-time acquisition, correlations/capital allocation; live routing through planner and Aegis |
| FANG | Offline deterministic arbitration/basic EMA Anchor and existing independent strategies | Shared live/replay consumer integration and conflict fixture parity | State-owned bounded live grids/scaling after ownership/recovery acceptance |
| COGNITION | Offline sample/recency attribution and strict historical shadow evaluation; legacy learning context | Inferred/calibrated regimes and stronger provenance | New live adaptation only from verified net outcomes; never override Aegis |
| AEGIS | Persistent hard limits/reservations; offline portfolio concentration/correlation diagnostics | Stress scenarios and portfolio-aware research selection | Native protective orders, liquidation/margin/funding-aware gates and operational qualification |
| NEXUS | Extensive conservative execution evidence | Prioritize complete settlement/recovery/credentials over additional report refinements | Unattended live certification remains blocked; no hold bypass |
| ORACLE | Deterministic replay and fixed-parameter held-out cost scenarios with native comparison | Data integrity/provenance and richer execution models | Shared parity, funding/queue/partial/latency/portfolio models and reliable datasets |
| GENESIS | CPU search/Pareto, immutable checkpoint/resume and frozen-frontier later held-out assessment | Visual campaign: actual generation/Pareto development in the unified creature | NSGA-II, parallel scheduling and optional GPU only after CPU correctness/parity |

## Prioritized backlog and cycle policy

1. **P0 preserve:** every financial guard, exact/unknown distinction, disabled live
   defaults, no signed development requests, immutable history and memorial.
2. **P1 next feature cycle:** Hunter Radar + cockpit; Oracle scenarios/walk-forward
   + cockpit; Genesis resumability/OOS; Fang shared planning and Cognition shadow
   confidence; Aegis portfolio analysis. Choose coherent acceptance slices.
3. **P1 certification track:** NEXUS net settlement, authoritative recovery and
   secure credentials; Aegis protective-order/account-model acceptance; secure IPC.
   Essential for live certification, not a reason to halt independent deliveries.
4. **P2 later:** advanced multi-instrument simulation, full grid variants, broader
   account modes, measured optional GPU, distribution signing/installer and effects.

At the start of each cycle, review **all seven rows** and record the chosen
acceptance slice, blockers and why other work is deferred. After several consecutive
single-head checkpoints, deliberately reassess independent work. Update this matrix
at every meaningful release. Do not equate alpha versions with major milestone
completion, and never accelerate by weakening guards or manufacturing evidence.

## Current cycle record

Reassessment baseline: alpha.20 / 368 local desktop-feature tests; all four source
CI jobs pass. Chosen next slice: **M5 HUNTER + M3/M7 native Radar**. Then M6 research
validation and M4/M5 planning/shadow work. Financial and signing safeguards remain.
Alpha.21 closes the initial offline HUNTER eligibility/ranking/hysteresis slice
and provides a native Radar workspace, bounded worker, filtering and liquidity map.
Eight core regressions plus CLI isolation verify deterministic, approval-bound,
read-only behavior. Autonomous acquisition, correlation/budgets and live rotation
remain outstanding; M5 and M7 are **partial**, not complete. Next cycle: M6 ORACLE
held-out validation and cost scenarios, then GENESIS resumability/OOS. The other
rows were reviewed; NEXUS certification work remains required before final release.

### Alpha.22 / M6 + M7 reassessment

All seven rows reviewed. Delivered disjoint rolling held-out evaluation and bounded
cost scenarios through CLI and native fold/scenario panels. Six focused tests pass;
full release verification is recorded in TEST_RESULTS.md. This is fixed-configuration
validation, not parameter fitting, independent dataset certification or shared
live/replay planner parity. M6 remains partial. Next: GENESIS checkpoint/resume and
candidate OOS; independent FANG planning, COGNITION shadow and AEGIS portfolio
research proceed without relaxing NEXUS holds. No major milestone is certified.

### Alpha.23 / M4 + M5 + M7 reassessment

All seven rows reviewed. FANG pure arbitration and basic EMA Anchor, COGNITION
source-separated strict historical shadow statistics, and AEGIS offline exposure/
correlation research now have CLI and native workspaces. Shared kernels are research
foundations; live/replay orchestration, calibrated inferred regimes, protective
orders and authoritative portfolio reconciliation remain outstanding. Native panels
preserve observed engine history and explicitly distinguish caller data.

Next coherent slice is **GENESIS resumability and candidate held-out assessment**
(M6/M7), then shared-planner integration with fixture parity. NEXUS's existing
financial/execution holds remain; settlement/recovery/credentials certification
must return before stable release. No original major milestone is complete.

### Alpha.24 / M6 + M7 reassessment

All seven rows reviewed. GENESIS now advances and resumes complete generations,
retaining exact random state, candidates/parents/history, context binding and an
immutable checkpoint publication path. CLI and native controls share that kernel.
No candidate fitness is recomputed on resume; serialized-step parity, context/
corruption refusal and no-overwrite behavior have targeted regressions.

Next M6 objective: bounded candidate assessment on chronologically later held-out
data, with explicit leakage/provenance limits. Shared live/replay planning remains
an M4 dependency; HUNTER autonomous acquisition and portfolio-aware selection,
COGNITION inferred/calibrated regimes, AEGIS protective orders, NEXUS financial
settlement/recovery/credentials and secure control IPC remain open. No major
milestone is fully certified and no live financial safeguard is relaxed.

### Alpha.25 / M6 reassessment and owner-directed sequence change

All seven rows reviewed. The current engineering slice adds frozen-training-frontier
assessment on strictly later held-out history/test data with combined resource caps,
shared candidate configuration and native/CLI comparison. It does not certify unseen
data, checkpoint authenticity, full walk-forward fitting or financial outcomes.

**After alpha.25 publication the normal queue is temporarily suspended by the owner.**
The next campaign is the unified, genuinely seven-headed native Leviathan described
in VISUAL_CAMPAIGN.md. Normal engineering blockers/deferred work remain recorded
above and are not discarded. After visual completion/publication, provide a full
handoff and pause for explicit owner authorization before returning to that queue.
No original major milestone is fully certified and no financial safeguard is waived.

### Alpha.26 — The Treasury, The Offering, and Autonomous Readiness Matrix

Added permanent architectural and native visual integration for **LEVIATHAN TREASURY**
and **THE OFFERING** (`src/treasury.rs`, `src/treasury_ui.rs`) inside the single unified
window, preserving every financial safeguard and Ashlan Haley Rapponotti's permanent
memorial:

- **Implemented now:** dedicated-wallet onboarding (**THE OFFERING**) with explicit
  dedicated-wallet warning and verbatim capital-loss acknowledgment; Windows DPAPI
  (`CryptProtectData`, current OS user) and Unix `0700`/`0600` persistent credential
  storage with zeroizing secret buffers, masked UI inputs, credential verification,
  replacement and secure overwrite-removal; read-only public-address Hyperliquid `/info`
  account verification (signer/agent role, standard perp `disabled` mode, equity,
  available collateral, margin used, unrealized PnL; private keys never touch the
  network); Offering illumination across the Amethyst Heart, AEGIS, and NEXUS only after
  genuine validation; 12-metric truthful Treasury portfolio view (never fabricating
  unavailable figures; deposits/withdrawals explicitly separated from trading profits);
  six operational modes (`DORMANT`, `OBSERVATION`, `SIMULATION`, gated `AWAKENED`,
  `GUARDIAN`, `SUSPENDED`) with prominent one-click **EMERGENCY SUSPEND** and persistent
  local suspension latch; and an explicit 14-item Autonomous Leviathan Architecture
  Matrix. No automatic withdrawal or arbitrary transfer capability exists.
- **Explicitly pending / unfinished (gated in UI & tracked here):**
  - Unattended live **AWAKENED** certification remains locked (`PendingCertification`)
    until complete fee/funding round-trip net settlement, external deposit/withdrawal
    cash-flow ledger reconciliation, full WebSocket stream/restart recovery,
    authenticated engine-control IPC, external watchdog supervision daemon, and
    configurable external notification channels are implemented and independently
    verified.
  - Headless engine operation remains independent of GUI responsiveness; live execution
    defaults remain disabled and no real-money trading was activated during development.

### Alpha.27 — Standalone Windows Distribution, Startup Diagnostics & Automated Release Verification

Corrected distribution architecture so ordinary Windows users download and double-click
**`HYDRA-Leviathan-Windows-x64.exe`** directly from GitHub Releases without needing Rust,
PowerShell, CLI arguments, repository files, or ZIP extraction. Added OS-native
`%LOCALAPPDATA%\Hydra` clean-profile bootstrap, readable local crash reports, visible Win32
startup error dialogs, `catch_unwind` WGPU→Glow fallback, dedicated `.github/workflows/release.yml`
with post-publish public download & SHA-256 checks, and the Permanent Definition of Done in
`AGENTS.md`. Preserves all financial safeguards and Ashlan Haley Rapponotti's permanent memorial.

### Alpha.28 — Permanent GitHub Compliance, Restored Obsidian-Amethyst Presentation & LEVIATHAN EVOLUTION

All seven milestone rows reviewed. Delivered:

- **Permanent GitHub Compliance & Restored Presentation (M7):** Restored `README.md` with
  original obsidian-amethyst `docs/assets/leviathan-banner.svg` and `docs/assets/architecture.svg`,
  genuine native WGPU screenshots, neutral Releases navigation pointing exclusively to the
  repository's native GitHub Releases section, and automated CI/local enforcement via
  `.github/scripts/verify-github-compliance.ps1` and `.cursor/rules/github-compliance.mdc`.
- **LEVIATHAN EVOLUTION Native Update Manager (M3 / M7):** Implemented `src/evolution.rs` and
  `src/evolution_ui.rs` with top-bar **`CHECK FOR UPDATES`** button, dedicated **`Updates`**
  (`Alt+U` / `--updates`) workspace tab, GitHub Releases channel selection (`Development` /
  `Stable`), SemVer 2.0.0 comparison, OS/arch asset matching, release notes, download progress and
  cancellation, pure-Rust FIPS 180-4 SHA-256 verification, cryptographic `secp256k1` signed
  manifest provenance verification (auto-install strictly locked until pinned signing keys are
  configured), trading-safe maintenance gating, staging/backup/rollback, and update diagnostics.
- **Next Project Awakening slices (continuing immediately on `main`):**
  1. **M4 FANG Shared Deterministic Planner Integration:** Connect the deterministic planner and
     ownership/budget arbitration into live/replay consumers with fixture parity.
  2. **M5 HUNTER Autonomous Public Market Acquisition & Portfolio-Aware Ranking + COGNITION
     Inferred Regime Calibration.**
  3. **M6 ORACLE Realistic Execution Models (Funding/Partial/Latency) & Dataset Provenance +
     GENESIS Multi-Objective Evolution.**
  4. **M1/M2/M4 Certification Track:** Complete round-trip fee/funding net settlement, stream/restart
     recovery, native protective orders, and authenticated local control IPC. No original major
     milestone is fully certified yet and no financial safeguard is weakened.

### Alpha.29 — Five-Head Project Awakening Leap (HUNTER, FANG, COGNITION, AEGIS & ORACLE)

All seven milestone rows reviewed. Delivered substantial independent acceptance increments across
five subsystems with immediate unified native cockpit integration (**462 passing offline tests**):

- **M5 HUNTER:** Read-only public Hyperliquid `metaAndAssetCtxs` acquisition (`src/hunter.rs`)
  with portfolio-aware diversification penalty for already-held positions and live public radar
  table in the cockpit (`src/hunter_ui.rs`). WebSocket streaming and planner-connected rotation
  remain next.
- **M4 FANG:** Checked-Decimal bounded deterministic arithmetic/geometric grid planner
  (`src/fang_planner.rs`) enforcing `1..=20` level limits, hard cumulative notional cap,
  anti-Martingale multiplier cap (`<= 1.5x`), monotonic take-profit, and protective stop floor,
  with interactive cockpit grid UI (`src/diagnostics_ui.rs`). Live/replay shared planner wiring
  remains next.
- **M5 COGNITION:** Zero-lookahead causal market-regime inference (`src/cognition_shadow.rs`) from
  completed candles with prefix causality tests and interactive cockpit inspection. Verified-net
  live learning adaptation remains gated on M2 settlement.
- **M4 AEGIS:** Checked-Decimal liquidation-distance estimator and simultaneous adverse-shock
  margin stress matrix (`-5%` to `-35%`, `src/portfolio_research.rs`) with interactive cockpit
  simulator (`src/diagnostics_ui.rs`). Native protective exchange orders and full live margin
  certification remain next.
- **M6 ORACLE:** Historical OHLCV dataset integrity & FIPS 180-4 SHA-256 provenance auditor
  (`src/oracle_validation.rs`) detecting reversals, interval gaps, OHLC inversions, and return
  spikes, wired directly into the ORACLE cockpit tab (`src/research_ui.rs`). Funding/partial-fill/
  latency simulation and GENESIS NSGA-II remain next alongside the M1/M2/M3 live certification
  track (round-trip net settlement, stream recovery, authenticated control IPC, watchdog). No
  original major milestone is fully certified yet and no financial safeguard is weakened.

### v1.7.0 — Public Release Foundation & Rebirth of the Leviathan

All seven milestone rows reviewed. Graduates HYDRA from the `v1.7.0-alpha.*` prerelease series
into the normal public `v1.x` semantic-versioning lifecycle (`draft: false`, `prerelease: false`,
`make_latest: true`, preserving all historical alpha releases and establishing `ROADMAP_V2.md`):

- **M3 / M7 Visual & Rendering Leap:** Replaced polygonal WGPU geometry with a full 2.5D PBR
  WGSL seven-headed obsidian-and-amethyst dragon sanctuary (`src/leviathan_shader.rs`) featuring
  overlapping normal-mapped scales, sub-scale amethyst bioluminescence, seven sculpted horn &
  head phenotypes, independent cursor-tracking slit eyes, backlit wings, dynamic Amethyst Heart
  point-light illumination, ACES tonemapping, `Low`/`Medium`/`High`/`Ultra` presets, and rolling
  **Adaptive LOD** while retaining the Glow OpenGL fallback.
- **M3 / M7 Distribution & Reliability:** Added structured `hydra-1.7.0-release-manifest.json`
  generation and `/releases/latest` verification in `.github/workflows/release.yml`, updated
  **LEVIATHAN EVOLUTION** to default to public `Stable` releases on `v1.7.0+`, and added read-only
  SQLite `PRAGMA quick_check(1)` startup health diagnostics (`src/startup_diagnostics.rs`).
- **Explicit Live-Certification Boundary:** Public release `v1.7.0` does **not** certify
  unattended live trading. Unattended `AWAKENED` execution remains locked pending M1/M2/M3/M4
  certification (complete round-trip fee/funding net settlement, stream/restart recovery, native
  protective exchange orders, authenticated local control IPC, watchdog supervision, and signed
  release-key infrastructure — see `ROADMAP_V2.md`).

### v1.8.0 — Project Incarnation ("The Living Sovereign" — Articulated 3D WGPU PBR Engine)

All seven milestone rows reviewed:

- **M3 / M7 Articulated 3D Skeletal & Procedural Mesh WGPU PBR (`src/leviathan_3d_math.rs`,
  `src/leviathan_mesh.rs`, `src/leviathan_renderer_3d.rs`, `src/desktop_renderer.rs`):**
  Delivered a true 96-bone articulated 3D skeletal hierarchy and skinned 96-byte vertex mesh
  builder rendered via a 4-pass native WGPU pipeline (`1536 × 1536` `Depth32Float` 3×3 PCF
  directional shadows, Cook-Torrance GGX PBR opaque/translucent shading, overlapping obsidian
  diamond scales with amethyst sub-scale seams, 3D reptilian slit eyes, hinged jaws & ivory fangs,
  half-resolution separable HDR bloom, ACES compositing, `SceneQuality::Cinematic`, and optional
  `Features::TIMESTAMP_QUERY` live GPU pass timing) while keeping the Glow fallback intact.
- **M3 / M7 Camera Choreography & Analytical Workspace Charts (`src/leviathan_scene.rs`,
  `src/cockpit.rs`, `src/bin/hydra-cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`):**
  Added smooth Sovereign Overview ↔ heroic 3/4 Focused Head transitions (`Esc`, `--head <0..=6>`),
  clean bottom obsidian memorial plaque in close-ups, Overview readiness strip & equity hover
  crosshair, NEXUS lifecycle pipeline summary, and horizontal analytical bar plots in HUNTER,
  FANG, and AEGIS workspaces (**470** desktop-wgpu offline tests, **432** headless offline tests).
- **Explicit Live-Certification Boundary Unchanged:** Unattended `AWAKENED` live execution remains
  locked pending complete round-trip fee/funding net settlement, stream/restart recovery, native
  protective exchange orders, authenticated local control IPC, watchdog supervision, and signed
  release-signing key infrastructure.

### v1.9.0 — Project Synapse Phase I ("Spatial Consciousness")

All seven milestone rows reviewed against Project Synapse master directive:

- **M3 / M7 Spatial Consciousness Final Graphical Enhancement (`src/leviathan_spatial.rs`,
  `src/leviathan_awareness.rs`, `src/leviathan_3d_math.rs`, `src/leviathan_mesh.rs`,
  `src/leviathan_layout.rs`, `src/leviathan_scene.rs`, `src/cockpit.rs`, `src/bin/hydra-cockpit.rs`):**
  Completed floating Hybrid World-Space + Native `egui` holographic workspaces tethered by
  glowing conduits, 3D drag repositioning, interchangeable `SPATIAL | CLASSIC` modes with persistent
  preferences, 360° orbital camera + collision avoidance + outer cathedral perimeter + rear dorsal
  armor, seven personality-paced aware heads with dwell hysteresis, whole-body thoracic/wing/tail
  compensation, pairwise skull separation, and permanent fixed safety controls (**478** desktop-wgpu
  tests, **432** headless tests). Major graphics-first architecture work is now concluded.
- **M1–M6 Next Engineering Queue (Project Synapse Phase II onward):** Advance directly to **Phase II
  Connected Intelligence** (continuous HUNTER WebSocket streams, FANG shared live/replay/paper
  planner, COGNITION calibrated assessment, AEGIS independent admission, NEXUS paper execution, and
  versioned internal event architecture) followed by Phase III–V settlement, recovery, IPC, and
  watchdog certification. Live-trading defaults (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`) and
  Windows DPAPI vs. Unix `0700`/`0600` owner-only filesystem permission distinctions stay explicit.

### v1.10.0 — Project Synapse Phase II ("Connected Intelligence")

All seven milestone rows reviewed against Project Synapse Phase II:

- **M3 / M4 / M5 / M7 Connected Nervous System & `SIMULATION_ONLY` Paper Engine (`src/synapse_bus.rs`,
  `src/paper_pipeline.rs`, `src/cli/mod.rs`, `src/cockpit.rs`, `tests/synapse_pipeline.rs`):**
  Unified `HUNTER` -> `COGNITION` -> `FANG` (`plan_candidates` + `ema_anchor` + aligned trend
  structure) -> `AEGIS` (drawdown breaker + exposure caps + `-10%` margin stress) -> `NEXUS` Paper
  (`PaperLedger` checked-`Decimal` fills/exits, slippage/fees) across the schema-versioned
  `SynapseBus` (capacity 512, JSONL replay), exposed `hydra synapse-paper` offline CLI and live
  `Workspace::Overview` paper-simulation controls & real bus-driven 3D head pulses, and passed
  **485** desktop-wgpu tests / **439** headless tests with zero external crate additions.
- **Explicit Live-Certification Boundary Unchanged:** Paper execution is permanently bounded by
  `SIMULATION_ONLY = true`; unattended `AWAKENED` live execution remains locked pending complete
  round-trip fee/funding net settlement, stream/restart recovery, native protective exchange orders,
  authenticated local control IPC, watchdog supervision, and release-signing key infrastructure.

### v1.11.0 — Project Ascendancy Milestone 1 (Trustworthy Ingestion, Durable Paper & Execution Gate)

All seven milestone rows reviewed accurately against source & Project Ascendancy directive (Phase II
Connected Intelligence was only preliminary in `v1.10.0` and is hereby corrected, not overclaimed):

- **M1 / M2 / M3 Authoritative Execution Gate (`src/live_gate.rs`, `src/core/engine.rs`, `src/execution/mod.rs`, `src/adapters/hyperliquid.rs`, `src/state/mod.rs`, `src/treasury.rs`, `tests/live_gate_adversarial.rs`):**
  Unified all real-money entry points behind one fail-closed `PendingCertification` check and verified
  offline adversarial refusal across constructors, engine, adapter, flatten, CLI, and readiness checks.
- **M5 / M3 / M7 Genuine HUNTER Streaming, Durable `PaperStore` & Independent `PaperService` (`src/hunter_stream.rs`, `src/paper_store.rs`, `src/paper_service.rs`, `src/synapse_bus.rs` v2, `src/paper_pipeline.rs`, `src/cli/mod.rs`, `src/cockpit.rs`):**
  Replaced GUI-owned fake-metric polling with real WebSocket `l2Book`/`trades`/`candle`/`activeAssetCtx` +
  REST fallback ingestion (`watchlist ∪ held_positions`), eliminated fabricated depth/spread/directionality/
  coverage/age placeholders, added checked missing-mark halts & post-fill exposure validation, created
  isolated SQLite `paper_sessions.db` + OS-locked & authenticated-IPC detached `paper-service`, and passed
  **496** desktop-wgpu / **450** headless tests (0-vuln `cargo audit`).
- **Remaining Ascendancy Certification & Unification Work:** Cross-consumer FANG strategy parity
  (`Engine`/`ORACLE`/`Paper`), COGNITION statistical calibration replacing heuristics, NEXUS complete
  fee/funding net settlement & WebSocket/REST recovery, ORACLE realistic bid/ask/partial/latency
  simulation & walk-forward, GENESIS NSGA-II integration, authenticated live-engine control IPC,
  and multi-week paper qualification campaigns remain open before any limited live pilot.

### v1.12.0 — Project Ascendancy P0 Operational Correction (Operational Reliability & Transactional Truth)

All seven milestone rows reviewed against `v1.12.0`:

- **M3 / M7 Self-Contained Standalone Worker, Transactional Loop & Watchdog (`src/bin/hydra-cockpit.rs`, `src/paper_service.rs`, `src/paper_store.rs`, `src/cockpit.rs`):**
  Eliminated sibling `hydra.exe` dependency via embedded `--paper-service-worker` mode; staged pipeline
  cycles before atomic SQLite commits; enforced explicit `Faulted` status transitions and persisted
  `fault_reason`; persisted compact market observations; bounded pre-allocation IPC reads
  (`Read::take`); added stream-exit watchdog restarts; added lossless checkpoint & `SynapseBus` event-ID
  restoration; enforced lock/paused ownership guards (`archive_session_guarded`,
  `reset_session_confirmed_guarded`); and passed **504** desktop-wgpu / **458** headless tests.
- **M1 / M2 / M5 Structural Transport Isolation, Independent Freshness & Hourly Funding (`src/live_gate.rs`, `src/adapters/hyperliquid.rs`, `src/hunter_stream.rs`, `src/hunter.rs`, `src/paper_pipeline.rs`):**
  Added sealed `VenueTransportClass`, per-field freshness validation (`FieldFreshness` /
  `MarketFreshnessReport`), stale-mark exclusion BEFORE both simulated exits and entries, exact
  `observed_history_minutes` with `instrument_listing_age_days: None`, and non-double-charged hourly
  `apply_simulated_funding`. Unattended `AWAKENED` live certification remains locked (`PendingCertification`);
  P1 intelligence unification and P2 live-pilot preparation follow on `main`.

### v1.13.0–v1.15.0 — The Sovereign Trilogy (`PROJECT OMNISCIENCE`, `PROJECT IMMORTALITY`, `PROJECT SOVEREIGNTY`)

All seven milestone rows reviewed against Sovereign Trilogy verification and [`READINESS_V2.md`](READINESS_V2.md):

- **v1.13.0 (`PROJECT OMNISCIENCE`)**: Causal HUNTER microstructure & correlation (`hunter_microstructure`), COGNITION Bayesian/Brier/Page-Hinkley stats (`cognition_stats`), unified 7-strategy `FangUnifiedPlanner` with bit-for-bit cross-consumer parity (`fang_parity`), ORACLE multi-market portfolio simulator & purged walk-forward + circular block bootstrap CI (`oracle_research`), and GENESIS constraint-aware NSGA-II (`genesis_nsga2`).
- **v1.14.0 (`PROJECT IMMORTALITY`)**: Exact `Decimal` FIFO settlement with entry/exit fees, maker rebates, hourly funding attribution, deposit/withdrawal separation & at-most-once learning (`settlement.rs`), REST/WS execution recovery (`execution_recovery.rs`), `reduce_only` protective stops authorized during blocked entries (`protective_orders.rs`), fail-closed AEGIS sovereign risk (`aegis_sovereign.rs`), `SigningAccessScope` Treasury isolation (`treasury.rs`), authenticated main-engine IPC (`engine_ipc.rs`), and `tests/immortality_suite.rs`.
- **v1.15.0 (`PROJECT SOVEREIGNTY` & Section 26 `v2.0.0` Hold)**: External OS-locked watchdog (`src/watchdog.rs`), 10-scenario fault-injection campaign (10/10 pass), honest real-clock qualification reports, scientific strategy acceptance gating (`EligibleForMonitoredPilot` vs. `RejectedKeepDisabled`), priority `ResourceBudgetScheduler` (`src/qualification.rs`), all 7 `HeadId`s connected on `SynapseBus`, 6-pillar `PendingCertification` (`src/live_gate.rs`), and `READINESS_V2.md`. All **530** desktop-wgpu / **484** headless offline tests pass; `v2.0.0` stays intentionally **HELD**.
- **v1.16.0 (`PROJECT CONVERGENCE — UNIFIED EXECUTION & FINANCIAL TRUTH`)**: Converged `src/core/engine.rs` onto `FangUnifiedPlanner` + `evaluate_sovereign_admission` + `BoundedGrid` `research_only` exclusion; fixed Windows `WSAECONNRESET (10054)` IPC resets (`set_nonblocking(false)` + flush/shutdown + `EngineControlAuthority`); added `EngineSignerAuthority` (`src/treasury.rs`), durable SQLite `SettlementStore` + `PendingFundingObligation` (`src/settlement.rs`), `VenueTransportClass::TestnetSandbox` + `TriggerOrderAction::reduce_only_stop` / `CancelAction::by_oid` + `HyperliquidAdapter::new_testnet`, full ORACLE/GENESIS fingerprinting + SQLite `research_events`, and `tests/convergence_e2e.rs`. Verified across CI runs `46092459099` & `46092462506` and standalone Windows EXE SHA-256 `6dbf609357f38e5461a0237356d6239ae48cb2e76cbdbc2cd086af1d78875659`.
- **v1.17.0 (`PROJECT SENTINEL — AUTONOMOUS RESILIENCE & OPERATIONAL PROOF`)**: Added persisted `watchdog-state.json` restart budgets, non-blocking `supervise_live_worker` hang-kill + real backoff sleep, SQLite `inspect_post_restart_evidence`, `measure_host_resources` (`hydra profile-host`), `ResourceBudgetScheduler` slot reservation in `genesis_nsga2`, SHA3-256 version-bound `EvidenceRegistry` (`src/live_gate.rs`), `hydra testnet-probe`, rolling OOS `strategy-accept` (`simulate_portfolio_with_bar_returns`), and `tests/sentinel_faults.rs` (3/3 x 3 stress passes). Verified across CI runs `46096037097` & `46096040198`, standalone Windows EXE SHA-256 `566f29d00786ea9f1dfd433cd64706d3d5cbfe71681d3562f43e5a4dfd51c3ea`, and live `38.63s` public-market WS/L2 `public_paper-9370231f3128` lifecycle (`5` cycles, `quick_check: ok`).
- **v1.18.0 (`PROJECT AWAKENING — RESTRICTED MONITORED PILOT CANDIDATE`)**: Delivered Section 15/16 Restricted Monitored Live-Pilot mode (`LiveExecutionTier`, `MonitoredPilotLimits`, `MonitoredPilotGrant`, `evaluate_monitored_pilot_eligibility`, `authorize_monitored_pilot_session`), `Engine::step_once_at` ceiling & supervision-lease enforcement (zero blind liquidation, zero restart auto-resume), IPC `RenewPilotSupervision`, `hydra pilot-readiness --json`, and `tests/monitored_pilot_suite.rs` (2/2 x 3 stress passes). All **541** desktop-wgpu / **495** headless tests pass (`0` clippy warnings across new/modified files); `v2.0.0` remains cleanly held in `DORMANT` strictly on external strategy qualification, owner Testnet order/stop proof, and owner pilot consent.
- **v1.19.0 (`PROJECT EQUILIBRIUM — AUTHORITATIVE RUNTIME, LABORATORY & OPERATIONAL VALIDATION`)**: Eliminated synthetic/fallback runtime inputs across `HyperliquidAdapter` (real L2 top-10 depth, `metaAndAssetCtxs` funding, cached `maxLeverage`, `redirect::Policy::none()`) & `hunter_stream::FieldFreshness` (`MAX_CLOCK_SKEW_MS = 5_000` exchange clock validation); migrated live signing strictly into zero-plaintext `EngineSignerAuthority::sign_digest`; wired `Engine::new_with_venues` (`SettlementStore`, `EngineIpcServer`, `instrument_constraints`, HUNTER microstructure, COGNITION Bayesian weights, MTM exposure, real in-flight `reserved_notional_usd`, rolling correlation matrix, separate `session_pnl_usd`, per-candidate pre-submission revalidation, fail-closed AEGIS & protective routing); preserved `PendingPaperFundingObligation` across close/checkpoint; enforced strict `Url::parse`, non-`Deserialize` `MonitoredPilotGrant` with `session_id`, session-scoped `RenewPilotSupervision { session_id }`, producer-attested expiring/revocable `EvidenceRecord`, and scientific NaN/Inf/zero-trade rejection; added self-contained `--watchdog-supervisor` (startup warmup grace + clean code-0 exit handling), live Cockpit IPC controls, read-only `TESTNET SANDBOX` probe UI, cancellable ORACLE/GENESIS NSGA-II Laboratory workflows, 60-day historical `BTC/ETH/SOL` validation (`rejected_keep_disabled`), standalone live WS/L2 crash-recovery proof (`19656 -> 40764`), and `tests/equilibrium_e2e.rs` (2/2). All **545** desktop-wgpu (`41` suites) / **499** headless (`40` suites) tests pass (`0` clippy warnings in modified files); `v2.0.0` remains cleanly held in `DORMANT`.

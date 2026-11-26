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
| **M1 execution foundation** — preserve strategies; authorization; durable IDs; typed receipts; confirmed reductions; no invented PnL; recoverable executables | Durable intent/receipt boundary, dry/live gate, exact residual proofs, original entry context, unresolved holds, Windows/Linux prereleases | execution_safety, execution_facts, position_evidence, position_context, atomic_admission; tagged CI through alpha.20 | Broader startup/stream recovery and supported-mode operational certification | Venue/account coverage and true financial settlement; no live development testing | Extra forensic formats / full signed-envelope attestation | Preserve established safety boundary while other milestones advance |
| **M2 authoritative ledger** — scoped identities; exact arithmetic; fill/fee/funding ledger; atomic outcomes; account ownership; secure signing | Decimal facts/deduplication, native acknowledgment coverage, scoped evidence, nonce coordination, persistent risk and account locks | ledger_hyperliquid, ledger_integration, acknowledged_ledger, signer_nonce, account_ownership, signing_reference | Complete net settlement and history coverage; stream races; exactly-once final learning outcome; secure credentials; expanded margin models | Blocks stable unattended live operation and verified-outcome learning, **not offline research/UI** | More archival inspection variants; cross-host scope beyond current support | Financial coverage/convention evidence and credential-storage design after a balanced feature cycle |
| **M3 independent engine / desktop** — native Windows; headless/TUI; genuine telemetry; reconnect; secure IPC; responsive rendering | eframe/glow cockpit, background read-only SQLite snapshots, telemetry CLI, independent engine lifetime | Native executable smoke/captures; telemetry_cli; tui_smoke; read-only history tests | Authenticated control IPC, persistent layouts, richer portfolio/configuration workspaces, DPI/resize/fallback/performance qualification | Writable controls require authenticated ownership/authorization; **read-only workspaces do not** | Installer and optional renderer/effects | Research validation interfaces; secure IPC design |
| **M4 shared planning / risk** — shared deterministic planner; Passivbot kernels; bounded grids/scaling; arbitration; independent Aegis | Pure offline arbitration/basic EMA Anchor; offline portfolio analysis; existing divergence/momentum/mean-reversion strategies; persistent Aegis reservations/drawdown; original-R context | strategy/indicator tests; aegis_integration; reservation_reconciliation; atomic_admission | Live/replay planner integration; adaptive EMA Anchor; grids/trailing state; net-position ownership/arbitration; protective exchange orders; liquidation/correlation models | Planner/ownership before live multi-strategy scaling; budget constraints before any grid | Optional strategy variants and acceleration | Integrate pure planning with live/replay consumers under unchanged risk gates; verify fixture parity |
| **M5 discovery / cognition** — approved markets; liquidity/cost ranking; hysteresis; sample/recency confidence; attribution; shadow policies | Offline approved-market eligibility/ranking/hysteresis; strict prior-outcome shadow statistics and strategy/regime attribution; configured-symbol observations; legacy adaptive/regime components; durable context | 8 hunter core tests, hunter_cli isolation; learning_integration and existing indicator/regime tests; telemetry observations | Autonomous Hunter acquisition/live rotation; portfolio-aware discovery; calibrated live confidence and inferred regimes; verified-outcome adaptation | Approved metadata for live selection; verified outcomes for new live learning; **offline ranking/shadow work independent** | Bandits/neural methods without measured justification | Inferred/calibrated regime evaluation and portfolio-aware discovery |
| **M6 Oracle / Genesis** — historical integrity/provenance; realistic costs; shared parity; walk-forward/OOS; resumable multiobjective search | Deterministic next-bar Oracle, fixed fees/slippage and conservative stops; seeded CPU Genesis/Pareto search with generation checkpoint/resume; fixed-parameter held-out cost scenarios | oracle_integration, oracle_validation, genesis_integration, checkpoint_tests, genesis_checkpoint_cli, genesis_oos, genesis_oos_cli, research_cli; native research report rendering | Train/select/test walk-forward and independently sourced OOS certification, dataset manifests, funding/partial-fill/latency/portfolio models, NSGA-II | Shared planner for parity; trustworthy data for broader models; **walk-forward and checkpoints independent now** | GPU until CPU parity and benchmark justification | Visual campaign: measured optimization progress and integrated research views |
| **M7 visual workspaces / distribution** — all seven functional workspaces; meaningful analytical geometry; true event animation; onboarding; original assets; packaging | Original SVG identity, seven-head topology, event-backed refresh pulses, Hunter Radar and liquidity map, FANG/COGNITION/AEGIS offline panels, Oracle/Genesis plots, searchable records, portable artifacts/checksums | Actual native captures; GUI smoke; dark/light README and asset checks; four CI jobs; package smoke | Richer charts/risk stress heatmap, settings/security, layout persistence, authenticated management, measured rendering | Genuine input services per workspace; authenticated IPC for commands; no fake activity or results | Decorative effects, installer/code signing until foundation qualifies | Research scenario/OOS panels, then risk/shadow panels |

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
with post-publish public download & SHA-256 checks, top-of-README download redesign, and the
Permanent Definition of Done in `AGENTS.md`. Preserves all financial safeguards and Ashlan
Haley Rapponotti's permanent memorial. Next: **LEVIATHAN EVOLUTION** secure native updater
(`v1.7.0-alpha.28`) and continued progression across all seven major milestones.

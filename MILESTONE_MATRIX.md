# Project Leviathan milestone completion matrix

This is the current acceptance ledger, not a release counter. Reviewed against the
owner's original directive (sections 7–28 and strategic order in section 30), the
seven major milestones in IMPLEMENTATION_PLAN.md, actual source and tests through
alpha.23. **No original major milestone is fully certified complete.** Alpha.11–20
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
| **M4 shared planning / risk** — shared deterministic planner; Passivbot kernels; bounded grids/scaling; arbitration; independent Aegis | Pure offline arbitration/basic EMA Anchor; offline portfolio analysis; existing divergence/momentum/mean-reversion strategies; persistent Aegis reservations/drawdown; original-R context | strategy/indicator tests; aegis_integration; reservation_reconciliation; atomic_admission | Live/replay planner integration; adaptive EMA Anchor; grids/trailing state; net-position ownership/arbitration; protective exchange orders; liquidation/correlation models | Planner/ownership before live multi-strategy scaling; budget constraints before any grid | Optional strategy variants and acceleration | Extract pure planning/ownership contracts and offline fixture parity; no automatic deployment |
| **M5 discovery / cognition** — approved markets; liquidity/cost ranking; hysteresis; sample/recency confidence; attribution; shadow policies | Offline approved-market eligibility/ranking/hysteresis; strict prior-outcome shadow statistics and strategy/regime attribution; configured-symbol observations; legacy adaptive/regime components; durable context | 8 hunter core tests, hunter_cli isolation; learning_integration and existing indicator/regime tests; telemetry observations | Autonomous Hunter acquisition/live rotation; portfolio-aware discovery; calibrated live confidence and inferred regimes; verified-outcome adaptation | Approved metadata for live selection; verified outcomes for new live learning; **offline ranking/shadow work independent** | Bandits/neural methods without measured justification | Confidence-aware shadow evaluation and portfolio-aware discovery |
| **M6 Oracle / Genesis** — historical integrity/provenance; realistic costs; shared parity; walk-forward/OOS; resumable multiobjective search | Deterministic next-bar Oracle, fixed fees/slippage and conservative stops; seeded CPU Genesis/Pareto search; fixed-parameter held-out cost scenarios | oracle_integration, 6 oracle_validation unit tests, genesis_integration, research_cli; native research report rendering | Train/select/test walk-forward and candidate OOS, dataset manifests, funding/partial-fill/latency/portfolio models, checkpoints/resume, NSGA-II | Shared planner for parity; trustworthy data for broader models; **walk-forward and checkpoints independent now** | GPU until CPU parity and benchmark justification | Nonoverlapping OOS/scenario evaluation and GUI, then resumable optimizer state |
| **M7 visual workspaces / distribution** — all seven functional workspaces; meaningful analytical geometry; true event animation; onboarding; original assets; packaging | Original SVG identity, seven-head topology, event-backed refresh pulses, Hunter Radar and liquidity map, FANG/COGNITION/AEGIS offline panels, Oracle/Genesis plots, searchable records, portable artifacts/checksums | Actual native captures; GUI smoke; dark/light README and asset checks; four CI jobs; package smoke | Richer charts/risk stress heatmap, settings/security, layout persistence, authenticated management, measured rendering | Genuine input services per workspace; authenticated IPC for commands; no fake activity or results | Decorative effects, installer/code signing until foundation qualifies | Research scenario/OOS panels, then risk/shadow panels |

## Head-by-head progression queue

| Head | Current boundary | Independent next delivery | Later / blocked live acceptance |
|---|---|---|---|
| HUNTER | Offline eligibility/ranking/hysteresis and native Radar | Point-in-time data acquisition and portfolio-aware research selection | Autonomous point-in-time acquisition, correlations/capital allocation; live routing through planner and Aegis |
| FANG | Existing independent signal strategies | Pure shared planner, EMA Anchor research kernel, conflict explanations | State-owned bounded live grids/scaling after ownership/recovery acceptance |
| COGNITION | Legacy expectancy/similarity/regime and entry context | Sample/recency confidence, strategy attribution, shadow-policy report and visualization | New live adaptation only from verified net outcomes; never override Aegis |
| AEGIS | Persistent hard limits, reservations and narrow reconciliation | Read-only portfolio concentration/correlation analysis with explicit unknowns | Native protective orders, liquidation/margin/funding-aware gates and operational qualification |
| NEXUS | Extensive conservative execution evidence | Prioritize complete settlement/recovery/credentials over additional report refinements | Unattended live certification remains blocked; no hold bypass |
| ORACLE | Fixed-cost deterministic single-instrument replay | Bounded cost scenarios and disjoint rolling OOS validation; UI comparison | Shared parity, funding/queue/partial/latency/portfolio models and reliable datasets |
| GENESIS | Seeded bounded CPU search and Pareto reports | Checkpoint/resume, OOS candidate assessment and generation visualization | NSGA-II, parallel scheduling and optional GPU only after CPU correctness/parity |

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

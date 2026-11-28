# HYDRA v2.0.0 Live-Trading Readiness Matrix (Section 26)

> **Evidence-First Verdict (`v1.16.0 | PROJECT CONVERGENCE — UNIFIED EXECUTION & FINANCIAL TRUTH`):**
> **Unified Convergence engineering & Section 26 foundation are implemented and offline/loopback verified (**535** `desktop-wgpu` / **489** headless tests, `0` clippy warnings across Convergence modules, 3x Windows `engine_ipc` stress pass).** Per **Accelerated Awakening**, mandatory `72h` and `14–30` day fixed calendar waits are removed for an initial **Restricted Actively Monitored Live Pilot** (`MODE: DORMANT` by default) and replaced with measurable evidence-based qualification in **`v1.17.0 | PROJECT SENTINEL`** (real external watchdog, resource profiler, historical OOS validation, live public-market paper lifecycle, and Hyperliquid Testnet functional exchange proof where keys/permissions allow), alongside owner-provisioned `PINNED_RELEASE_SIGNING_PUBKEY_HEX` in `src/evolution.rs`. Unattended `AWAKENED` trading remains separately gated.

---

## 1. Authoritative Readiness Evidence Matrix

| Section 26 Requirement | Module & Implementation Evidence | Offline / Loopback Verification | Pilot Readiness | Unattended Readiness |
| :--- | :--- | :--- | :--- | :--- |
| **Authoritative financial settlement** | `src/settlement.rs`, `src/ledger.rs` (`SettlementEngine`, FIFO partial fills, fees/maker rebates, funding coverage windows, external cash flow separation, at-most-once `VerifiedLearningOutcome`) | `settlement` (3 tests), `ledger_integration`, `immortality_suite`, `hydra settlement-audit` | Offline Verified | Requires live small-fill reconciliation |
| **Supported-account reconciliation** | `src/account.rs`, `src/ledger_hyperliquid.rs`, `src/execution_recovery.rs` | Fails closed on unified/portfolio margin modes; REST/WS delta reconciliation tested | Offline Verified | Verified during pilot soak |
| **Deterministic shared strategy planning** | `src/fang_engine.rs`, `src/fang_planner.rs`, `src/oracle.rs`, `src/core/engine.rs`, `src/paper_pipeline.rs` | `fang_engine` (2 tests), `tests/fang_parity.rs` bit-for-bit across all 4 consumers | Ready (Simulation Verified) | Verified after pilot |
| **Independent portfolio risk enforcement** | `src/aegis_sovereign.rs`, `src/aegis.rs`, `src/risk/mod.rs` (fail-closed missing inputs, post-fee exposure, correlated clusters, liq buffer, daily/weekly loss breakers) | `aegis_sovereign`, `aegis_integration`, `atomic_admission`, `immortality_suite` | Ready | Ready |
| **Native protective orders** | `src/protective_orders.rs` (`reduce_only: true` exchange stops across partial fills, orphans & blocked entries) | `protective_orders`, `immortality_suite` | Offline Verified | Verified on live exchange stop ack |
| **Secure signing authority** | `src/treasury.rs` (`WindowsDpapiCurrentUser`, `SigningAccessScope::EngineExclusive`, zeroizing `SecretString`, key rotation & revocation) | `treasury` (6 tests), `immortality_suite` | Ready (Windows DPAPI) | Blocked on Unix unless encrypted vault added |
| **Authenticated live-engine control** | `src/engine_ipc.rs` (256-bit constant-time token, monotonic nonce replay guard, timestamp freshness, 16 KiB frame cap) | `engine_ipc`, `immortality_suite` | Ready | Ready |
| **Durable execution recovery** | `src/execution_recovery.rs`, `src/paper_service.rs`, `src/paper_store.rs` (`OrderLifecycleState`, `RefuseBlindRetry`, `PaperSessionLock`) | `execution_recovery` (2 tests), `paper_store`, `paper_service` | Offline Verified | Verified during pilot |
| **External watchdog supervision** | `src/watchdog.rs` (`WatchdogSupervisor`, `watchdog.lock`, `PRAGMA quick_check(1)`, Win32/POSIX disk budget, restart escalation, `live_auto_resume_prohibited`) | `watchdog`, `sovereignty_suite`, `hydra watchdog-check` | Ready | Ready |
| **10-Scenario fault injection & resource isolation** | `src/qualification.rs` (`run_fault_injection_campaign`, `ResourceBudgetScheduler`) | 10/10 scenarios pass (`qualification`, `sovereignty_suite`, `hydra qualify-campaign`) | Verified | Verified |
| **Extended paper qualification (Multi-week public soak)** | `src/paper_service.rs`, `src/qualification.rs` (infrastructure 100% live via `HYDRA-Leviathan-Windows-x64.exe --paper-service-worker`) | Honest real-clock soak tracking (`actual_elapsed_seconds`); single-session synthetic smoke only during build | **Pending Real Elapsed Time** | **Blocked Pending Multi-Week Public Data** |
| **Independently validated deployable strategies** | `src/oracle_research.rs`, `src/genesis_nsga2.rs`, `src/cognition_stats.rs`, `src/qualification.rs` (`evaluate_scientific_strategy_acceptance`, `hydra strategy-accept`) | Purged CV, block bootstrap, NSGA-II determinism, conservative `RejectedKeepDisabled` verified | Candidate Screening Ready | Requires positive multi-month OOS dataset evidence |
| **Operational diagnostics & 7-head Synapses** | `src/synapse_bus.rs` v2, `src/cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`, `src/research_ui.rs`, CLI inspectors | All 7 heads emit typed events; isolated standalone Windows exe verified | Ready | Ready |
| **Explicit owner activation & signed updates** | `src/live_gate.rs` (`PendingCertification`), `src/evolution.rs` (`PINNED_RELEASE_SIGNING_PUBKEY_HEX`) | `live_gate`, `live_gate_adversarial` (2 tests) | Blocked by External Signing Key Setup | Blocked until Owner Pinned Key |

---

## 2. Monitored Limited-Capital Live Pilot vs. Unattended Live Operation

| Criterion | Restricted Actively Monitored Live Pilot (`v2.0.0` Candidate) | Unattended `AWAKENED` Production Operation |
| :--- | :--- | :--- |
| **Prerequisite 1: Live Service & Watchdog** | **Project Sentinel (`v1.17.0`):** Real watchdog supervising live worker heartbeat/hang/restart + functional public-market paper lifecycle on standalone EXE (no fixed calendar wait) | Sustained real monitored pilot track record across varying market & volatility regimes with zero unresolved faults |
| **Prerequisite 2: Strategy Acceptance** | Genuine historical dataset passes `hydra strategy-accept` (`EligibleForMonitoredPilot`) with finite OOS & downside bounds | Sustained positive post-cost expectancy in both purged OOS research and live pilot fills |
| **Prerequisite 3: Exchange & Protective Orders** | **Hyperliquid Testnet** functional proof of account signing, order ack/cancel, and native `reduce_only` protective stop placement + reconciliation | Audited small-capital monitored pilot live-fill & native stop reconciliation |
| **Prerequisite 4: Owner Limits & Emergency Controls** | Explicit owner-defined capital/leverage/loss/drawdown ceilings, dedicated small-balance wallet in Windows DPAPI `EngineSignerAuthority`, active monitoring & `EmergencySuspend` | Owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX`, external OS watchdog service running, zero unresolved holds |

---

## 3. Owner Action Required to Enable Signed Native Evolution Auto-Apply (`src/evolution.rs`)

1. Generate an offline dedicated `secp256k1` signing keypair (`privkey` stored exclusively in hardware/GitHub encrypted Actions secret; **never** commit private keys to git).
2. Extract the 33-byte compressed public key hex and set `pub const PINNED_RELEASE_SIGNING_PUBKEY_HEX: Option<&str> = Some("<66-hex-chars>");` in `src/evolution.rs`.
3. Sign release manifests in CI or offline (`hydra-release-manifest.json` SHA-256) and upload the detached signature alongside `SHA256SUMS.txt` on GitHub Releases.
4. Until steps 1–3 are completed by the repository owner, **LEVIATHAN EVOLUTION** intentionally keeps automatic installation **locked**, verifying downloads via FIPS 180-4 SHA-256 and flagging provenance as unpinned.

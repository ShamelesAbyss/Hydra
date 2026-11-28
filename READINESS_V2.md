# HYDRA v2.0.0 Live-Trading Readiness Matrix (Section 26)

> **Evidence-First Verdict (`v1.18.0 | PROJECT AWAKENING — RESTRICTED MONITORED PILOT CANDIDATE`):**
> **All Sovereign Trilogy (`v1.13.0–v1.15.0`), Convergence (`v1.16.0`), Sentinel (`v1.17.0`), and Section 15/16 Restricted Monitored Live-Pilot architecture (`v1.18.0`, `MonitoredPilotLimits`, `MonitoredPilotGrant`, `Engine::step_once_at` ceiling & supervision-lease enforcement, IPC `RenewPilotSupervision`, `hydra pilot-readiness`) pass **541** `desktop-wgpu` / **495** headless tests (`0` clippy warnings across new/modified modules).** Per Section 16 (*"Never declare HYDRA 2.0 ready merely to satisfy a release schedule... Do not call a locked candidate live-ready"*), **`v2.0.0` remains cleanly HELD in `MODE: DORMANT` / `PendingCertification`** strictly on real external blockers: (1) default strategy config returned **`rejected_keep_disabled`** on 45-day public `BTC/ETH/SOL` historical OOS (`-2.80%` net / `-0.51%` mean OOS fold return, requiring a qualified strategy passing `hydra strategy-accept`), (2) authenticated **Hyperliquid Testnet** order/stop execution requires an isolated owner-authorized Testnet identity + valueless faucet funds (`hydra testnet-probe` verified 213-asset reachability), (3) explicit owner `MonitoredPilotLimits` + DPAPI wallet authorization + consent statement, and (4) owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX` in `src/evolution.rs` for signed auto-updates. Unattended `AWAKENED` trading remains separately gated (`unattended_awakened_eligible: false`).

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
| **External watchdog supervision** | `src/watchdog.rs` (`WatchdogSupervisor`, `watchdog.lock`, atomic `watchdog-state.json`, `supervise_live_worker` hang/backoff termination, `PRAGMA quick_check(1)`, SQLite `inspect_post_restart_evidence`, `measure_host_resources`) | `watchdog` (2 tests), `sovereignty_suite`, `sentinel_faults`, `hydra watchdog-check`, `hydra profile-host` | Verified (Live Process + Host Profile) | Ready |
| **Intensive fault injection & resource isolation** | `src/qualification.rs`, `src/genesis_nsga2.rs`, `tests/sentinel_faults.rs` (`ResourceBudgetScheduler` slot reservation for `ProtectiveCritical`, `EvidenceProvenanceLabel`) | 10/10 scenarios pass + `sentinel_faults` (3/3 x 3 stress passes) | Verified | Verified |
| **Accelerated public paper lifecycle** | `src/paper_service.rs`, `src/qualification.rs` (standalone `HYDRA-Leviathan-Windows-x64.exe --paper-service-worker` without sibling `hydra.exe`) | Verified on `v1.17.0` standalone EXE: `38.63s` live Hyperliquid WS/L2 (`public_paper-9370231f3128`, `5` cycles, `pause`/`resume`/`stop`, `quick_check: ok`) | Verified (Standalone Live WS/L2 Functional Lifecycle) | Requires Sustained Multi-Regime Pilot History |
| **Independently validated deployable strategies** | `src/oracle_research.rs`, `src/genesis_nsga2.rs`, `src/cognition_stats.rs`, `src/qualification.rs` (`evaluate_scientific_strategy_acceptance`, `hydra strategy-accept`) | Tested on 45-day public `BTC/ETH/SOL` `1h` bars (`1,081`/sym, `dataset_sha3_256` `7ff196eaadefb109ac24ff72e5e476615897ca4ff99d907e485368d50a7a31b8`): **`rejected_keep_disabled`** (`-2.80%` net, `-0.51%` OOS mean) | **Blocked Pending Positive Candidate** | Requires Positive Multi-Month OOS Evidence |
| **Operational diagnostics & 7-head Synapses** | `src/synapse_bus.rs` v2, `src/cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`, `src/research_ui.rs`, CLI inspectors | All 7 heads emit typed events; isolated standalone Windows exe verified | Ready | Ready |
| **Restricted Monitored Pilot gating, Testnet proof & signed updates** | `src/live_gate.rs` (`LiveExecutionTier`, `MonitoredPilotLimits`, `MonitoredPilotGrant`, `EvidenceRegistry`), `src/core/engine.rs` (`step_once_at`), `src/engine_ipc.rs` (`RenewPilotSupervision`), `src/evolution.rs` | `monitored_pilot_suite` (2/2 x 3), `live_gate`, `hydra pilot-readiness --json`, `hydra testnet-probe` (`213` Testnet assets reachable) | **Blocked Pending Owner Testnet Key + Pilot Consent/Ceilings** | Blocked (`unattended_awakened_eligible: false`) |

---

## 2. Monitored Limited-Capital Live Pilot vs. Unattended Live Operation

| Criterion | Restricted Actively Monitored Live Pilot (`v2.0.0` Candidate) | Unattended `AWAKENED` Production Operation |
| :--- | :--- | :--- |
| **Prerequisite 1: Live Service & Watchdog** | **Project Sentinel (`v1.17.0`) Verified:** Live worker hang-kill/backoff watchdog, persisted restart budget, Legion RTX 4070 resource profile, and standalone public-market paper lifecycle (no fixed calendar wait) | Sustained real monitored pilot track record across varying market & volatility regimes with zero unresolved faults |
| **Prerequisite 2: Strategy Acceptance** | Genuine historical dataset passes `hydra strategy-accept` (`EligibleForMonitoredPilot`) — currently **`rejected_keep_disabled`** on default config (`-2.80%` net) | Sustained positive post-cost expectancy in both purged OOS research and live pilot fills |
| **Prerequisite 3: Exchange & Protective Orders** | **Hyperliquid Testnet** reachable (`213` assets via `hydra testnet-probe`); owner must run authenticated Testnet signing, order ack/cancel & `reduce_only` stop proof using an isolated Testnet wallet | Audited small-capital monitored pilot live-fill & native stop reconciliation |
| **Prerequisite 4: Owner Limits & Emergency Controls** | Explicit owner-defined capital/leverage/loss/drawdown/duration ceilings, dedicated small-balance wallet in Windows DPAPI `EngineSignerAuthority`, active monitoring & `EmergencySuspend` | Owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX`, external OS watchdog service running, zero unresolved holds |

---

## 3. Owner Actions Required for Testnet Functional Proof & Signed Auto-Updates

### A. Owner Action for Isolated Hyperliquid Testnet Order & Protective Stop Proof
1. Generate or import a **brand-new isolated Testnet-only secp256k1 keypair** (never reuse any mainnet key or seed phrase) and fund it only with valueless Testnet USDC from the official Hyperliquid Testnet faucet.
2. Verify read-only connectivity first with `hydra testnet-probe --json`.
3. Configure Hydra against `https://api.hyperliquid-testnet.xyz` (`VenueTransportClass::TestnetSandbox`), execute one bounded Testnet agent entry + native `TriggerOrderAction::reduce_only_stop` + `CancelAction::by_oid` cancellation cycle, and verify startup/reconnect recovery and zero residual open orders.

### B. Owner Action to Enable Signed Native Evolution Auto-Apply (`src/evolution.rs`)

1. Generate an offline dedicated `secp256k1` signing keypair (`privkey` stored exclusively in hardware/GitHub encrypted Actions secret; **never** commit private keys to git).
2. Extract the 33-byte compressed public key hex and set `pub const PINNED_RELEASE_SIGNING_PUBKEY_HEX: Option<&str> = Some("<66-hex-chars>");` in `src/evolution.rs`.
3. Sign release manifests in CI or offline (`hydra-release-manifest.json` SHA-256) and upload the detached signature alongside `SHA256SUMS.txt` on GitHub Releases.
4. Until steps 1–3 are completed by the repository owner, **LEVIATHAN EVOLUTION** intentionally keeps automatic installation **locked**, verifying downloads via FIPS 180-4 SHA-256 and flagging provenance as unpinned.

# HYDRA v2.0.0 Live-Trading Readiness Matrix (Section 26)

> **Evidence-First Verdict (`v2.0.1 | THE AWAKENING — RESTRICTED MONITORED MAINNET PILOT (LOCKED QUALIFICATION BUILD)`):**
> **All `A01`–`A08` architectural acceptance gates pass **552** `desktop-wgpu` / **506** headless offline tests on Windows and **506** headless tests natively on Linux (fixing a Linux test-only poll race from unpublished `v2.0.0`), plus standalone `HYDRA-Leviathan-Windows-x64.exe` (`28,314,112 B`) GUI, engine watchdog crash-recovery, and live public WS/L2 paper drills, while `v2.0.1` explicitly ships as a `LOCKED QUALIFICATION BUILD` in `MODE: DORMANT` (`PendingCertification`).** Live entries remain structurally locked on real external facts: (1) Strategy Laboratory (`A09`: `BTC/ETH/SOL` 1h, 5,001 bars, untouched 1,477-bar holdout `dataset_sha3_256` `227860483a28991fe5c19a6846797a1e41d6574a55c29e2e77731c8c2cab734a`) produced **`NO_QUALIFIED_STRATEGY`** (`nsga2-f4-s7-o14-sl350-tp621`: `-9.27%` net holdout, `12.40%` maxDD, `77` trades, `-0.83%` 3-fold WF mean OOS, `rejected_keep_disabled`), (2) owner has not yet run authenticated **Hyperliquid Testnet** execution (`hydra testnet-qualify --execute-confirmed`) on a disposable faucet wallet, (3) no owner `MonitoredPilotGrant` / hard ceilings / DPAPI consent has been issued, (4) unattended `AWAKENED` stays `PendingCertification` (`unattended_awakened_eligible: false`), and (5) SHA-256 release checksums do **not** establish publisher authentication (`PINNED_RELEASE_SIGNING_PUBKEY_HEX` in `src/evolution.rs` remains unset).

---

## 1. Authoritative Readiness Evidence Matrix

| Section 26 Requirement | Module & Implementation Evidence | Offline / Loopback Verification | Pilot Readiness | Unattended Readiness |
| :--- | :--- | :--- | :--- | :--- |
| **Authoritative financial settlement** | `src/settlement.rs`, `src/ledger.rs` (`SettlementEngine`, FIFO partial fills, fees/maker rebates, funding coverage windows, external cash flow separation, at-most-once `VerifiedLearningOutcome`) | `settlement` (3 tests), `ledger_integration`, `immortality_suite`, `hydra settlement-audit` | Offline Verified | Requires live small-fill reconciliation |
| **Supported-account reconciliation** | `src/account.rs`, `src/ledger_hyperliquid.rs`, `src/execution_recovery.rs` | Fails closed on unified/portfolio margin modes; REST/WS delta reconciliation tested | Offline Verified | Verified during pilot soak |
| **Deterministic shared strategy planning** | `src/fang_engine.rs`, `src/fang_planner.rs`, `src/oracle.rs`, `src/core/engine.rs`, `src/paper_pipeline.rs` | `fang_engine` (2 tests), `tests/fang_parity.rs` bit-for-bit across all 4 consumers | Ready (Simulation Verified) | Verified after pilot |
| **Independent portfolio risk enforcement** | `src/aegis_sovereign.rs`, `src/aegis.rs`, `src/core/engine.rs` (fail-closed refusal on missing L2 depth / `instrument_max_leverage`, MTM exposure + real in-flight reservations, rolling `compute_correlation_matrix`, separate daily/weekly + `session_pnl_usd`, per-candidate pre-submission revalidation) | `aegis_sovereign`, `aegis_integration`, `atomic_admission`, `equilibrium_e2e`, `immortality_suite` | Ready | Ready |
| **Native protective orders** | `src/protective_orders.rs` (`reduce_only: true` exchange stops across partial fills, orphans & blocked entries; fail-closed on missing protective venue) | `protective_orders`, `immortality_suite` | Offline Verified | Verified on live exchange stop ack |
| **Secure signing authority** | `src/treasury.rs` (`WindowsDpapiCurrentUser`, `SigningAccessScope::EngineExclusive`, zero-plaintext `EngineSignerAuthority::sign_digest` in `hyperliquid_wire`, `from_disposable_sandbox_key`) | `treasury` (6 tests), `equilibrium_e2e`, `immortality_suite` | Ready (Windows DPAPI) | Blocked on Unix unless encrypted vault added |
| **Authenticated live-engine control** | `src/engine_ipc.rs`, `src/cockpit.rs` (256-bit constant-time token, monotonic nonce, session-scoped `RenewPilotSupervision { session_id }`, live Cockpit polling badge & `EMERGENCY SUSPEND`) | `engine_ipc`, `equilibrium_e2e`, `immortality_suite` | Ready | Ready |
| **Durable execution recovery** | `src/execution_recovery.rs`, `src/paper_service.rs`, `src/paper_store.rs` (`OrderLifecycleState`, `RefuseBlindRetry`, `PendingPaperFundingObligation`, `PaperSessionLock`) | `execution_recovery` (2 tests), `paper_pipeline`, `paper_store`, `paper_service` | Offline Verified | Verified during pilot |
| **External watchdog supervision** | `src/watchdog.rs`, `src/bin/hydra-cockpit.rs` (`--watchdog-supervisor`, startup grace + clean code-0 exit handling, hang/kill/backoff auto-restart, `watchdog-state.json`, `PRAGMA quick_check(1)`, `measure_host_resources`) | `watchdog` (2 tests), `equilibrium_e2e`, `sentinel_faults`, standalone EXE forced-kill restart test | Verified (Standalone Live Process + Forced-Kill Recovery) | Ready |
| **Intensive fault injection & resource isolation** | `src/qualification.rs`, `src/genesis_nsga2.rs`, `src/research_ui.rs` (`ResourceBudgetScheduler`, NaN/Inf/zero-trade rejection, cancellable UI Laboratory workers + SQLite `research_events`) | 10/10 scenarios pass + `sentinel_faults` (3/3) + `qualification` unit tests | Verified | Verified |
| **Accelerated public paper lifecycle** | `src/paper_service.rs`, `src/bin/hydra-cockpit.rs` (`HYDRA-Leviathan-Windows-x64.exe --watchdog-supervisor` / `--paper-service-worker`, no sibling `hydra.exe`) | Verified on `v1.19.0` standalone EXE: `public_paper-8de662e9592c` (live WS/L2, forced-kill restart `19656 -> 40764`, `total_restarts: 1`, `Pause`/`Resume`/`Stop`, `quick_check: ok`) | Verified (Standalone Supervised Live WS/L2 Lifecycle) | Requires Sustained Multi-Regime Pilot History |
| **Independently validated deployable strategies** | `src/oracle_research.rs`, `src/genesis_nsga2.rs`, `src/cognition_stats.rs`, `src/qualification.rs` (`evaluate_scientific_strategy_acceptance`, `hydra strategy-accept`) | Tested on 60-day public `BTC/ETH/SOL` `1h` bars (`1,441`/sym, `dataset_sha3_256` `e92062f79d1c367a0cc8e77443471a0bca5076db7a2ce1ad55b3e4ae5553f6f3`): **`rejected_keep_disabled`** (`-2.57%` net vs `-35.45%` B&H, `-0.24%` OOS mean, `6.35%` worst OOS DD) | **Blocked Pending Positive Candidate** | Requires Positive Multi-Month OOS Evidence |
| **Operational diagnostics & 7-head Synapses** | `src/synapse_bus.rs` v2, `src/cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`, `src/research_ui.rs`, CLI inspectors | All 7 heads emit typed events; isolated standalone Windows EXE verified across all 5 GUI modes | Ready | Ready |
| **Restricted Monitored Pilot gating, Testnet proof & signed updates** | `src/live_gate.rs` (`LiveExecutionTier`, non-`Deserialize` `MonitoredPilotGrant`, producer-attested/expirable/revocable `EvidenceRecord`, strict `Url::parse`), `src/core/engine.rs` (`step_once_at`), `src/evolution.rs` | `monitored_pilot_suite` (2/2), `equilibrium_e2e` (2/2), `live_gate`, `hydra pilot-readiness --json`, `hydra testnet-probe` (`213` assets) & `hydra testnet-qualify` (`executed_mutations: false`) | **Blocked Pending Owner Testnet Key + Pilot Consent/Ceilings** | Blocked (`unattended_awakened_eligible: false`) |

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

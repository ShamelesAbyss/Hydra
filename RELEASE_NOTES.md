# v1.19.0 | PROJECT EQUILIBRIUM — AUTHORITATIVE RUNTIME, LABORATORY & OPERATIONAL VALIDATION

> **Equilibrium Release (`v1.19.0`).** Continuing directly from verified `v1.18.0` (commit `beee63471741aa536f77c9d548f192e7d77d7485`), **Project Equilibrium** hardens the runtime execution core, signing isolation, Cockpit & Watchdog orchestration, and Quantitative Laboratory honesty end-to-end: genuine L2/funding/microstructure & exchange-clock freshness in `HyperliquidAdapter` and `Engine`, zero-plaintext `EngineSignerAuthority` signing, COGNITION Bayesian weights & MTM/reserved/correlation AEGIS admission with per-candidate pilot revalidation, lossless `PendingPaperFundingObligation`, strict `Url::parse`, non-`Deserialize` `MonitoredPilotGrant` with session-scoped `RenewPilotSupervision`, producer-attested expiring `EvidenceRecord`, self-contained `--watchdog-supervisor`, live Cockpit `EngineIpcClient` controls, interactive ORACLE/GENESIS NSGA-II Laboratory & `TESTNET SANDBOX` panels, 60-day real Hyperliquid validation, and `tests/equilibrium_e2e.rs`. Live execution remains **strictly disabled (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`)** pending external deployable-strategy qualification, owner-run authenticated Testnet functional proof, and explicit owner monitored-pilot consent + hard ceilings.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Awakening command center (embedding both `--paper-service-worker` and `--watchdog-supervisor` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, live Engine IPC panel, interactive Research Laboratory, Treasury & `TESTNET SANDBOX` probe, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.19.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.19.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive.
- **`hydra-1.19.0-source.zip`** — Exact-tag source export.
- **`hydra-1.19.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.19.0-SHA256SUMS.txt`** & **`hydra-1.19.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Equilibrium Runtime, Security & Laboratory

- **Authoritative Market Inputs & Zero-Plaintext Signing (`src/hunter_stream.rs`, `src/adapters/hyperliquid.rs`, `src/hyperliquid_wire.rs`, `src/treasury.rs`):**
  - `FieldFreshness::evaluate` now validates both local receipt age and `exchange_ts_ms` (`MAX_CLOCK_SKEW_MS = 5_000`); `HyperliquidAdapter::snapshot` computes genuine L2 top-10 cumulative USD depth + `metaAndAssetCtxs` funding rates, `warm_meta` caches exchange `maxLeverage` into `instrument_constraints()`, redirects are disabled (`redirect::Policy::none()`), and `HyperliquidAdapter` stores zero plaintext private keys in memory (signing delegates strictly into `EngineSignerAuthority::sign_digest`).
- **Converged `Engine` Truthfulness & Paper Funding Continuity (`src/core/engine.rs`, `src/paper_pipeline.rs`):**
  - `Engine::new_with_venues` wires `SettlementStore`, `EngineIpcServer`, and venue `instrument_constraints`; `step_once_at` integrates HUNTER microstructure, COGNITION Bayesian weights (`OutcomeSource::CallerSuppliedNormalizedR`), MTM exposure, real in-flight `reserved_notional_usd`, rolling `compute_correlation_matrix`, separate daily/weekly + `session_pnl_usd`, per-candidate pre-submission revalidation (`require_authorized_pilot_execution`, halts, suspension), fail-closed AEGIS & protective routing, and fill -> `VerifiedLearningOutcome` promotion. `PendingPaperFundingObligation` now persists across position closes & `PipelineRecoveryCheckpoint`.
- **Self-Contained Watchdog, Gate Hardening & Live UI Controls (`src/watchdog.rs`, `src/bin/hydra-cockpit.rs`, `src/paper_service.rs`, `src/live_gate.rs`, `src/engine_ipc.rs`, `src/qualification.rs`, `src/cockpit.rs`, `src/treasury_ui.rs`, `src/research_ui.rs`):**
  - Enforced strict `reqwest::Url::parse`, non-`Deserialize` `MonitoredPilotGrant` with `session_id`, session-scoped `RenewPilotSupervision { session_id }`, producer-attested expiring/revocable `EvidenceRecord`, and scientific NaN/Inf/zero-trade rejection. Added standalone `--watchdog-supervisor` (with worker startup grace & clean shutdown exit recognition), `WatchdogSupervise` + `TestnetQualify` CLI commands, live Cockpit `EngineIpcClient` polling/controls + `"Start Supervised (Watchdog)"`, read-only `TESTNET SANDBOX` probe UI, and cancellable bounded ORACLE/GENESIS NSGA-II Laboratory workflows.

---

## Testing, 60-Day Laboratory & Standalone Runtime Verification

- **545 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **41 test suites** and **499 passing offline headless tests** (`cargo test --locked --all-targets`) across **40 test suites** (including `tests/equilibrium_e2e.rs` 2/2, `tests/monitored_pilot_suite.rs` 2/2, `tests/sentinel_faults.rs` 3/3, `tests/convergence_e2e.rs` 2/2, `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`).
- `cargo fmt --check`, `cargo audit` (0 vulnerabilities, 1 allowed `chrono` warning `RUSTSEC-2026-0543`), `cargo check --locked --features desktop --no-default-features` (Glow), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Genuine 60-Day Hyperliquid Laboratory Campaign (`hl_60d_btc_eth_sol.json`, `BTC/ETH/SOL` `1h` bars `2026-09-29T03:00:00Z`..`2026-11-28T03:00:00Z`, `dataset_sha3_256` `e92062f79d1c367a0cc8e77443471a0bca5076db7a2ce1ad55b3e4ae5553f6f3`):**
  - `oracle-portfolio --folds 3 --embargo-bars 4`: `$10,000` -> `$9,742.86` (`-2.57%` net across `184` trades after `$241.24` fees + `$84.80` slippage) vs `-35.45%` equal-weight B&H (`+32.88%` excess, `8.34%` max DD).
  - `strategy-accept --folds 3 --embargo-bars 4`: **`rejected_keep_disabled`** (`-2.57%` net < `0.50%` floor; `-0.24%` mean OOS across 3 folds with `46`/`42`/`43` trades, `6.35%` worst OOS DD).
  - `genesis-nsga2 --train-bars 720 --embargo-bars 4 --population 8 --generations 3 --workers 2 --seed 7`: explored 5 Pareto candidates (`auto_deployment_authorized: false`).
  - `qualify-campaign`: `10/10` fault scenarios passed; `profile-host`: 32 cores (`i9-14900HX`), RTX 4070 (`8,188 MB` VRAM), `quick_check_ok: true`; `testnet-probe`: `213` Testnet assets (`BTC=60697.0`, `ETH=1676.65`); `testnet-qualify`: `executed_mutations: false` (read-only).
- **Verified Standalone `v1.19.0` Binary (`HYDRA-Leviathan-Windows-x64.exe`, `25,660,416 B`, `SIBLING_HYDRA=False`):**
  - All 5 GUI launch modes (`spatial`, `classic`, `glow`, `treasury`, `updates`) initialized cleanly (`hydra.db` `quick_check = ok`); detached `--watchdog-supervisor` ran live Hyperliquid WS/L2 (`public_paper-8de662e9592c`), automatically restarted killed worker PID `19656` -> `40764` (`total_restarts: 1` in `watchdog-state.json`), executed `Pause` -> `Resume` -> `Stop` (`cycle_count: 3`, `stream_connected: true`, clean supervisor exit `0`, `paper_sessions.db` `quick_check = ok`), and `hydra pilot-readiness --json` remained fail-closed (`MODE: DORMANT`).
- **Remaining External Blockers for `v2.0.0` Monitored Pilot Activation (Zero Fabrication):**
  1. Passing `hydra strategy-accept` candidate over genuine historical market data (`default` config truthfully returned `rejected_keep_disabled` at `-2.57%` net / `-0.24%` mean OOS across 60-day `BTC/ETH/SOL` `1h` data).
  2. Owner-run authenticated Hyperliquid Testnet functional order/stop proof using an isolated Testnet wallet (`hydra testnet-qualify --execute-confirmed` with disposable keys & faucet funds).
  3. Explicit owner `MonitoredPilotLimits`, dedicated DPAPI wallet authorization, owner consent, and owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX` for signed auto-updates.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.19.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.19.0-linux-SHA256SUMS.txt
mkdir hydra-1.19.0 && tar -xzf hydra-1.19.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.19.0
cd hydra-1.19.0 && ./hydra --version && ./hydra pilot-readiness --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜
# v1.18.0 | PROJECT AWAKENING — RESTRICTED MONITORED PILOT CANDIDATE

> **Final Ascension Milestone III (`v1.18.0`).** Continuing directly from verified `v1.17.0` (commit `049580d5c16c305c5f50f8640456c515b05f5013`), **Project Awakening** delivers the complete Section 15/16 **Restricted Monitored Live-Pilot** authorization & engine enforcement architecture (`MonitoredPilotLimits`, `MonitoredPilotGrant`, `LiveExecutionTier::RestrictedMonitoredPilot`, fail-closed `evaluate_monitored_pilot_eligibility`, `Engine::step_once_at` ceiling/instrument/supervision-lease enforcement with zero blind liquidation and zero auto-resume on restart, authenticated IPC `RenewPilotSupervision`, `hydra pilot-readiness --json`, and `tests/monitored_pilot_suite.rs`). Per Section 16 (*"Do not call a locked candidate live-ready"*), live execution remains **strictly disabled (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`)** pending external deployable-strategy qualification, owner-run authenticated Testnet exchange mutation proof, and explicit owner monitored-pilot consent + hard ceilings.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Awakening command center (embedding `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.18.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.18.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive.
- **`hydra-1.18.0-source.zip`** — Exact-tag source export.
- **`hydra-1.18.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.18.0-SHA256SUMS.txt`** & **`hydra-1.18.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Restricted Monitored Pilot Architecture

- **Explicit Owner Ceilings, Consent & Evidence Gating (`src/live_gate.rs`, `src/cli/mod.rs`):**
  - Added `LiveExecutionTier` (`Dormant`, `RestrictedMonitoredPilot`, `UnattendedAwakened`), `PilotOrderType` (requiring `NativeReduceOnlyStop`), fail-closed `MonitoredPilotLimits::validate` (`wallet_address`, `allowed_instruments`, `qualified_strategy_id`, `max_allocated_capital_usd`, `max_order_notional_usd`, `max_leverage <= 5`, `max_aggregate_exposure_usd`, `max_daily_loss_usd`, `max_session_loss_usd`, `max_drawdown_ratio <= 0.20`, `pilot_session_duration_secs <= 86400`, `operator_supervision_lease_secs` `30..=1800`), ephemeral `MonitoredPilotGrant` (never persisted for restart auto-resume), `evaluate_monitored_pilot_eligibility` (requiring all 5 core pillars plus historical OOS, Hyperliquid Testnet, and public-paper Pillar 6 records; keeping `unattended_awakened_eligible: false`), and `hydra pilot-readiness --json`.
- **Engine Enforcement & Authenticated Supervision Lease Renewal (`src/core/engine.rs`, `src/engine_ipc.rs`, `tests/monitored_pilot_suite.rs`):**
  - Bound `Engine::step_once_at(tick, now_ms)` and `Engine::attach_monitored_pilot_at` to enforce allowed symbols, qualified strategy ID, per-order notional ceiling, aggregate exposure ceiling, daily/session loss bounds, session duration, and active operator supervision lease (`EngineControlCommand::RenewPilotSupervision`, refused when suspended).
  - Verified that supervision lease expiration blocks new entries without blindly liquidating open positions while keeping native `reduce_only` protective management active, and restarted engines always boot with `active_pilot().is_none()`.

---

## Testing, Live Public-Paper & Accelerated Awakening Verification

- **541 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **40 test suites** and **495 passing offline headless tests** (`cargo test --locked --all-targets`) across **39 test suites** (including `tests/monitored_pilot_suite.rs` 2/2 x 3 stress iterations, `tests/sentinel_faults.rs` 3/3, `tests/convergence_e2e.rs` 2/2, `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings in all `v1.18.0` modified/added modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Verified `v1.17.0` Published Standalone Binary & Real Public-Market Paper Lifecycle (`HYDRA-Leviathan-Windows-x64.exe` SHA-256 `566f29d00786ea9f1dfd433cd64706d3d5cbfe71681d3562f43e5a4dfd51c3ea`, `SIBLING_HYDRA=False`):**
  - Clean-profile GUI initialized `hydra.db` (`PRAGMA quick_check(1)` = `ok`); detached `public_paper-9370231f3128` worker streamed live Hyperliquid WS/L2 (`stream_connected: true`, `watchlist: ["SOL", "BTC", "ETH"]`), completed `5` cycles over `38.63s` wall-clock (`pause` -> `resume` -> `stop`, `mtm_equity_usd: "9996.73"`, `1,114,112 B` SQLite `quick_check: ok`).
- **Remaining External Blockers for `v2.0.0` Monitored Pilot Activation (Zero Fabrication):**
  1. Passing `hydra strategy-accept` candidate over genuine historical market data (`default` config truthfully returned `rejected_keep_disabled` at `-2.80%` net / `-0.51%` mean OOS across 45-day `BTC/ETH/SOL` `1h` data).
  2. Owner-run authenticated Hyperliquid Testnet functional order/stop proof using an isolated Testnet wallet (`hydra testnet-probe` verified read-only reachability across `213` Testnet assets).
  3. Explicit owner `MonitoredPilotLimits`, dedicated DPAPI wallet authorization, owner consent, and owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX` for signed auto-updates.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.18.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.18.0-linux-SHA256SUMS.txt
mkdir hydra-1.18.0 && tar -xzf hydra-1.18.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.18.0
cd hydra-1.18.0 && ./hydra --version && ./hydra pilot-readiness --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜
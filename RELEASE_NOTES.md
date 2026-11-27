# v1.14.0 | PROJECT IMMORTALITY — FINANCIAL TRUTH & SOVEREIGN PROTECTION

> **Sovereign Trilogy Milestone II (`v1.14.0`).** Continuing directly from verified `v1.13.0`, **Project Immortality** delivers the authoritative financial accounting, execution recovery, exchange-native reduce-only protective stops, sovereign fail-closed portfolio risk, Treasury signing-scope isolation, authenticated main-engine IPC, and evidence-backed 5-pillar certification evaluation architecture while keeping unattended live execution strictly locked (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`) until external watchdog supervision and extended public paper qualification complete.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Immortality command center (embedding `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.14.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.14.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (`hydra certification-status`, `hydra settlement-audit`, `hydra fang-registry`, `hydra cognition-stats`, `hydra oracle-portfolio`, `hydra genesis-nsga2`, `hydra paper-service`, `hydra synapse-paper`).
- **`hydra-1.14.0-source.zip`** — Exact-tag source export.
- **`hydra-1.14.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.14.0-SHA256SUMS.txt`** & **`hydra-1.14.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Financial Truth & Sovereign Protection

- **NEXUS Authoritative Financial Settlement (`src/settlement.rs`, `hydra settlement-audit`):**
  - Exact `rust_decimal::Decimal` FIFO partial-fill lot settlement attributing multi-tranche entry fees, exit fees, negative maker rebates, and position-specific hourly funding payments (`FundingCoverageWindow`).
  - Retains explicit `SettlementHoldReason`s (`UnknownFee`, `UnverifiedPnlConvention`, `MissingFundingCoverage`, `UnsupportedToken`, `ExternalCashFlowGap`) and keeps `verified_net_pnl_usdc: None` rather than converting unknown components to zero.
  - Idempotent fill/funding/cash-flow deduplication, `reconcile_account_equity` separating external deposits/withdrawals from realized net trading PnL, and `emit_eligible_learning_outcomes` enforcing strictly **at-most-once** learning emission across reconnects/restarts.
- **NEXUS Execution Recovery & Native Protective Orders (`src/execution_recovery.rs`, `src/protective_orders.rs`):**
  - Full `OrderLifecycleState` separation (`Prepared`, `Signed`, `Submitted`, `Acknowledged`, `PartiallyFilled`, `Filled`, `Cancelled`, `Rejected`, `Unresolved`), `InstrumentOrderConstraints` tick/lot rounding & leverage checks, duplicate `cloid` blocking, `check_rest_ws_consistency`, blind-retry refusal on ambiguous submissions, and startup position ownership recovery.
  - `ProtectiveOrderManager::reconcile_positions_and_stops` managing exchange-side `reduce_only: true` protective stops across partial-fill quantity adjustments and closed-position orphan cancellations—remaining authorized even when new exposure entries are blocked, while halting new entries if stop protection cannot be verified.
- **AEGIS Independent Portfolio Risk Sovereignty (`src/aegis_sovereign.rs`):**
  - Fail-closed admission evaluator (`evaluate_sovereign_admission`) rejecting any missing or stale risk input (`account_equity_usd`, `available_collateral_usd`, `estimated_liquidation_distance_pct`, `market_depth_usd`, `instrument_max_leverage`, `daily_pnl_usd`, `weekly_pnl_usd`).
  - Validates post-fee exposure, correlated cluster limits, liquidation distance floor, hourly funding drag, daily/weekly loss breakers, and audited circuit-breaker rearm.
- **Treasury Credential Isolation & Authenticated Engine IPC (`src/treasury.rs`, `src/engine_ipc.rs`, `src/live_gate.rs`, `hydra certification-status`):**
  - Added `SigningAccessScope` (`EngineExclusive` vs `UiReadOnly`), encrypted-backend enforcement (`WindowsDpapiCurrentUser`), `revoke_live_authorization`, and `replace_signing_key_audited` without leaking secret bytes in errors or `Debug` output.
  - Added OS-scoped `EngineIpcServer` / `EngineIpcClient` with constant-time 256-bit token checks, strictly monotonic `nonce` replay prevention, timestamp freshness bounds, pre-allocation bounded line reads, `EmergencySuspend`, mode transition refusal for `AWAKENED`/`GUARDIAN`, and evidence-backed 5-pillar certification status (`evaluate_certification_readiness`).

---

## Testing & Validation Results

- **527 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **36 test suites** (`311` lib unit tests + integration/doc suites), plus **481 passing offline headless tests** (`cargo test --locked --all-targets`) across **35 test suites** (`265` lib unit tests + integration/doc suites, including `tests/immortality_suite.rs`, `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.14.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.14.0-linux-SHA256SUMS.txt
mkdir hydra-1.14.0 && tar -xzf hydra-1.14.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.14.0
cd hydra-1.14.0 && ./hydra --version && ./hydra certification-status --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

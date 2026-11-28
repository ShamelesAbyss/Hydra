# v1.15.0 | PROJECT SOVEREIGNTY — AUTONOMOUS QUANTITATIVE INTELLIGENCE

> **Sovereign Trilogy Milestone III (`v1.15.0`).** Completing the internal Sovereign Trilogy engineering campaign alongside verified `v1.13.0` (**Project Omniscience**) and `v1.14.0` (**Project Immortality**), **Project Sovereignty** delivers external OS-locked process watchdog supervision, a 10-scenario fault-injection campaign, priority-based resource isolation, conservative scientific strategy acceptance gating, full 7-head `SynapseBus` integration, and an evidence-first Section 26 `v2.0.0` Readiness Matrix (`READINESS_V2.md`). Unattended live execution intentionally remains locked (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`) until multi-week public-market paper soak telemetry and owner-provisioned `secp256k1` release signing keys are completed externally.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Sovereignty command center (embedding `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.15.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.15.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (`hydra watchdog-check`, `hydra qualify-campaign`, `hydra strategy-accept`, `hydra certification-status`, `hydra settlement-audit`, `hydra fang-registry`, `hydra cognition-stats`, `hydra oracle-portfolio`, `hydra genesis-nsga2`, `hydra paper-service`, `hydra synapse-paper`).
- **`hydra-1.15.0-source.zip`** — Exact-tag source export.
- **`hydra-1.15.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.15.0-SHA256SUMS.txt`** & **`hydra-1.15.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — External Supervision, Fault Injection & `v2.0.0` Readiness

- **External Watchdog Supervision (`src/watchdog.rs`, `hydra watchdog-check`):**
  - Enforces exclusive OS `watchdog.lock` process ownership, SQLite `PRAGMA quick_check(1)` + database size-budget monitoring, real Win32 (`GetDiskFreeSpaceExW`) / POSIX (`df -Pk`) disk free-space verification, exponential restart backoff budgets with persistent `watchdog-alerts.jsonl` escalation records, and a mandatory 7-item `PostRestartRevalidationChecklist` (`live_auto_resume_prohibited: true`).
- **10-Scenario Fault-Injection Campaign & Resource Priority Scheduler (`src/qualification.rs`, `hydra qualify-campaign`, `hydra strategy-accept`):**
  - Exercises all 10 failure classes end-to-end (`network_loss`, `exchange_unavailable`, `stale_and_malformed_data`, `unexpected_process_termination`, `database_locking`, `disk_exhaustion`, `interrupted_persistence`, `ipc_auth_and_frame_failure`, `resource_contention`, and `restart_continuity`), records honest real-clock elapsed qualification runtimes (`actual_elapsed_seconds`), enforces conservative scientific strategy gating (`EligibleForMonitoredPilot` vs. `RejectedKeepDisabled`), and isolates priority slots (`ExecutionAndProtective` / `MarketIngestion` > `UiAndTelemetry` > `HeavyOptimization`) via `ResourceBudgetScheduler`.
- **7-Head `SynapseBus`, 6-Pillar `PendingCertification` & `READINESS_V2.md` (`src/live_gate.rs`, `tests/sovereignty_suite.rs`, `READINESS_V2.md`):**
  - Verifies typed `SynapseBus` connectivity across all seven `HeadId`s (`Hunter`, `Fang`, `Cognition`, `Aegis`, `Nexus`, `Oracle`, `Genesis`), expands `evaluate_certification_readiness` to 6 explicit pillars (`ExternalWatchdog` verified offline; `ExtendedMultiWeekPublicPaperAndOwnerSigning` cleanly marked `PendingExternalValidation`), and publishes the Section 26 `v2.0.0` Live-Trading Readiness Matrix distinguishing a monitored limited-capital pilot from unattended production `AWAKENED` operation.

---

## Testing & Validation Results

- **530 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **37 test suites** (`313` lib unit tests + integration/doc suites) and **484 passing offline headless tests** (`cargo test --locked --all-targets`) across **36 test suites** (`267` lib unit tests + integration/doc suites, including `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`).
- `hydra --no-banner qualify-campaign` passed **10/10** fault-injection scenarios (`all_passed: true`) alongside an honest real-clock local simulation snapshot (`cycles_completed: 5`, `equity_usd: "9998.2000"`, `total_fees_usd: "1.8000"`).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (0 warnings in new `v1.15.0` modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.15.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.15.0-linux-SHA256SUMS.txt
mkdir hydra-1.15.0 && tar -xzf hydra-1.15.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.15.0
cd hydra-1.15.0 && ./hydra --version && ./hydra qualify-campaign
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜
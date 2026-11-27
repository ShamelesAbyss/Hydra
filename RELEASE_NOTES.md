# v1.12.0 | PROJECT ASCENDANCY — OPERATIONAL RELIABILITY & TRANSACTIONAL TRUTH

> **Project Ascendancy P0 Operational Correction (`v1.12.0`).** Continuing directly from `v1.11.0`, `v1.12.0` hardens the runtime, transactional, and distribution reliability of the Project Ascendancy paper-simulation foundation: self-contained standalone headless worker mode (`HYDRA-Leviathan-Windows-x64.exe --paper-service-worker` with zero dependency on a sibling `hydra.exe`), staged transactional `commit_cycle` execution with explicit `Faulted` recording and stream watchdog supervision, lossless checkpoint & `SynapseBus` event-ID recovery in `paper_sessions.db`, lock/paused archive & reset guards, bounded pre-allocation IPC reads, independent per-field freshness & modeled hourly funding attribution, and structural `VenueTransportClass` isolation of loopback test fixtures from production certification (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Ascendancy command center (now embedding headless `--paper-service-worker` execution so starting a detached background paper session requires zero sibling `hydra.exe`, alongside automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.12.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.12.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (including `hydra paper-service` & `hydra synapse-paper`) with guides and assets.
- **`hydra-1.12.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.12.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.12.0-SHA256SUMS.txt`** & **`hydra-1.12.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Operational Reliability & Transactional Truth

- **Self-Contained Standalone Detached Paper Service (`src/bin/hydra-cockpit.rs`, `src/paper_service.rs`):**
  - Added internal windowless `--paper-service-worker` mode to `hydra-cockpit` so `PaperServiceClient::spawn_detached` re-executes the standalone graphical binary directly without requiring a sibling `hydra.exe`.
- **Transactional Service Loop, Stream Watchdog & Bounded IPC (`src/paper_service.rs`, `src/cockpit.rs`):**
  - Cycles execute against a cloned staging pipeline and promote live memory state ONLY after `commit_cycle` succeeds atomically in SQLite; heartbeat, pipeline, or persistence failures transition cleanly to `PaperSessionStatus::Faulted` with persisted `fault_reason` surfaced in the Cockpit Overview without overwriting fault status on exit.
  - Bounded local TCP IPC reads with `Read::take(MAX_IPC_LINE_BYTES + 1)` before allocation, propagated DB errors cleanly to IPC callers, persisted compact market-observation snapshots to `paper_observations`, and added unexpected stream-exit watchdog restarts.
- **Complete Durable Session Recovery & Ownership Guards (`src/paper_store.rs`, `src/synapse_bus.rs`, `src/cli/mod.rs`):**
  - Persisted `PipelineRecoveryCheckpoint` (`SynapseBus::next_event_id`, `dropped_events`, `simulated_samples`, `last_funding_applied_ms`, `demo_candles`) inside atomic `IMMEDIATE` transactions and restored bus history on `load_pipeline` so event IDs remain strictly monotonic across restarts.
  - Guarded `archive_session_guarded` and `reset_session_confirmed_guarded` against `Running`, `Paused`, and OS-locked (`PaperSessionLock::assert_unowned`) sessions.
- **Per-Field Freshness, True History Duration & Funding Settlement (`src/hunter_stream.rs`, `src/hunter.rs`, `src/paper_pipeline.rs`):**
  - Tracked `FieldFreshness` and `MarketFreshnessReport` independently across `mark`, `bid_ask`, `l2_depth`, `funding`, `instrument_meta`, `completed_candles`, and `derived_stats` so a fresh L2 book never masks stale marks or funding.
  - Rejected stale marks BEFORE both simulated exits and entries, replaced synthetic `completed_candles.len() / 24.0` listing-age estimates with exact `observed_history_minutes` and `instrument_listing_age_days: None`, and applied `apply_simulated_funding` across crossed epoch-hour boundaries without double-charging.
- **Structural Gate Isolation (`src/live_gate.rs`, `src/adapters/hyperliquid.rs`, `tests/live_gate_adversarial.rs`):**
  - Added sealed `VenueTransportClass` (`ProductionMainnet`, `#[cfg(test)] LoopbackTestFixture`) so non-test builds compile only `ProductionMainnet` without relying solely on URL string classification.

---

## Testing & Validation Results

- **504 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **34 test suites** (`290` lib unit tests + integration/doc suites), plus **458 passing offline headless tests** (`cargo test --locked --all-targets`) across **33 test suites** (`244` lib unit tests + integration/doc suites, including `tests/live_gate_adversarial.rs` 2/2 and `tests/synapse_pipeline.rs` 2/2).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.12.0-SHA256SUMS.txt`** from the Assets section below.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.12.0-linux-SHA256SUMS.txt
mkdir hydra-1.12.0 && tar -xzf hydra-1.12.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.12.0
cd hydra-1.12.0 && ./hydra --version && ./hydra paper-service list
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

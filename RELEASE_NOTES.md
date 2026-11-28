# v1.17.0 | PROJECT SENTINEL — AUTONOMOUS RESILIENCE & OPERATIONAL PROOF

> **Final Ascension Milestone II (`v1.17.0`).** Continuing directly from verified `v1.16.0` (commit `76728dff9eebafa040c87beb85d22facd1001af7`), **Project Sentinel** adds real live-process external watchdog supervision with persisted restart budgets, enforced exponential backoff, and hang-termination (`src/watchdog.rs`), priority CPU/resource isolation around GENESIS NSGA-II (`src/genesis_nsga2.rs`), version-bound SHA3-256 `EvidenceRegistry` verification removing hardcoded `OfflineFixtureVerified` defaults (`src/live_gate.rs`), Windows 11 host resource profiling (`hydra profile-host`) & read-only Hyperliquid Testnet metadata/mids probing (`hydra testnet-probe`), real historical 45-day `BTC`/`ETH`/`SOL` OOS evaluation via `hydra strategy-accept`, and intensive end-to-end fault validation (`tests/sentinel_faults.rs`). Live trading remains locked (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`) until deployable strategy acceptance, owner-run Testnet functional order/stop proof, and explicit owner monitored-pilot ceilings are satisfied.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Sentinel command center (embedding `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.17.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.17.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive.
- **`hydra-1.17.0-source.zip`** — Exact-tag source export.
- **`hydra-1.17.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.17.0-SHA256SUMS.txt`** & **`hydra-1.17.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Project Sentinel

- **Live Worker External Watchdog, Host Profiler & Authoritative Post-Restart Inspection (`src/watchdog.rs`, `src/cli/mod.rs`):**
  - Added atomic `watchdog-state.json` persistence for `RestartBudgetTracker` across supervisor restarts, fixed `supervise_bounded_process` to sleep computed exponential backoff, and implemented non-blocking `WatchdogSupervisor::supervise_live_worker` polling heartbeat/progress probes (`WorkerHealthProbe`), triggering bounded graceful shutdown -> forced `kill()` on stalled workers, and persisting durable `worker_hang_detected` / `worker_hang_terminated` / `restart_budget_exhausted` JSONL alerts.
  - Replaced hardcoded `PostRestartRevalidationChecklist` assumptions with read-only SQLite `inspect_post_restart_evidence` inspecting `orders`, `execution_journal`, and `meta_kv` (`engine_control_authority`, `trading_account`) while keeping `live_auto_resume_prohibited: true` and honest `market_fresh: false` offline defaults.
  - Added `HostResourceSnapshot` & `measure_host_resources` (`hydra profile-host`) probing real logical CPU cores, Win32 `GlobalMemoryStatusEx` / Linux `/proc/meminfo` host RAM, OS free disk space, SQLite DB bytes, and optional NVIDIA GPU VRAM (`nvidia-smi`), verified on the target Windows 11 Lenovo Legion (`32` logical cores, `20,806 / 32,543 MB` RAM, `NVIDIA GeForce RTX 4070 Laptop GPU` `1,498 / 8,188 MB` VRAM).
- **Resource Budget Isolation, Provenance Clarity & Version-Bound `EvidenceRegistry` (`src/genesis_nsga2.rs`, `src/qualification.rs`, `src/live_gate.rs`, `src/oracle_research.rs`, `src/hyperliquid_wire.rs`):**
  - Connected `ResourceBudgetScheduler` to `run_nsga2_campaign` so background research reserves at least 1 `ProtectiveCritical` CPU worker slot; added `EvidenceProvenanceLabel` distinguishing synthetic snapshots from historical OOS, public-paper, and Testnet evidence.
  - Replaced hardcoded `OfflineFixtureVerified` defaults in `src/live_gate.rs` with SHA3-256 signed `EvidenceRecord` & `EvidenceRegistry::verify_record` bound to exact `software_version`, `strategy_registry_version`, `risk_policy_sha3`, environment, and digest; upgraded `hydra strategy-accept` to evaluate rolling purged walk-forward folds and OOS bar-level `simulate_portfolio_with_bar_returns` bootstrap CI; and added `hydra testnet-probe` (`https://api.hyperliquid-testnet.xyz/info`, read-only).

---

## Testing, Host & Accelerated Awakening Qualification Evidence

- **539 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **39 test suites** and **493 passing offline headless tests** (`cargo test --locked --all-targets`) across **38 test suites** (including `tests/sentinel_faults.rs` 3/3 x 3 stress iterations, `tests/convergence_e2e.rs` 2/2, `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings in all `v1.17.0` Sentinel files), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Honest Accelerated Awakening Field Checks (prior to `v1.17.0` tag):**
  - **`hydra profile-host --json` (Windows 11 Legion):** `32` cores, `20,806 / 32,543 MB` RAM, `328,159,125,504 B` free disk, RTX 4070 Laptop GPU (`1,498 / 8,188 MB` VRAM, `17%` utilization).
  - **`hydra testnet-probe --json`:** Hyperliquid Testnet reachable (`213` assets, `3,533` mids, `BTC` `szDecimals=5`/`maxLeverage=40`, `btc_mid="60687.5"`, `eth_mid="1678.55"`; authenticated order/stop mutations require isolated owner testnet keys + faucet funds).
  - **`hydra strategy-accept` (45-day public `1h` `BTC/ETH/SOL` dataset, `1,081` bars/symbol, `dataset_sha3_256` `7ff196eaadefb109ac24ff72e5e476615897ca4ff99d907e485368d50a7a31b8`):** Truthfully evaluated to **`rejected_keep_disabled`** (default `UnifiedStrategyConfig` achieved `+27.12%` excess over `-29.92%` buy-and-hold, yet finished `-2.80%` net after costs over `149` trades with `8.09%` max DD and `-0.51%` mean OOS return across 3 purged folds — keeping live entries disabled per evidence-first rules).

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.17.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.17.0-linux-SHA256SUMS.txt
mkdir hydra-1.17.0 && tar -xzf hydra-1.17.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.17.0
cd hydra-1.17.0 && ./hydra --version && ./hydra profile-host --json && ./hydra testnet-probe --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜
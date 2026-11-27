# v1.11.0 | PROJECT ASCENDANCY — TRUSTWORTHY INGESTION, DURABLE PAPER & EXECUTION GATE

> **Project Ascendancy Milestone 1 (`v1.11.0`).** Continuing directly from `v1.10.0`, `v1.11.0` corrects the Connected Intelligence foundation: real Hyperliquid public WebSocket & REST snapshot ingestion (`src/hunter_stream.rs`), elimination of fabricated public-market approximations (`src/paper_pipeline.rs`, `src/synapse_bus.rs` schema v2), isolated durable SQLite paper-trading sessions (`src/paper_store.rs`), an independent OS-locked + authenticated-IPC headless paper engine service (`src/paper_service.rs`, `hydra paper-service`), and a fail-closed `PendingCertification` execution gate across every production entry point (`src/live_gate.rs`). **Unattended live-trading certification remains incomplete** (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Ascendancy command center (automatic Glow fallback, clean-profile auto-initialization, visible startup diagnostics, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client + detached headless service launcher, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.11.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.11.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (including `hydra paper-service` & `hydra synapse-paper`) with guides and assets.
- **`hydra-1.11.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.11.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.11.0-SHA256SUMS.txt`** & **`hydra-1.11.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Project Ascendancy

- **Authoritative `PendingCertification` Live-Execution Gate (`src/live_gate.rs`, `tests/live_gate_adversarial.rs`):**
  - Enforces one authoritative fail-closed `PendingCertification` gate across `Engine::new`, `HyperliquidAdapter::new`, `HyperliquidAdapter::submit_order_impl`, `ExecutionEngine::submit` (when targeting a non-loopback production venue), `StateManager::emergency_flatten`, `Command::Run`, and `Treasury::ReadinessReport`. No CLI flag, direct constructor, or adapter setting can bypass certification while settlement, recovery, protective orders, IPC, or watchdog gates remain open.
- **Genuine HUNTER Public WebSocket & REST Snapshot Ingestion (`src/hunter_stream.rs`):**
  - Connects via `tokio-tungstenite` (`rustls-tls-webpki-roots`) to Hyperliquid public WebSocket (`wss://api.hyperliquid.xyz/ws` / loopback test endpoints) for `l2Book`, `trades`, `candle`, and `activeAssetCtx`, paired with bounded `/info` REST snapshot initialization & conservative REST fallback during reconnects.
  - Preserves original `exchange_ts_ms: Option<i64>` separately from local `received_at_ms: i64`, deduplicates trades by `tid`, counts out-of-order timestamps, separates in-progress candles from strictly completed chronological OHLCV bars, computes true bid/ask depth and spread strictly from L2 levels, leaves derived metrics `None` when `< 12` completed bars exist, and monitors `watchlist ∪ held_positions` continuously.
- **Provenance Truthfulness & Missing-Mark Hardening (`src/synapse_bus.rs` v2, `src/paper_pipeline.rs`):**
  - Eliminated the old GUI-owned fake-metric poller (`open_interest * 0.05`, `.or(Some(8.0))`, `directionality: Some(0.55)`, `age_days: Some(90.0)`, `coverage_fraction: Some(1.0)`). Upgraded `SynapseBus` to `SYNAPSE_SCHEMA_VERSION = 2` with `AuthoritativeExchange`, `DerivedMarketStat`, `SimulationAssumption`, `SyntheticTestFixture`, `session_id`, and `is_critical()`.
  - Added checked missing-mark refusal (`checked_mark_to_market_equity`) and post-fill exposure/fee validation on AEGIS paper admission while keeping protective exits active.
- **Durable SQLite Paper Sessions & Independent Engine Service (`src/paper_store.rs`, `src/paper_service.rs`, `src/cli/mod.rs`, `src/cockpit.rs`):**
  - Isolated SQLite database (`paper_sessions.db`, structurally separated from `hydra.db`) persisting `public_paper`, `offline_demo`, and `historical_replay` sessions independently across restarts, complete with positions, orders, closed trades, equity/drawdown history, risk decisions, durable critical events, and confirmation-gated resets (`CONFIRM_RESET:<session_id>`).
  - Independent headless `paper-service` runner with OS exclusive locks (`paper-locks/`), authenticated local loopback TCP control (`paper-control/` with 256-bit random token and `chmod 0600` on Unix), heartbeat tracking, pause/resume/stop lifecycle, `hydra paper-service` CLI, and persistent Cockpit Overview integration without disturbing the finalized 3D visuals.

---

## Testing & Validation Results

- **496 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **34 test suites** (`282` lib unit tests + integration/doc suites), plus **450 passing offline headless tests** (`cargo test --locked --all-targets`) across **33 test suites** (`236` lib unit tests + integration/doc suites, including `tests/live_gate_adversarial.rs` 2/2 and `tests/synapse_pipeline.rs` 2/2).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.11.0-SHA256SUMS.txt`** from the Assets section below.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.11.0-linux-SHA256SUMS.txt
mkdir hydra-1.11.0 && tar -xzf hydra-1.11.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.11.0
cd hydra-1.11.0 && ./hydra --version && ./hydra paper-service list
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

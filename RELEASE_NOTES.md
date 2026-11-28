# v1.16.0 | PROJECT CONVERGENCE — UNIFIED EXECUTION & FINANCIAL TRUTH

> **Final Ascension Milestone I (`v1.16.0`).** Continuing directly from verified `v1.15.0`, **Project Convergence** eliminates parallel runtime drift by converging the actual `Engine` (`src/core/engine.rs`), Windows socket IPC (`src/engine_ipc.rs`, `src/paper_service.rs`), durable SQLite settlement (`src/settlement.rs`), native Hyperliquid `reduce_only` trigger stops & `TestnetSandbox` (`src/hyperliquid_wire.rs`, `src/adapters/hyperliquid.rs`, `src/live_gate.rs`), engine-owned `EngineSignerAuthority` (`src/treasury.rs`), and full ORACLE/GENESIS experiment fingerprinting (`src/oracle_research.rs`, `src/genesis_nsga2.rs`) into one authoritative runtime path verified end-to-end against stateful mock exchange execution. Unattended live execution remains locked (`MODE: DORMANT`, `ENABLE_LIVE_TRADING=false`, `PendingCertification`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Convergence command center (embedding `--paper-service-worker` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, persistent Overview paper-session client, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.16.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.16.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive.
- **`hydra-1.16.0-source.zip`** — Exact-tag source export.
- **`hydra-1.16.0-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.16.0-SHA256SUMS.txt`** & **`hydra-1.16.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Engineering Upgrades — Project Convergence

- **Windows IPC Socket Lifecycle Repair & Engine Control Authority (`src/engine_ipc.rs`, `src/paper_service.rs`):**
  - Fixed root-cause Windows `WSAECONNRESET` (`10054`) on non-blocking `TcpListener::accept` by explicitly restoring blocking mode (`stream.set_nonblocking(false)`) before applying timeouts, using chunked bounded frame reads (`MAX_ENGINE_IPC_BYTES`), flushing + shutting down Write-half sockets cleanly, bounding concurrent client threads (`MAX_CONCURRENT_IPC_CLIENTS = 8`), applying Windows owner ACLs (`icacls`), and returning fail-closed status (`mode: "UNKNOWN"`, `protective_stops_verified: false`, `maintenance_ready: false`) on unauthorized/malformed/oversized requests.
  - Added `EngineControlAuthority` bound to running `Engine` state & persistent SQLite `meta_kv` (`engine_mode`, `engine_emergency_suspended`, `engine_breaker_latched`) + `engine-ipc-audit.jsonl`.
- **Unified Authoritative `Engine` & FANG/AEGIS/NEXUS Integration (`src/core/engine.rs`, `src/fang_engine.rs`, `src/settlement.rs`, `src/treasury.rs`, `src/hyperliquid_wire.rs`, `src/adapters/hyperliquid.rs`, `src/live_gate.rs`):**
  - Replaced the legacy `strat.evaluate()` loop inside `Engine` with shared deterministic `FangUnifiedPlanner` planning, marked `BoundedGrid` `research_only: true` and excluded it from live families, enforced `evaluate_sovereign_admission` + `InstrumentOrderConstraints` rounding before entry execution, wired startup/recovery journal reconciliation (`reconcile_startup_and_recovery`), and executed place-new-then-cancel-old native `reduce_only: true` protective stop management (`reconcile_protective_orders`) even when `EmergencySuspend` blocks new entries.
  - Added durable SQLite `SettlementStore` + `PendingFundingObligation` holds, `EngineSignerAuthority` in Treasury (crate-private raw secret helper + documented DPAPI same-user threat model), `VenueTransportClass::TestnetSandbox` + native `TriggerOrderAction` / `CancelAction`, and full ORACLE/GENESIS SHA3-256 dataset/config/option fingerprinting + SQLite `research_events` (`proposal_only_non_executable`).

---

## Testing & Validation Results

- **535 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **38 test suites** (`316` lib unit tests + integration/doc suites) and **489 passing offline headless tests** (`cargo test --locked --all-targets`) across **37 test suites** (`270` lib unit tests + integration/doc suites, including `tests/convergence_e2e.rs` 2/2, real-engine `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, `tests/hyperliquid_conformance.rs`, and 3 consecutive `engine_ipc` parallel socket stress iterations with 0 resets).
- `cargo fmt --check`, `cargo audit` (596 crate dependencies, 0 vulnerability entries), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (0 warnings in `v1.16.0` modified/added modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.16.0-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.16.0-linux-SHA256SUMS.txt
mkdir hydra-1.16.0 && tar -xzf hydra-1.16.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.16.0
cd hydra-1.16.0 && ./hydra --version && ./hydra qualify-campaign
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜
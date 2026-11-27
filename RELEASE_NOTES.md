# v1.10.0 | PROJECT SYNAPSE — CONNECTED INTELLIGENCE

> **Project Synapse Phase II — Connected Intelligence.** Building directly on `v1.9.0 | PROJECT SYNAPSE — SPATIAL CONSCIOUSNESS`, `v1.10.0` unifies Hydra's independent subsystem silos (`HUNTER`, `COGNITION`, `FANG`, `AEGIS`, and `NEXUS Paper`) across a versioned typed Nervous System event bus and a connected, strictly `SIMULATION_ONLY` paper-trading pipeline. Live real-money execution stays strictly locked (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`), and unattended `AWAKENED` execution remains uncertified pending complete round-trip settlement, stream/restart recovery, native protective exchange orders, authenticated local control IPC, and watchdog certification (`MILESTONE_MATRIX.md`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Connected Intelligence command center (automatic Glow fallback, clean-profile auto-initialization, visible startup diagnostics, 360° orbital 96-bone Leviathan sanctuary, live Synapse Overview paper-simulation controls & head energy pulses, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.10.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.10.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive (including `hydra synapse-paper`) with guides and assets.
- **`hydra-1.10.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.10.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.10.0-SHA256SUMS.txt`** & **`hydra-1.10.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Features — Connected Intelligence Architecture

- **Versioned Synapse Nervous System Event Bus (`src/synapse_bus.rs`):**
  - Introduces schema-versioned (`SYNAPSE_SCHEMA_VERSION = 1`) bounded FIFO event transport (`SynapseBus`, capacity 512 with explicit dropped-oldest accounting), typed `HeadId`, `DataProvenance` tagging, correlation IDs, JSONL serialization/replay (`to_jsonl` / `from_jsonl`), and dedicated typed payloads (`HunterObservation`, `CognitionAssessment`, `FangPlanEvaluated`, `AegisDecision`, `NexusPaperExecution`, `OracleGenesisFinding`, `PipelineFault`).
- **Connected 5-Head `SIMULATION_ONLY` Paper Pipeline (`src/paper_pipeline.rs`):**
  - Orchestrates **`HUNTER` -> `COGNITION` -> `FANG` -> `AEGIS` -> `NEXUS` (Paper)** deterministically per step:
    - **HUNTER:** Stale-data age rejection (`stale_market_max_age_ms`), eligibility checks, and watchlist rotation (`evaluate`).
    - **COGNITION:** Zero-lookahead causal market-regime inference (`infer_regime`, `TrendingUp`, `TrendingDown`, `MeanRevertingRange`, `HighVolatilityTurbulent`) plus simulated-outcome shadow attribution (`evaluate_shadow`).
    - **FANG:** Shared deterministic `plan` arbitration honoring symbol budgets, single-strategy ownership, confidence floor, and EMA-anchor trend/pullback structure (`ema_anchor` + aligned trend gating).
    - **AEGIS:** Independent paper admission checking breaker latch, freshness, per-market/total exposure ceilings, and `-10%` adverse-shock maintenance-margin stress (`evaluate_margin_stress`), while permitting protective exits even when the drawdown breaker is latched.
    - **NEXUS Paper (`PaperLedger`):** Exact checked-`Decimal` simulated fills with modeled slippage/fees, stop-loss / take-profit exits, realized & mark-to-market equity tracking, and immutable `simulation_only = true` isolation.
  - Includes both a zero-network deterministic multi-cycle generator (`run_deterministic_demo_step`) and a bounded background public-market simulation worker (`spawn_public_paper_worker`) querying Hyperliquid public `/info` read-only data with exponential reconnect backoff.
- **Native Cockpit Live Overview & CLI Command (`src/cockpit.rs`, `src/cli/mod.rs`):**
  - **`Workspace::Overview`** renders **`SYNAPSE NERVOUS SYSTEM · CONNECTED PAPER PIPELINE (SIMULATION ONLY)`** for both empty and populated databases with live one-click controls (`Run Paper Cycle (Offline)`, `Start / Stop Public Paper Stream`, `Reset Paper Ledger`), truthful equity/position/closed-trade/fee/drawdown metrics, real event-driven pulsing across dragon heads on the 3D Leviathan, and recent typed bus event logs.
  - **`hydra synapse-paper`** runs deterministic multi-cycle offline simulation directly from the CLI (`--candles`, `--symbol`, `--equity`, `--json`) with no wallet keys, config files, or network connectivity required.

---

## Testing & Validation Results

- **485 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across **33 test suites** (`273` lib unit tests + integration/doc suites), plus **439 passing offline headless tests** (`cargo test --locked --all-targets`) across **32 test suites** (`227` lib unit tests + integration/doc suites, including `tests/synapse_pipeline.rs` 2/2).
- `cargo fmt --check`, `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Security, Risk Safeguards & Known Limitations

- **Simulation vs. Live Boundary:** `SynapsePipeline` and `PaperLedger` are permanently `SIMULATION_ONLY = true` and never construct signed Hyperliquid exchange actions or bypass `ENABLE_LIVE_TRADING=false` / `MODE: DORMANT`.
- **Dedicated Wallet & Vault:** **LEVIATHAN TREASURY** & **THE OFFERING** encrypt secrets via Windows DPAPI (`CryptProtectData`, current OS user scope) on Windows and enforce strict owner-only filesystem permissions (`chmod 0700` dir / `0600` file, not encryption) on Unix. Private keys never touch AI services, logs, telemetry, or network calls.
- **Live-Trading Certification Incomplete:** Unattended `AWAKENED` live execution stays locked pending complete round-trip fee/funding net settlement, external deposit/withdrawal reconciliation, stream/restart recovery, native protective exchange orders, authenticated local control IPC, and external watchdog supervision.
- **Unsigned Executables & Auto-Install Lock:** Windows executables are currently unsigned; **LEVIATHAN EVOLUTION** automatic in-place updates remain locked pending pinned release-signing key infrastructure.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.10.0-SHA256SUMS.txt`** from the Assets section below.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).
5. Optional flags: `--mode spatial|classic`, `--orbit <deg>`, `--quality ultra|cinematic`, `--head 0..6`, `--renderer glow|software`, `--fullscreen`, `--treasury`, `--updates`.

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.10.0-linux-SHA256SUMS.txt
mkdir hydra-1.10.0 && tar -xzf hydra-1.10.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.10.0
cd hydra-1.10.0 && ./hydra --version && ./hydra synapse-paper --candles 24 --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

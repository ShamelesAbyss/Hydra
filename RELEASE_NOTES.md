# HYDRA — Project Leviathan v1.7.0-alpha.29: Five-Head Project Awakening Leap (HUNTER Public Radar, FANG Bounded Grids, COGNITION Regime Inference, AEGIS Stress Matrix & ORACLE Dataset Integrity)

> **Engineering prerelease.** Trading remains disabled by default (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`). Live unattended `AWAKENED` operation stays strictly gated until authoritative financial settlement, complete recovery, authenticated control IPC, and watchdog certification pass.

## Release Assets (Attached Below on This GitHub Release)

All downloadable binaries, portable packages, source exports, and checksum manifests are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native WGPU graphical command center (automatic Glow fallback, clean-profile auto-init, visible startup diagnostics, Treasury & Offering, Leviathan Evolution updater, and all seven integrated head workspaces). Double-click directly on Windows 10/11 without Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction.
- **`hydra-1.7.0-alpha.29-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable package containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, guides, artwork, and memorial rules.
- **`hydra-1.7.0-alpha.29-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless CLI package plus guides and assets.
- **`hydra-1.7.0-alpha.29-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.7.0-alpha.29-SHA256SUMS.txt`** & **`hydra-1.7.0-alpha.29-linux-SHA256SUMS.txt`** — SHA-256 integrity manifests for all published files.

## Installation & Launch

### Windows x64 (Standalone Graphical Executable)
1. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.7.0-alpha.29-SHA256SUMS.txt`** from the Assets list below.
2. Verify integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
3. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** (Windows SmartScreen may note the engineering prerelease binary is unsigned).
4. On a clean Windows profile without an existing database, config, or wallet, Hydra initializes `%LOCALAPPDATA%\Hydra\data\hydra.db` automatically and opens directly into the living seven-headed WGPU sanctuary with trading disabled.
5. Optional terminal flags: `--renderer glow`, `--renderer software`, `--fullscreen`, `--treasury`, `--updates`.

### Linux x86_64 Headless CLI
```sh
sha256sum --check hydra-1.7.0-alpha.29-linux-SHA256SUMS.txt
mkdir hydra-alpha29 && tar -xzf hydra-1.7.0-alpha.29-x86_64-unknown-linux-gnu.tar.gz -C hydra-alpha29
cd hydra-alpha29 && ./hydra --version && ./hydra --no-banner config-check
```

## Major Project Awakening Capabilities Added in v1.7.0-alpha.29

### 1. HUNTER — Live Read-Only Public Hyperliquid Market Acquisition & Portfolio-Aware Ranking (`src/hunter.rs`, `src/hunter_ui.rs`)
- Queries Hyperliquid's public read-only `/info` endpoint (`metaAndAssetCtxs`, bounded 15s timeout & 4 MiB response cap, zero wallet credentials or signing) to observe real perpetual mark/oracle prices, 24h notional volume, open interest, 1h funding rates, impact spreads, and max leverage.
- Filters delisted/illiquid/wide-spread markets, ranks eligible instruments with configurable portfolio-aware diversification weighting for already-held symbols, and applies deterministic watchlist hysteresis.
- Integrated directly into the **HUNTER** native cockpit tab via **`Fetch Live Public Hyperliquid Radar (Read-Only)`** with an interactive live perp table.

### 2. FANG — Bounded Deterministic Anti-Martingale Grid Planner (`src/fang_planner.rs`, `src/diagnostics_ui.rs`)
- Computes deterministic arithmetic or geometric entry/take-profit grids using checked `Decimal` arithmetic strictly capped by `max_total_notional_usd`, bounded to `1..=20` rungs, and protected by a hard anti-Martingale multiplier ceiling (`0.5..=1.5x`; unbounded Martingale is structurally rejected).
- Calculates exact per-level entry prices, notionals, quantities, cumulative exposure, weighted average entry prices, monotonic take-profit targets, and a protective stop floor beyond the deepest rung, with an interactive calculator inside the **FANG** cockpit tab.

### 3. COGNITION — Zero-Lookahead Causal Market-Regime Inference (`src/cognition_shadow.rs`, `src/diagnostics_ui.rs`)
- Infers market regimes (`TRENDING_UP`, `TRENDING_DOWN`, `MEAN_REVERTING_RANGE`, `HIGH_VOLATILITY_TURBULENT`, `INSUFFICIENT_HISTORY`) deterministically from strictly completed historical candles using Kaufman efficiency ratio and per-bar log-return standard deviation without future leakage.
- Integrated into the **COGNITION** cockpit tab for interactive candle regime inspection.

### 4. AEGIS — Checked-Decimal Liquidation Distance & Multi-Shock Margin Stress Matrix (`src/portfolio_research.rs`, `src/diagnostics_ui.rs`)
- Computes conservative checked-`Decimal` gross leverage, maintenance margin requirements, estimated liquidation prices, percentage distance-to-liquidation buffers, and simultaneous adverse-shock stress scenarios (`-5%`, `-10%`, `-20%`, `-35%`) flagging maintenance-margin breaches.
- Integrated directly into the **AEGIS** cockpit tab as an interactive stress simulator (strictly read-only; never releases live AEGIS risk holds).

### 5. ORACLE — Historical Dataset Integrity & FIPS 180-4 SHA-256 Provenance Audit (`src/oracle_validation.rs`, `src/research_ui.rs`)
- Audits OHLCV candle datasets for chronology reversals, duplicate timestamps, missing bar interval gaps, OHLC inversions, and extreme single-bar spikes while generating a deterministic 64-hex FIPS 180-4 SHA-256 content fingerprint for reproducible ORACLE/GENESIS research runs.
- Integrated directly into the **ORACLE** cockpit tab via **`Audit Dataset Integrity & SHA-256`**.

## Verification, Security & Known Limitations
- **Offline tests:** **462 passing offline tests** (`cargo test --locked --features desktop-wgpu --all-targets`), adding regressions across HUNTER (loopback `metaAndAssetCtxs` & portfolio-aware ranking), FANG (long/short bounded grids & anti-Martingale enforcement), COGNITION (causal regime classification & prefix non-leakage), AEGIS (liquidation distance & multi-shock breach detection), and ORACLE (gap/inversion detection & SHA-256 stability).
- **Prior Release Verified (`v1.7.0-alpha.28`):** CI release run `45976251961` succeeded and published all 6 assets; unauthenticated HTTPS download of `HYDRA-Leviathan-Windows-x64.exe` matched SHA-256 `62886752aa37e5f6ec3244b2502f16e112bf21238a6116a90ba70ca0616e2277` and launched cleanly on an isolated empty Windows profile with zero CLI arguments.
- **Compliance & Audit:** `cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow compatibility (`--features desktop`), `cargo audit` (0 vulnerability entries, 1 Chrono maintenance warning), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Known limits:** Engineering prerelease only; Windows binaries are unsigned; automatic in-place update installation stays locked until pinned release-signing key infrastructure is configured; unattended `AWAKENED` live trading remains locked pending complete net settlement, recovery, authenticated control IPC, and watchdog certification (see `MILESTONE_MATRIX.md`).

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

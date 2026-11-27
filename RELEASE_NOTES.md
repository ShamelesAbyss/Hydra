# v1.7.0 | PROJECT LEVIATHAN

> **Public Release Foundation.** `v1.7.0` graduates HYDRA from the `v1.7.0-alpha.*` engineering series into a normal public semantic-versioning release cycle. **Important safety notice:** public distribution does **not** constitute certification of unattended live-trading safety. Trading remains disabled by default (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`), and unattended `AWAKENED` live execution remains strictly locked pending complete round-trip financial settlement, stream/restart recovery, authenticated control IPC, and watchdog certification (see `MILESTONE_MATRIX.md`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native WGPU graphical command center (automatic Glow fallback, clean-profile auto-initialization, visible startup diagnostics, PBR seven-headed Leviathan sanctuary, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.7.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.7.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive with guides and assets.
- **`hydra-1.7.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.7.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.7.0-SHA256SUMS.txt`** & **`hydra-1.7.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Visual Improvements — Rebirth of the Leviathan

- **GPU PBR Seven-Headed Dragon Sanctuary (`src/leviathan_shader.rs`):** Completely replaced the simplistic flat polygonal dragon presentation on WGPU with a real-time 2.5D physically based (PBR) signed-distance-field WGSL shader running directly on the GPU:
  - **Seven Anatomically Distinct Sculpted Heads:** Smooth-blended (`smin`) organic dragon skulls with arched supraorbital brow ridges, recessed dark orbital sockets, elongated tapered snouts, flared nostrils, articulated jaws revealing gleaming ivory-obsidian fangs, and seven distinct curved organic keratin horn phenotypes (`HUNTER`, `FANG`, `COGNITION`, `AEGIS`, `NEXUS`, `ORACLE`, `GENESIS`).
  - **Overlapping Obsidian Scales & Amethyst Sub-Scale Bioluminescence:** Analytical 3D cylindrical neck geometry covered in imbricated shield-shaped obsidian dragon scales with per-pixel PBR specular reflections, crevice ambient occlusion, dorsal ridge spines, ventral throat plates, and deep amethyst bioluminescence glowing out from beneath selected scale seams.
  - **Living Independent Attention & Tracking:** Each head exhibits independent breathing, staggered organic blinking eyelids, wet corneal eye specular glints, slit-pupil cursor/attention tracking with per-head personality weights, and genuine event-driven activation waves when real subsystem work executes.
  - **Sweeping Membranous Wings & Dynamic Amethyst Heart Point Light:** Backlit leathery obsidian-amethyst dragon wings with articulated wing bones unfurl subtly with breathing, while the central gem-cut **Amethyst Heart · Sovereign Core** acts as a live dynamic point light washing warm violet illumination upward across all seven necks and jaws, finished with volumetric mist, drifting amethyst embers, and ACES filmic tonemapping.
- **Cinematic Interface Overhaul (`src/leviathan_scene.rs`, `src/cockpit.rs`):** Crisp obsidian-glass floating head badges positioned cleanly above sculpted horns, smooth startup sanctuary reveal, unified subsystem domain labels reconciling all seven real heads alongside **LEVIATHAN TREASURY** and the central **SOVEREIGN CORE**, and preserved Glow OpenGL fallback for systems without WGPU.

## Performance Optimizations & Adaptive Rendering

- **Quality Presets & Adaptive LOD (`Low`, `Medium`, `High`, `Ultra` + `Adaptive LOD`):** Tight per-head and per-neck bounding-box culling keeps GPU overhead low on RTX 4070-class hardware (`High`/`Ultra` at 120–144 FPS with ~1.3 ms UI CPU build time), while `Adaptive LOD` monitors rolling frame cadence and smoothly steps shader detail down on weaker GPUs so trading, research, and inference workloads are never starved.

## New Features & Engineering Improvements

- **Public Release Cycle & Release Manifests (`.github/workflows/release.yml`, `src/evolution.rs`):** `v1.7.0` publishes as a normal public release (`draft: false`, `prerelease: false`, `make_latest: true`) with automated `/releases/latest` verification and structured `hydra-1.7.0-release-manifest.json` generation. Future internal prereleases are explicitly tagged `--latest=false` so they never replace the Latest public release.
- **LEVIATHAN EVOLUTION Native Update Manager:** Integrated top-bar **`CHECK FOR UPDATES`** control and **`Updates`** (`Alt+U` / `--updates`) workspace supporting `Stable` (default on `v1.7.0`) and `Development` channels, SemVer 2.0.0 comparison, FIPS 180-4 SHA-256 verification, `secp256k1` signed-manifest provenance checks, trading-safe maintenance gates, verified backups, and rollback.
- **Multi-Head Quantitative Intelligence (Integrated in Single Window):**
  - **HUNTER:** Live read-only public Hyperliquid `metaAndAssetCtxs` market radar with portfolio-aware diversification ranking.
  - **FANG:** Checked-Decimal bounded deterministic arithmetic/geometric grid planner with hard cumulative exposure caps and anti-Martingale multiplier limits (`<= 1.5x`).
  - **COGNITION:** Zero-lookahead causal market-regime inference (`TRENDING_UP`, `TRENDING_DOWN`, `MEAN_REVERTING_RANGE`, `HIGH_VOLATILITY_TURBULENT`) alongside strict prior-outcome shadow evaluation.
  - **AEGIS:** Persistent exposure reservations & drawdown breakers plus checked-Decimal liquidation-distance and multi-shock margin stress simulator (`-5%` to `-35%`).
  - **ORACLE & GENESIS:** Historical OHLCV dataset integrity & FIPS 180-4 SHA-256 provenance auditing, rolling held-out cost scenarios, resumable generation checkpoints, and frozen-frontier assessment.
- **Startup Database Integrity Diagnostics (`src/startup_diagnostics.rs`):** Non-destructive read-only SQLite `PRAGMA quick_check(1)` logged on startup alongside clean-profile `%LOCALAPPDATA%\Hydra` initialization, `catch_unwind` WGPU→Glow fallback, crash reports, and visible Win32 error dialogs.
- **Road to HYDRA 2.0 (`ROADMAP_V2.md`):** Established the iterative `v1.x` progression plan toward the generational `v2.0.0` milestone.

## Security & Risk-Management Safeguards

- Dedicated-wallet-only **LEVIATHAN TREASURY** & **THE OFFERING** with Windows DPAPI (`CryptProtectData`) / Unix `0700`/`0600` local credential encryption, zeroizing secret buffers, masked UI inputs, and verbatim capital-loss acknowledgment.
- Private signing keys stay strictly on the local machine and never touch AI services, logs, telemetry, or network calls. No automatic withdrawal or arbitrary transfer capability exists.
- Permanent automated GitHub compliance gate (`verify-github-compliance.ps1`) and permanent memorial verification (`verify-memorial.ps1`) enforced in CI.

## Testing & Validation Results

- **463 passing offline tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across 32 test binaries, covering the new PBR shader uniform alignment/sanitization, `Ultra` quality & Adaptive LOD stepping, public vs. prerelease SemVer channel resolution, SQLite startup health checks, HUNTER loopback public acquisition, FANG bounded grids, COGNITION causal regime inference, AEGIS margin stress, ORACLE dataset integrity, and all existing execution/ledger/signing invariants.
- `cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow compilation (`--features desktop --all-targets`), `cargo audit` (0 vulnerability entries, 1 known Chrono maintenance warning), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- Independently verified native Windows 11 RTX 4070 WGPU rendering and clean-profile zero-argument launch. No real-money orders, live wallet credentials, or transfers were used.

## Known Limitations

- **Live-Trading Certification Incomplete:** Unattended `AWAKENED` live execution remains locked pending complete round-trip fee/funding net settlement, external deposit/withdrawal reconciliation, broad stream/restart recovery, authenticated local control IPC, and external watchdog supervision.
- **Unsigned Executables & Auto-Install Lock:** Windows executables are currently unsigned (SmartScreen may display an unsigned-app notice). Automatic in-place update installation in **LEVIATHAN EVOLUTION** stays strictly locked until dedicated release-signing key infrastructure is configured.
- **Platform Scope:** Windows x64 is the verified native graphical target; Linux x64 ships the verified headless/Ratatui engine and CLI.

## Installation & Upgrade Instructions

### Fresh Installation (Windows x64)
1. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.7.0-SHA256SUMS.txt`** from the Assets section below.
2. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
3. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the reborn seven-headed WGPU sanctuary (defaults safely to `MODE: DORMANT`).
4. Optional flags: `--quality ultra`, `--renderer glow`, `--renderer software`, `--fullscreen`, `--treasury`, `--updates`.

### Upgrading From `v1.7.0-alpha.*`
1. Ensure Hydra is in `DORMANT` or `SUSPENDED` mode with no active engine session running.
2. Download and verify **`HYDRA-Leviathan-Windows-x64.exe`** from the Assets section below and replace your previous executable.
3. All local databases (`%LOCALAPPDATA%\Hydra\data\hydra.db`), OS-protected Treasury credentials, and risk policies are automatically preserved untouched. Launch boots safely in `MODE: DORMANT`.

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.7.0-linux-SHA256SUMS.txt
mkdir hydra-1.7.0 && tar -xzf hydra-1.7.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.7.0
cd hydra-1.7.0 && ./hydra --version && ./hydra --no-banner config-check
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

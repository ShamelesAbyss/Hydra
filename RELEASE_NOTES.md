# v1.9.0 | PROJECT SYNAPSE — SPATIAL CONSCIOUSNESS

> **Project Synapse Phase I — Spatial Consciousness.** Building directly on `v1.8.0 | PROJECT INCARNATION`, `v1.9.0` completes the final authorized graphical architecture upgrade: floating world-space-anchored holographic workspaces, 360° orbital sanctuary exploration with collision avoidance, seven personality-paced aware heads with natural whole-body compensation, and an interchangeable **`SPATIAL | CLASSIC`** UI mode. **Important safety notice:** public distribution does **not** constitute certification of unattended live-trading safety. Trading remains disabled by default (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`), and unattended `AWAKENED` execution stays locked pending complete settlement, recovery, authenticated IPC, and watchdog certification (`MILESTONE_MATRIX.md`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness command center (automatic Glow fallback, clean-profile auto-initialization, visible startup diagnostics, 360° orbital 96-bone Leviathan sanctuary, floating head-anchored holographic workspaces + `CLASSIC` fallback, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.9.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.9.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive with guides and assets.
- **`hydra-1.9.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.9.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.9.0-SHA256SUMS.txt`** & **`hydra-1.9.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Features — Spatial Consciousness Architecture

- **Hybrid World-Space + Native `egui` Spatial Interface (`src/leviathan_spatial.rs`, `src/leviathan_layout.rs`, `src/cockpit.rs`):**
  - Floating obsidian-glass panels (`Head(0..=6)` plus global `Overview`, `Treasury`, `Updates`, `Positions`, `Timeline`, `Source`, and `Spatial` panels) anchor smoothly in 3D world space to their respective dragon skulls or Amethyst Heart and continuously project upright, unmirrored, back-to-front depth-ordered interactive native `egui` surfaces (`ScrollArea`, text inputs, buttons, sliders, pinning, collapse/expand, and 3D drag-to-reposition using `LeviathanCamera::unproject_drag_delta`), keeping all controls sharp and responsive while preserving Ashlan's bottom memorial plaque and the fixed top safety bar.
  - Luminous holographic energy conduits (`paint_spatial_conduits`) link each emerged workspace cleanly back to its dragon head or Sovereign Heart.
  - Switch seamlessly at runtime between **`SPATIAL`** (immersive full-sanctuary floating panels) and **`CLASSIC`** (docked 3-column + lower analytical drawer) modes without restarting; preferences persist locally to `%LOCALAPPDATA%\Hydra\spatial_prefs.json`.
- **360° Orbital Sanctuary Exploration & Outer Colonnade (`src/leviathan_3d_math.rs`, `src/leviathan_mesh.rs`):**
  - Supports full 360° horizontal orbit (`--orbit <deg>` or primary drag), bounded vertical orbit (`-20°` to `+54°`), scroll zoom, secondary/middle-drag panning, `Reset Cam` / double-click / `Esc` overview return, and explicit camera collision avoidance (clearing the dais floor and thorax collision sphere).
  - Expands the gothic obsidian cathedral colonnade to an outer `ring_r = 11.2` perimeter and adds a sculpted dorsal thoracic carapace ridge with 5 spine plates so side and rear 360° orbit views remain unobstructed and anatomically coherent.
- **Creature Awareness, Seven Personalities & Whole-Body Compensation (`src/leviathan_awareness.rs`):**
  - Independent critically damped spring kinematics with anti-thrashing dwell hysteresis (`2.6–7.4s`) tailor responsiveness per personality (`HUNTER`, `FANG`, `COGNITION`, `AEGIS`, `NEXUS`, `ORACLE`, `GENESIS`).
  - Opening or dragging a head's floating panel naturally turns that head and gaze toward its panel while applying restrained thoracic rotation (`±16°`), asymmetric wing balance, opposing 6-bone tail sweep, and pairwise skull collision separation.
  - Idle glances remain clearly distinct from event-driven activity pulses and alert jaw/shield states.
- **Permanent Fixed Safety Controls:** `MODE`, Treasury readiness, `EMERGENCY SUSPEND`, `LEVIATHAN TREASURY`, and `CHECK FOR UPDATES` remain pinned in the top command bar across every orbit angle, zoom level, and interface mode (`SPATIAL` or `CLASSIC`). Private credential fields stay strictly inside native masked `egui` widgets and never enter persistent textures, logs, or telemetry.

---

## Performance & Compatibility Verification

- **RTX 4070 Laptop GPU (`--release --features desktop-wgpu`, `1600 × 960`, `--quality ultra`, offline dormant DB):**
  - Cold-start `--release` captures recorded live WGPU 3D PBR rendering with `TIMESTAMP_QUERY` GPU pass durations of **`0.14–0.25 ms` GPU** (overview/focus/treasury/updates) and **`0.24 ms` GPU (`UI 300.3/s · cpu 0.52 ms`)** during 165° rear-orbit inspection (`synapse-orbit-rear-v2.png`).
  - Steady-state interactive `WGPU 3D PBR` dev run in `SPATIAL` mode measured **`UI 144.3/s · cpu 0.51 ms · 0.53 ms GPU`**.
- **Fallback Preservation:** `--features desktop` (`--renderer glow`) compiles with zero warnings and passes live startup verification (`--renderer glow`), retaining classic compatibility rendering and `catch_unwind` recovery.

---

## Testing & Validation Results

- **478 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across 32 test suites, plus **432 passing offline headless tests** (`cargo test --locked --all-targets`) across 31 test suites.
- `cargo fmt --check`, `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, Glow runtime startup, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Security, Risk Safeguards & Known Limitations

- Dedicated-wallet-only **LEVIATHAN TREASURY** & **THE OFFERING** encrypt secrets via Windows DPAPI (`CryptProtectData`, current OS user scope) on Windows and enforce strict owner-only filesystem permissions (`chmod 0700` dir / `0600` file, not encryption) on Unix. Private keys never touch AI services, logs, telemetry, or network calls.
- **Live-Trading Certification Incomplete:** Unattended `AWAKENED` live execution stays locked pending complete round-trip fee/funding net settlement, external deposit/withdrawal reconciliation, stream/restart recovery, authenticated local control IPC, and external watchdog supervision.
- **Unsigned Executables & Auto-Install Lock:** Windows executables are currently unsigned; **LEVIATHAN EVOLUTION** automatic in-place updates remain locked pending pinned release-signing key infrastructure.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.9.0-SHA256SUMS.txt`** from the Assets section below.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).
5. Optional flags: `--mode spatial|classic`, `--orbit <deg>`, `--quality ultra|cinematic`, `--head 0..6`, `--renderer glow|software`, `--fullscreen`, `--treasury`, `--updates`.

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.9.0-linux-SHA256SUMS.txt
mkdir hydra-1.9.0 && tar -xzf hydra-1.9.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.9.0
cd hydra-1.9.0 && ./hydra --version && ./hydra --no-banner config-check
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

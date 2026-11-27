# v1.8.0 | PROJECT INCARNATION

> **The Living Sovereign — Articulated 3D Skeletal & PBR WGPU Upgrade.** `v1.8.0` advances HYDRA's native desktop sanctuary to a true 96-bone articulated 3D skeletal and procedural mesh PBR renderer (`WGPU 3D PBR`), while keeping all financial safeguards intact. **Important safety notice:** public distribution does **not** constitute certification of unattended live-trading safety. Trading remains disabled by default (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`), and unattended `AWAKENED` live execution remains strictly locked pending complete round-trip financial settlement, stream/restart recovery, authenticated control IPC, and watchdog certification (see `MILESTONE_MATRIX.md`).

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

All compiled executables, portable archives, source exports, checksums, and structured release metadata are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native WGPU 3D PBR command center (automatic Glow fallback, clean-profile auto-initialization, visible startup diagnostics, 96-bone articulated seven-headed Leviathan sanctuary, Treasury & Offering, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-1.8.0-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-1.8.0-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive with guides and assets.
- **`hydra-1.8.0-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.8.0-release-manifest.json`** — Machine-readable release manifest recording exact version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-1.8.0-SHA256SUMS.txt`** & **`hydra-1.8.0-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch.

---

## Major Visual Improvements — Project Incarnation (`WGPU 3D PBR`)

- **96-Bone Articulated 3D Leviathan Skeleton (`src/leviathan_3d_math.rs`):** Hierarchical 96-bone kinematics animating a sculpted central thorax, beating Amethyst Heart, clawed forelimbs, flapping backlit wings, 6-bone coiled tail, seven 6-bone serpentine necks, seven tracking skulls, and articulated lower jaws with cursor-tracking eyes and staggered blinks.
- **Procedural 3D Dragon Anatomy & Gothic Sanctuary (`src/leviathan_mesh.rs`):** Builds skinned 96-byte GPU meshes for the armored chest and sternal rib-prongs, faceted outer + star-core Amethyst Heart, seven neck root conduits, two-tier octagonal obsidian dais, physical dorsal neck spines, seven anatomically distinct dragon skulls with overhanging brow plates, twin nasal spurs, curved ivory fangs, recessed 3D reptilian eyeballs, unique keratin horn crowns (`HUNTER`, `FANG`, `COGNITION`, `AEGIS`, `NEXUS`, `ORACLE`, `GENESIS`), AEGIS 3D hexagonal shield ring, ORACLE 3D astrolabe rings, and six fluted gothic sanctuary pillars and pointed arches.
- **4-Pass Native WGPU 3D PBR, Shadow Mapping, HDR Bloom & Live GPU Telemetry (`src/leviathan_renderer_3d.rs`, `src/desktop_renderer.rs`):**
  - **Shadow Pass:** `1536 × 1536` `Depth32Float` directional shadow map with `3 × 3` PCF filtering.
  - **Opaque + Translucent 3D PBR Passes:** Cook-Torrance GGX microfacet lighting, two-row overlapping diamond obsidian scales with amethyst sub-scale bioluminescence seams, anisotropic irises with slit pupils, volumetric translucent wing membranes and sanctuary fields, plus gothic cathedral mist and starfield.
  - **HDR Bloom & ACES Composite:** Half-resolution separable HDR bloom (`Rgba16Float`) and ACES filmic tonemapping.
  - **`SceneQuality::Cinematic` & Live GPU Pass Timing:** Adds the `Cinematic` quality tier (`Low`, `Medium`, `High`, `Ultra`, `Cinematic`) and opts into `Features::TIMESTAMP_QUERY` when supported by the host adapter to display real GPU pass timing directly in the command strip (`0.27–0.32 ms GPU` on RTX 4070 `Ultra` `1600 × 960`).
- **Heroic 3/4 Head Focus & Integrated Workspace Visualizations (`src/leviathan_scene.rs`, `src/cockpit.rs`, `src/bin/hydra-cockpit.rs`, `src/hunter_ui.rs`, `src/diagnostics_ui.rs`):** Smooth camera flight between Sovereign Overview and 3/4 close-up Focused Head inspection (`Esc` or button to return, `--head <0..=6>` CLI flag, non-overlapping bottom obsidian memorial plaque in close-ups), Overview 7-head readiness strip + interactive equity chart crosshair, NEXUS lifecycle pipeline summary, and horizontal analytical bar charts in HUNTER, FANG, and AEGIS workspaces.

---

## Performance & Compatibility Verification

- **RTX 4070 Laptop GPU (`--release --features desktop-wgpu`, `1600 × 960`, `--quality ultra`, offline dormant DB):**
  - Sovereign Overview (`leviathan-awakened-wgpu.png`): **`144.9 UI/s`**, **`0.66 ms` CPU UI build**, **`0.32 ms` GPU pass** (`WGPU 3D PBR`).
  - Focused Head (`leviathan-focused-head.png`, FANG): **`143.1 UI/s`**, **`0.68 ms` CPU UI build**, **`0.27 ms` GPU pass**.
  - Treasury & Updates modes: **`144.2–144.4 UI/s`**, **`0.64–0.77 ms` CPU**, **`0.28 ms` GPU pass**.
  - `SceneQuality::Cinematic` debug run: **`144.1 UI/s`**, **`0.94 ms` CPU**, **`0.38 ms` GPU pass**.
- **Fallback Preservation:** `--features desktop` (`--renderer glow`) compiles cleanly with zero warnings and retains the OpenGL fallback alongside `catch_unwind` recovery.

---

## Testing & Validation Results

- **470 passing offline tests** (`cargo test --locked --features desktop-wgpu --all-targets`) across 32 test suites, plus **432 passing offline headless tests** (`cargo test --locked --all-targets`) across 31 test suites.
- `cargo fmt --check`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `cargo check --locked --features desktop --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- No real-money orders, live wallet credentials, or transfers were used.

## Security, Risk Safeguards & Known Limitations

- Dedicated-wallet-only **LEVIATHAN TREASURY** & **THE OFFERING** (Windows DPAPI / Unix `0600` encryption), strictly local private signing keys, disabled automatic withdrawals/transfers, and `verify-github-compliance.ps1` + `verify-memorial.ps1` enforcement remain intact.
- **Live-Trading Certification Incomplete:** Unattended `AWAKENED` live execution remains locked pending complete round-trip fee/funding net settlement, external deposit/withdrawal reconciliation, broad stream/restart recovery, authenticated local control IPC, and external watchdog supervision.
- **Unsigned Executables & Auto-Install Lock:** Windows executables are currently unsigned; **LEVIATHAN EVOLUTION** automatic in-place updates remain locked pending pinned release-signing key infrastructure.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.8.0-SHA256SUMS.txt`** from the Assets section below.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).
5. Optional flags: `--quality ultra`, `--quality cinematic`, `--head 0..6`, `--renderer glow`, `--renderer software`, `--fullscreen`, `--treasury`, `--updates`.

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-1.8.0-linux-SHA256SUMS.txt
mkdir hydra-1.8.0 && tar -xzf hydra-1.8.0-x86_64-unknown-linux-gnu.tar.gz -C hydra-1.8.0
cd hydra-1.8.0 && ./hydra --version && ./hydra --no-banner config-check
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

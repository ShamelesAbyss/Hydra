# HYDRA — Project Leviathan v1.7.0-alpha.28: Leviathan Evolution Native Updater, Restored Obsidian-Amethyst Identity & Standalone Windows Release

> **Engineering prerelease.** Trading remains disabled by default (`MODE: DORMANT`; `ENABLE_LIVE_TRADING=false`). Live unattended `AWAKENED` operation stays strictly gated until authoritative financial settlement, complete recovery, authenticated control IPC, and watchdog certification pass.

## Release Assets (Attached Below on This GitHub Release)

All downloadable binaries, packages, source exports, and checksum manifests are attached directly to this GitHub Release:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Primary standalone Windows x64 native WGPU graphical command center (automatic Glow fallback, clean-profile auto-init, visible startup diagnostics, Treasury & Offering, Leviathan Evolution updater). Double-click directly on Windows 10/11 without Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction.
- **`hydra-1.7.0-alpha.28-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable package containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, guides, artwork, and memorial rules.
- **`hydra-1.7.0-alpha.28-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless CLI package plus guides and assets.
- **`hydra-1.7.0-alpha.28-source.zip`** — Exact-tag source export (excluding inherited legacy binaries).
- **`hydra-1.7.0-alpha.28-SHA256SUMS.txt`** & **`hydra-1.7.0-alpha.28-linux-SHA256SUMS.txt`** — SHA-256 integrity manifests for all published files.

## Installation & Launch

### Windows x64 (Standalone Graphical Executable)
1. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-1.7.0-alpha.28-SHA256SUMS.txt`** from the Assets list below.
2. Verify integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
3. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** (Windows SmartScreen may note the engineering prerelease binary is unsigned).
4. On a clean Windows profile without an existing database, config, or wallet, Hydra safely initializes an empty dormant local history at `%LOCALAPPDATA%\Hydra\data\hydra.db` and launches into the living seven-headed WGPU sanctuary with trading disabled.
5. Optional terminal flags: `--renderer glow`, `--renderer software`, `--fullscreen`, `--treasury`, `--updates`.

### Linux x86_64 Headless CLI
```sh
sha256sum --check hydra-1.7.0-alpha.28-linux-SHA256SUMS.txt
mkdir hydra-alpha28 && tar -xzf hydra-1.7.0-alpha.28-x86_64-unknown-linux-gnu.tar.gz -C hydra-alpha28
cd hydra-alpha28 && ./hydra --version && ./hydra --no-banner config-check
```

## Major Changes in v1.7.0-alpha.28

### 1. LEVIATHAN EVOLUTION — Integrated Native Update Manager (`src/evolution.rs`, `src/evolution_ui.rs`)
- **Unified Cockpit Integration:** Added a prominent **`CHECK FOR UPDATES`** button in the top command bar and a dedicated **`LEVIATHAN EVOLUTION`** (`Updates` / `Alt+U` / `--updates`) workspace tab inside the existing single unified window, pulsing the **GENESIS** head during real update checks and package downloads.
- **GitHub Releases API & SemVer 2.0.0 Resolution:** Supports **Development (Prerelease)** and **Stable** channels, skips drafts, selects the correct OS/architecture package (`HYDRA-Leviathan-Windows-x64.exe` on Windows x64, `tar.gz` on Linux x64), displays release notes, and never misuses `/releases/latest` for prereleases.
- **Pure-Rust FIPS 180-4 SHA-256 & `secp256k1` Signed Provenance:** Streams downloads with real byte progress and cancellation, verifies downloaded packages byte-for-byte against `SHA256SUMS.txt`, and supports cryptographic `secp256k1` ECDSA signed-manifest verification. Automatic installation stays strictly locked until pinned release-signing key infrastructure is configured.
- **Trading-Safe Maintenance Gate, Staging, Backup & Rollback:** Refuses updates while `AWAKENED`/`GUARDIAN` mode is active, positions are open, or execution intents remain unresolved. Never touches `%LOCALAPPDATA%\Hydra\data\`, Treasury credentials, or risk policies, creates SHA-256-verified executable backups for rollback, logs durable update diagnostics, and never auto-resumes live trading after restart.

### 2. Permanent GitHub Compliance & Restored README Presentation
- **Restored Obsidian-Amethyst Identity:** Upgraded `docs/assets/leviathan-banner.svg` and `docs/assets/architecture.svg` to match the living seven-headed obsidian-amethyst dragon sanctuary, Amethyst Heart, Treasury, and Evolution architecture.
- **Permanent Automated Compliance Gate (`.github/scripts/verify-github-compliance.ps1`, `.cursor/rules/github-compliance.mdc`):** Enforces across both CI workflows (`verify.yml` and `release.yml`) that `README.md` and repository documentation never contain direct executable/archive/checksum download links, `/releases/download/` URLs, shorteners, or mirrors, and verifies every `docs/assets/*` image path exists with exact casing.

### 3. Standalone Distribution & Clean-Profile Hardening (Carried Forward & Published)
- Ships the clean-profile `%LOCALAPPDATA%\Hydra` bootstrap, global panic hook, readable local crash logs, visible Win32 error dialog (`MessageBoxW`), `catch_unwind` WGPU→Glow fallback, and `1600 × 960` monitor-safe default window geometry introduced in `v1.7.0-alpha.27`, now published end-to-end via `.github/workflows/release.yml`.

## Verification, Security & Known Limitations
- **Offline tests:** **457 passing offline tests** (`cargo test --locked --features desktop-wgpu --all-targets`), covering NIST FIPS 180-4 SHA-256 vectors, SemVer ordering, channel filtering, `secp256k1` signed-manifest verification and tamper rejection, maintenance gate enforcement, unsafe path rejection, and loopback download/staging/backup/rollback roundtrips.
- **Checks & Audit:** `cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow compatibility (`--features desktop`), `cargo audit` (0 vulnerability entries, 1 Chrono maintenance warning), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Known limits:** Engineering prerelease only; Windows binaries are unsigned; automatic in-place update installation stays locked until pinned release-signing key infrastructure is configured; unattended `AWAKENED` live trading remains locked pending full settlement, recovery, authenticated control IPC, and watchdog certification (see `MILESTONE_MATRIX.md`).

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜

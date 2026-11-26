# HYDRA — Project Leviathan v1.7.0-alpha.26: Living Leviathan Sanctuary, WGPU Shader & Leviathan Treasury

> **Engineering prerelease.** Live-trading readiness remains incomplete (`ENABLE_LIVE_TRADING=false` by default). Unattended `AWAKENED` live execution remains gated until authoritative financial settlement, complete recovery, authenticated control IPC, and watchdog certification pass.

## Visual & Architectural Transformation

![Awakened seven-headed Leviathan WGPU cockpit](https://raw.githubusercontent.com/ShamelesAbyss/Hydra/v1.7.0-alpha.26/docs/assets/leviathan-awakened-wgpu.png)

![LEVIATHAN TREASURY and THE OFFERING dedicated wallet onboarding inside the unified window](https://raw.githubusercontent.com/ShamelesAbyss/Hydra/v1.7.0-alpha.26/docs/assets/leviathan-treasury-offering.png)

### Unified Living Seven-Headed Command Center
- **GPU-Accelerated WGPU Volumetric Sanctuary (`src/leviathan_shader.rs`):** Custom WGSL backdrop rendering obsidian-to-indigo abyssal depth, drifting fBM nebula wisps, sacred astrolabe/altar geometry, dual-peak Amethyst Heart bloom, and reactive per-head light halos, with a layered procedural Glow fallback (`--renderer glow`).
- **Living Anatomy & Seven Distinct Dragon Phenotypes (`src/leviathan_scene.rs`):** Smooth physiological breathing expands the thoracic carapace and lifts sweeping amethyst-veined cybernetic dragon wings; prime-staggered organic eyelid blinking animates multi-layered slit-pupil eyes; serpentine multi-harmonic sway ripples independently through all seven necks; and the central **Amethyst Heart** beats with a dual-peak cardiac pulse, gem-cut refraction facets, caustic star-core, and seven directional pylon sockets physically connecting every neck artery.
- **Obsidian-Glass Choreography (`src/cockpit.rs`):** Interactive seven-head Intelligence rail cards with bidirectional hover-to-head highlighting, structured Protection/Execution evidence tiles and Energy Signal Key, chamfered panels with luminous corner brackets, and smooth `Compact` / `Balanced` / `Expand` analytical drawer transitions alongside direct drag resizing.
- **Measured WGPU Performance (Windows 11, RTX 4070 Laptop GPU, 2560 × 1440, High quality):** Observed `UI 111–142/s` with `1.25–1.96 ms` median UI-build CPU time (GPU execution timing remains unmeasured).

### Leviathan Treasury & The Offering
- **Dedicated Wallet Vault (`src/treasury.rs`, `src/treasury_ui.rs`):** Exclusive **LEVIATHAN TREASURY** tab (`Alt+T` or `--treasury`) inside the single unified window with **THE OFFERING** ceremonial onboarding altar, explicit warning never to connect a primary personal wallet, Windows DPAPI (`CryptProtectData`, current OS user) / Unix `0700`/`0600` credential storage, zeroizing secret buffers, masked UI inputs, local integrity verification, credential replacement, and overwrite-before-delete removal. Private keys never touch logs, telemetry, or network calls.
- **Read-Only Public-Address Hyperliquid Verification:** Queries Hyperliquid `/info` using only public addresses to verify master/agent signer role, standard perp (`disabled`) collateral mode, equity, available collateral, margin used, open positions, and unrealized PnL. Genuine validation illuminates the Amethyst Heart and animates conduits to AEGIS and NEXUS.
- **12-Metric Truthful Portfolio Surface & Six Operational Modes:** Displays Starting Capital, Current Account Equity, Available Collateral, Capital Allocated, Unrealized PnL, Verified Realized Net PnL, Trading Fees, Funding Payments, Drawdown, Deposits/Withdrawals (explicitly separated from trading profits), Active Risk Limits, and Unresolved Financial Events without fabricating unavailable values. Adds `DORMANT`, `OBSERVATION`, `SIMULATION`, gated `AWAKENED`, `GUARDIAN`, and `SUSPENDED` modes plus a prominent one-click `EMERGENCY SUSPEND` control. No automatic withdrawal or arbitrary transfer capability exists.

## Installation & Launch

### Windows x86_64 Portable (`hydra.exe` + WGPU `hydra-cockpit.exe`)
1. Download `hydra-1.7.0-alpha.26-x86_64-pc-windows-msvc.zip` and `hydra-1.7.0-alpha.26-SHA256SUMS.txt`.
2. Verify checksum in PowerShell:
   ```powershell
   Get-FileHash .\hydra-1.7.0-alpha.26-x86_64-pc-windows-msvc.zip -Algorithm SHA256
   ```
3. Extract into a writable folder (keep `config/` beside the executables) and launch:
   ```powershell
   .\hydra.exe --version
   .\hydra.exe init-history --db data/hydra.db
   .\hydra-cockpit.exe --db data/hydra.db
   # Optional: open directly to LEVIATHAN TREASURY / THE OFFERING:
   .\hydra-cockpit.exe --db data/hydra.db --treasury
   # Optional: force OpenGL compatibility fallback:
   .\hydra-cockpit.exe --db data/hydra.db --renderer glow
   ```

### Linux x86_64 Headless CLI (`hydra`)
```sh
sha256sum --check hydra-1.7.0-alpha.26-linux-SHA256SUMS.txt
mkdir hydra-alpha26 && tar -xzf hydra-1.7.0-alpha.26-x86_64-unknown-linux-gnu.tar.gz -C hydra-alpha26
cd hydra-alpha26 && ./hydra --version && ./hydra --no-banner config-check
```

## Verification & Known Limits
- **449 offline tests pass** (`cargo test --locked --features desktop-wgpu --all-targets`), plus Glow-only desktop check, `cargo fmt --check`, `cargo clippy --locked --all-targets`, and zero dependency vulnerabilities (`cargo audit`, 1 existing Chrono maintenance warning).
- Unsigned Windows executables; live unattended operation remains locked pending complete net settlement, external deposit/withdrawal ledger reconciliation, full stream/restart recovery, authenticated control IPC, and watchdog supervision (see `MILESTONE_MATRIX.md`).
- **Permanent Memorial:** Ashlan Haley Rapponotti's dedication (`June 21, 2003 – November 13, 2026`) is preserved beneath the Amethyst Heart, verbatim at the end of `README.md`, in `.cursor/rules/permanent-memorial.mdc`, and verified automatically in CI and packages.

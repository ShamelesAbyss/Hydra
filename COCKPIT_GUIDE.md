# Native cockpit development build

Build on a workstation with native graphics support:

```powershell
cargo build --locked --features desktop --bin hydra-cockpit
.\target\debug\hydra-cockpit.exe --db data/hydra.db
```

The cockpit reads a local Hydra SQLite database and displays recorded positions,
execution intents, signals, equity, risk state, and ledger evidence where those
tables exist. The seven subsystem views distinguish missing data from observed
state. The executable opens the database read-only and can run independently of
the trading engine.

For a versioned JSON snapshot without a graphical session:

```text
cargo run --locked --bin hydra -- telemetry --db data/hydra.db --limit 100
```

The CLI bypasses strategy configuration and database initialization for this
command. Treat snapshots as private account data. Recorded equity and persisted
session state may lag the exchange.

On first launch, Initialize empty local history creates a new database at the entered path. Existing files are preserved. This explicit setup action runs locally without credentials or exchange access.

## Research workspaces

Select Oracle or Genesis, enter local strategy TOML and candle JSON paths, and optionally supply an options JSON file. Copy-default-options buttons expose the current schema. One background job is admitted at a time. Results display their simulation model and can be copied as JSON. These workspaces operate without a connected engine database.

## HUNTER market radar (alpha.21)

The HUNTER workspace loads local market-snapshot JSON independently of engine
history. Copy an empty template, populate it with your own observations and run
the bounded background evaluation. Search results, inspect rejection reasons,
filter selected members and explore actual depth/spread observations. This is
an offline research watchlist with no trading or capital-allocation authority.
Alt+1 through Alt+7 selects a subsystem. See [HUNTER.md](HUNTER.md).

## ORACLE held-out comparison (alpha.22)

Toggle **Rolling held-out / cost scenarios** in ORACLE. Supply strategy, candle
and optional validation-options paths. The background job compares fixed parameters
across disjoint test intervals and explicit fee/slippage scenarios. Expand each
scenario/fold for original simulated equity and trades; copy the complete report.
Capital resets per fold. No parameter training, automated deployment, continuous
portfolio compounding or real performance is implied. See [BACKTESTING.md](BACKTESTING.md).

## FANG / COGNITION / AEGIS diagnostics (alpha.23)

Alt+2/3/4 opens typed offline analytical workspaces independently of engine history.
Copy an empty template, supply a local JSON path and evaluate off the render thread.
FANG shows selections/rejection explanations and notional proportions; its EMA mode
shows actual completed-bar bands and bounded price-only suggestions. COGNITION shows
prior-outcome shadow comparisons and actual approximate confidence intervals. AEGIS
shows exposure, breaches/unknowns and aligned historical pair correlations. Recorded
engine history remains under a separate collapsing panel. No controller writes,
authorization or live-risk changes are available. See ANALYTICS.md.

## GENESIS checkpoint steps (alpha.24)

Enable **Resumable generation steps**, select an optional input checkpoint and a
new output filename, set the complete-generation budget and run the worker. The
last successfully saved checkpoint, generation count and completion state are
shown with JSON copy. Use that file as the next input with a new output path.
Existing files are preserved, failed writes never appear as successful saves,
and the normal full optimizer remains available. See OPTIMIZATION.md.

## GENESIS frozen-frontier assessment (alpha.25)

Select the held-out assessment option, training candles, a completed matching-build
checkpoint, later held-out data and optional validation options. The background
worker uses the fixed training frontier; expand each candidate for its actual
simulated scenario/fold reports. No automatic deployment or unseen-data guarantee
is implied. See OPTIMIZATION.md and the upcoming VISUAL_CAMPAIGN.md.


## Unified Leviathan Spatial & Classic Cockpit (v1.9.0)

HYDRA supports two interchangeable interface modes in the same window via the top-bar `SPATIAL | CLASSIC` switcher (or `--mode spatial|classic` on launch):

- **`SPATIAL` mode:** Full-sanctuary 3D canvas with floating obsidian-glass holographic panels anchored in world space to each dragon head or the Amethyst Heart (`Paint` conduits + depth-ordered native interactive `egui` surfaces). Drag panel headers to reposition them in 3D space (the corresponding dragon head turns to acknowledge the moved panel), orbit 360° with primary drag, pan with secondary/middle drag, scroll to zoom, press `Esc` or click `Reset Cam` / double-click background to return to Sovereign Overview, or pin multiple workspaces simultaneously. Preferences persist to `%LOCALAPPDATA%\Hydra\spatial_prefs.json`.
- **`CLASSIC` mode:** Docked three-column cockpit (`Intelligence` left rail, 360° orbitable 3D sanctuary center, `Protection` right rail) plus resizable lower inline workspace drawer (`Compact`, `Balanced`, `Expand`).

Select any dragon head directly in 3D space, via the dock/rail, or with `Alt+1` through `Alt+7`. Press `Alt+T` (`--treasury`) for **LEVIATHAN TREASURY**, `Alt+U` (`--updates`) for **LEVIATHAN EVOLUTION**, or open `Graphics / Spatial` to adjust quality (`Low`..`Cinematic`), spatial intensity, panel opacity, creature attention, body responsiveness, orbit sensitivity, reduced motion, and `Cinematic Auto-Orbit`.

## Leviathan Treasury & The Offering

The **LEVIATHAN TREASURY** tab inside the unified window is the exclusive surface for configuring a dedicated Hyperliquid trading wallet (never connect a primary personal wallet):

- **THE OFFERING:** Enter the dedicated wallet public address and masked private signing key, acknowledge the verbatim capital-loss disclosure, and save credentials locally via Windows DPAPI encryption (`CryptProtectData`, current OS user scope) on Windows or strict owner-only filesystem permissions (`chmod 0700` directory / `0600` file, not encryption) on Unix. Private keys are zeroized from UI buffers immediately after saving and are never logged, placed into GPU textures, or transmitted over the network.
- **Hyperliquid Verification:** Read-only public-address `/info` verification checks signer/agent role, standard perp (`disabled`) collateral mode, equity, and available collateral. Genuine validation illuminates the Amethyst Heart and animates the conduits between the Treasury, AEGIS, and NEXUS.
- **Treasury & Portfolio:** Displays all 12 financial metrics with explicit `VERIFIED`, `LOCAL HISTORY`, or `UNAVAILABLE` provenance badges. External deposits and withdrawals are explicitly separated from trading profits; uncertified realized net PnL remains explicitly unavailable.
- **Awakening & Emergency Suspension:** Offers `DORMANT`, `OBSERVATION`, `SIMULATION`, gated `AWAKENED` (locked until all readiness and unattended certification gates pass), `GUARDIAN`, and `SUSPENDED`, alongside a prominent fixed one-click `EMERGENCY SUSPEND` control in the top command bar.

The Windows desktop build uses WGPU (`WGPU 3D PBR` — 96-bone articulated 3D skeletal + Cook-Torrance GGX PBR + PCF shadows + HDR bloom, plus `TIMESTAMP_QUERY` live GPU pass timing when supported by hardware) and retains Glow (`--renderer glow`) as an OpenGL compatibility fallback. Run with `--renderer software` if hardware WGPU is unavailable, and `F11` to toggle fullscreen. The `Graphics / Spatial` panel and command strip report observed UI cadence, UI-build CPU milliseconds, and hardware GPU pass duration distinctly.

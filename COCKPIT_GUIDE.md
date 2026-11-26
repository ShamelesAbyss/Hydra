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

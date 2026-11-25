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

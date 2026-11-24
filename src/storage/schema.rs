//! SQLite migrations. Idempotent & forward-only.

use anyhow::Result;
use rusqlite::Connection;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"

        -- Leviathan: additive durable execution journal. Existing ledgers are preserved.
        CREATE TABLE IF NOT EXISTS execution_intents (
            intent_id TEXT PRIMARY KEY,
            venue TEXT NOT NULL,
            symbol TEXT NOT NULL,
            intent_json TEXT NOT NULL,
            state TEXT NOT NULL,
            result_json TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_execution_state ON execution_intents(state);
        CREATE TABLE IF NOT EXISTS confirmed_reductions (
            intent_id TEXT PRIMARY KEY REFERENCES execution_intents(intent_id),
            position_id INTEGER NOT NULL REFERENCES positions(id),
            filled_size REAL NOT NULL,
            exit_price REAL NOT NULL,
            remaining_size REAL NOT NULL,
            gross_pnl REAL NOT NULL,
            -- Fees/funding must be reconciled before this enters net-PnL learning.
            net_pnl REAL,
            reason TEXT NOT NULL,
            confirmed_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            ended_at   TEXT,
            runtime_secs INTEGER,
            venues TEXT NOT NULL,
            symbols TEXT NOT NULL,
            live_trading INTEGER NOT NULL,
            trades INTEGER DEFAULT 0,
            realized_usd REAL DEFAULT 0,
            shutdown_reason TEXT,
            config_snapshot TEXT
        );

        CREATE TABLE IF NOT EXISTS signals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            strategy TEXT NOT NULL,
            venue TEXT NOT NULL,
            symbol TEXT NOT NULL,
            side TEXT NOT NULL,
            confidence REAL NOT NULL,
            expected_edge_bps REAL NOT NULL,
            stop_price REAL,
            take_profit REAL,
            reason TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_signals_session ON signals(session_id);

        CREATE TABLE IF NOT EXISTS orders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent_id TEXT NOT NULL,
            venue TEXT NOT NULL,
            symbol TEXT NOT NULL,
            side TEXT NOT NULL,
            size REAL NOT NULL,
            limit_price REAL,
            reduce_only INTEGER NOT NULL DEFAULT 0,
            accepted INTEGER NOT NULL,
            venue_order_id TEXT,
            filled_size REAL NOT NULL DEFAULT 0,
            avg_fill_price REAL,
            message TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_orders_session ON orders(session_id);

        CREATE TABLE IF NOT EXISTS positions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            venue TEXT NOT NULL,
            symbol TEXT NOT NULL,
            side TEXT NOT NULL,
            size REAL NOT NULL,
            entry_price REAL NOT NULL,
            stop_price REAL,
            take_profit REAL,
            trailing_high REAL,
            opened_at TEXT NOT NULL,
            closed_at TEXT,
            strategy TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(closed_at);

        CREATE TABLE IF NOT EXISTS closed_trades (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            position_id INTEGER NOT NULL,
            session_id INTEGER NOT NULL,
            exit_price REAL NOT NULL,
            realized_pnl REAL NOT NULL,
            closed_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_closed_session ON closed_trades(session_id);

        CREATE TABLE IF NOT EXISTS errors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            message TEXT NOT NULL,
            at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_errors_session ON errors(session_id);

        CREATE TABLE IF NOT EXISTS heartbeats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS equity_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            venue TEXT NOT NULL,
            equity_usd REAL NOT NULL,
            at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS compound_state (
            id INTEGER PRIMARY KEY CHECK (id=1),
            starting_capital REAL NOT NULL,
            realized_profit  REAL NOT NULL,
            locked_buffer    REAL NOT NULL,
            current_risk_mult REAL NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS meta_kv (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- ─── v1.5.0 learning system ──────────────────────────────────────
        --
        -- One row per closed trade with the full feature vector at entry
        -- + the realized outcome. Drives adaptive sizing, similarity search,
        -- regime-conditioned expectancy, and time-of-day patterns.
        CREATE TABLE IF NOT EXISTS trade_context (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol            TEXT NOT NULL,
            strategy          TEXT NOT NULL,
            side              TEXT NOT NULL,
            -- Multi-timeframe regime at entry
            regime_1m         TEXT NOT NULL,
            regime_5m         TEXT NOT NULL,
            regime_15m        TEXT NOT NULL,
            -- Market context features
            atr_pct           REAL NOT NULL,
            spread_bps        REAL,
            edge_bps_predicted REAL NOT NULL,
            -- Time features (UTC)
            hour_utc          INTEGER NOT NULL,
            day_of_week       INTEGER NOT NULL,
            -- Outcome
            entry_price       REAL NOT NULL,
            exit_price        REAL NOT NULL,
            r_init            REAL NOT NULL,
            realized_pnl      REAL NOT NULL,
            realized_r        REAL NOT NULL,
            duration_secs     INTEGER NOT NULL,
            exit_reason       TEXT NOT NULL,
            opened_at         TEXT NOT NULL,
            closed_at         TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tc_symbol     ON trade_context(symbol, closed_at);
        CREATE INDEX IF NOT EXISTS idx_tc_strategy   ON trade_context(strategy, closed_at);
        CREATE INDEX IF NOT EXISTS idx_tc_combo      ON trade_context(symbol, strategy, regime_5m, closed_at);
        CREATE INDEX IF NOT EXISTS idx_tc_hour       ON trade_context(symbol, side, hour_utc);

        -- Cached adaptive multipliers/thresholds. Recomputed every N closes
        -- by the learning module. Read by the risk engine on every signal.
        CREATE TABLE IF NOT EXISTS adaptive_state (
            key            TEXT PRIMARY KEY,    -- e.g. 'mult:symbol:BTC' / 'mult:strategy:momentum' / 'edge_bps_threshold'
            value          REAL NOT NULL,
            sample_count   INTEGER NOT NULL,
            confidence_pct REAL NOT NULL,       -- 0–100 confidence in the value (Bayesian-ish)
            updated_at     TEXT NOT NULL
        );
    "#)?;

    let mut columns = conn.prepare("PRAGMA table_info(closed_trades)")?;
    let names = columns
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !names.iter().any(|name| name == "exit_reason") {
        conn.execute("ALTER TABLE closed_trades ADD COLUMN exit_reason TEXT", [])?;
    }
    Ok(())
}

use anyhow::Result;
use rusqlite::Connection;

/// Current schema version.
pub const CURRENT_VERSION: u32 = 1;

/// Run all migrations up to CURRENT_VERSION.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );",
    )?;

    let version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch(MIGRATION_V1)?;
        conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [1u32])?;
    }

    Ok(())
}

const MIGRATION_V1: &str = r#"
-- Markets
CREATE TABLE IF NOT EXISTS markets (
    id              TEXT PRIMARY KEY,
    question        TEXT NOT NULL,
    category        TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    resolution_date TEXT,
    platforms_json  TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_markets_status ON markets(status);

-- Trades
CREATE TABLE IF NOT EXISTS trades (
    id                  TEXT PRIMARY KEY,
    opportunity_id      TEXT NOT NULL,
    market_id           TEXT NOT NULL,
    legs_json           TEXT NOT NULL DEFAULT '[]',
    gross_spread        TEXT NOT NULL,
    net_spread          TEXT NOT NULL,
    total_fees          TEXT NOT NULL,
    gas_cost            TEXT NOT NULL,
    profit              TEXT NOT NULL,
    status              TEXT NOT NULL,
    failure_reason      TEXT,
    execution_ms        INTEGER NOT NULL,
    bankroll_after      TEXT NOT NULL,
    bankroll_change_pct TEXT NOT NULL,
    executed_at         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_trades_market ON trades(market_id);
CREATE INDEX IF NOT EXISTS idx_trades_executed ON trades(executed_at);
CREATE INDEX IF NOT EXISTS idx_trades_status ON trades(status);

-- Positions
CREATE TABLE IF NOT EXISTS positions (
    id            TEXT PRIMARY KEY,
    market_id     TEXT NOT NULL,
    platform      TEXT NOT NULL,
    side          TEXT NOT NULL,
    size          TEXT NOT NULL,
    entry_price   TEXT NOT NULL,
    current_price TEXT,
    unrealized_pnl TEXT NOT NULL DEFAULT '0',
    opened_at     TEXT NOT NULL,
    closed_at     TEXT
);
CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(closed_at) WHERE closed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_positions_market ON positions(market_id);

-- Balances
CREATE TABLE IF NOT EXISTS balances (
    platform   TEXT PRIMARY KEY,
    balance    TEXT NOT NULL,
    reserved   TEXT NOT NULL DEFAULT '0',
    available  TEXT NOT NULL DEFAULT '0',
    updated_at TEXT NOT NULL
);

-- Daily snapshots
CREATE TABLE IF NOT EXISTS daily_snapshots (
    date                TEXT PRIMARY KEY,
    total_bankroll      TEXT NOT NULL,
    total_pnl           TEXT NOT NULL,
    trade_count         INTEGER NOT NULL DEFAULT 0,
    win_count           INTEGER NOT NULL DEFAULT 0,
    loss_count          INTEGER NOT NULL DEFAULT 0,
    best_trade_pnl      TEXT NOT NULL DEFAULT '0',
    worst_trade_pnl     TEXT NOT NULL DEFAULT '0',
    avg_spread_captured TEXT NOT NULL DEFAULT '0',
    max_drawdown_pct    TEXT NOT NULL DEFAULT '0',
    platform_balances   TEXT NOT NULL DEFAULT '{}',
    report_sent         INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL
);

-- Audit log
CREATE TABLE IF NOT EXISTS audit_log (
    id        TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    action    TEXT NOT NULL,
    details   TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(timestamp);

-- Config history
CREATE TABLE IF NOT EXISTS config_history (
    id        TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    field     TEXT NOT NULL,
    old_value TEXT NOT NULL,
    new_value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_config_hist_ts ON config_history(timestamp);
"#;

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
        // Wrap DDL + version row in an explicit transaction so a partial
        // failure leaves the schema in a clean state for the next startup.
        conn.execute_batch(&format!(
            "BEGIN;\n{}\nINSERT INTO schema_version (version) VALUES (1);\nCOMMIT;",
            MIGRATION_V1
        ))?;
    }

    Ok(())
}

const MIGRATION_V1: &str = r#"
-- Markets
CREATE TABLE IF NOT EXISTS markets (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    unified_id          TEXT UNIQUE NOT NULL,
    question            TEXT NOT NULL,
    resolution_source   TEXT,
    expiration          TEXT,
    platforms           TEXT NOT NULL DEFAULT '{}',
    category            TEXT,
    confidence          REAL NOT NULL DEFAULT 1.0,
    status              TEXT NOT NULL DEFAULT 'active',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_markets_unified ON markets(unified_id);
CREATE INDEX IF NOT EXISTS idx_markets_status ON markets(status);

-- Trades
CREATE TABLE IF NOT EXISTS trades (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    opp_id              TEXT NOT NULL,
    market_id           TEXT NOT NULL,
    market_question     TEXT NOT NULL DEFAULT '',
    leg_a_platform      TEXT NOT NULL,
    leg_a_side          TEXT NOT NULL,
    leg_a_price         TEXT NOT NULL,
    leg_a_size          TEXT NOT NULL,
    leg_a_fill_price    TEXT NOT NULL DEFAULT '0',
    leg_a_fee           TEXT NOT NULL DEFAULT '0',
    leg_b_platform      TEXT NOT NULL,
    leg_b_side          TEXT NOT NULL,
    leg_b_price         TEXT NOT NULL,
    leg_b_size          TEXT NOT NULL,
    leg_b_fill_price    TEXT NOT NULL DEFAULT '0',
    leg_b_fee           TEXT NOT NULL DEFAULT '0',
    raw_spread          TEXT NOT NULL,
    net_spread          TEXT NOT NULL,
    profit              TEXT NOT NULL,
    status              TEXT NOT NULL,
    failure_reason      TEXT,
    execution_ms        INTEGER NOT NULL DEFAULT 0,
    executed_at         TEXT NOT NULL,
    bankroll_after      TEXT NOT NULL,
    bankroll_change_pct TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_trades_market ON trades(market_id);
CREATE INDEX IF NOT EXISTS idx_trades_executed ON trades(executed_at);
CREATE INDEX IF NOT EXISTS idx_trades_status ON trades(status);

-- Positions
CREATE TABLE IF NOT EXISTS positions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id       TEXT NOT NULL,
    platform        TEXT NOT NULL,
    side            TEXT NOT NULL,
    quantity        TEXT NOT NULL,
    avg_entry_price TEXT NOT NULL,
    unrealized_pnl  TEXT NOT NULL DEFAULT '0',
    opened_at       TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    closed          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(closed) WHERE closed = 0;
CREATE INDEX IF NOT EXISTS idx_positions_market ON positions(market_id);

-- Platform balances
CREATE TABLE IF NOT EXISTS platform_balances (
    platform            TEXT PRIMARY KEY,
    available           TEXT NOT NULL DEFAULT '0',
    reserved            TEXT NOT NULL DEFAULT '0',
    pending_settlement  TEXT NOT NULL DEFAULT '0',
    total               TEXT NOT NULL DEFAULT '0',
    updated_at          TEXT NOT NULL
);

-- Daily snapshots
CREATE TABLE IF NOT EXISTS daily_snapshots (
    date                TEXT PRIMARY KEY,
    bankroll            TEXT NOT NULL,
    gross_pnl           TEXT NOT NULL DEFAULT '0',
    fees_paid           TEXT NOT NULL DEFAULT '0',
    net_pnl             TEXT NOT NULL DEFAULT '0',
    trades_count        INTEGER NOT NULL DEFAULT 0,
    success_count       INTEGER NOT NULL DEFAULT 0,
    fail_count          INTEGER NOT NULL DEFAULT 0,
    success_rate        TEXT NOT NULL DEFAULT '0',
    peak_bankroll       TEXT NOT NULL DEFAULT '0',
    drawdown_pct        TEXT NOT NULL DEFAULT '0',
    kelly_utilization   TEXT NOT NULL DEFAULT '0',
    report_sent         INTEGER NOT NULL DEFAULT 0
);

-- Audit log
CREATE TABLE IF NOT EXISTS audit_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,
    module       TEXT NOT NULL,
    event_type   TEXT NOT NULL,
    data         TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(timestamp_ns);

-- Config history
CREATE TABLE IF NOT EXISTS config_history (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    changed_at TEXT NOT NULL,
    key       TEXT NOT NULL,
    old_value TEXT NOT NULL,
    new_value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_config_hist_ts ON config_history(changed_at);
"#;

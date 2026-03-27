use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

use super::migrations;
use super::traits::Database;
use crate::types::*;

/// SQLite-backed implementation of [`Database`].
#[derive(Clone)]
pub struct SqliteDb {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteDb {
    /// Open (or create) a SQLite database at `path`.
    pub fn new(path: &str, pool_size: u32, busy_timeout_ms: u64) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(manager)
            .context("failed to build SQLite connection pool")?;

        // Configure pragmas on a connection and run migrations
        {
            let conn = pool.get().context("failed to get connection from pool")?;
            conn.execute_batch(&format!(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = {busy_timeout_ms};
                 PRAGMA foreign_keys = ON;"
            ))?;
            migrations::run_migrations(&conn)?;
        }

        Ok(Self { pool })
    }

    fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get().context("failed to get db connection")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap_or_default()
}

fn dec_to_string(d: &Decimal) -> String {
    d.to_string()
}

fn opt_dec(s: Option<String>) -> Option<Decimal> {
    s.and_then(|v| Decimal::from_str(&v).ok())
}

fn platform_from_db(s: &str) -> Platform {
    Platform::from_str_loose(s).unwrap_or(Platform::Polymarket)
}

fn side_from_db(s: &str) -> Side {
    Side::from_str_loose(s).unwrap_or(Side::Yes)
}

fn status_from_db(s: &str) -> TradeStatus {
    TradeStatus::from_str_loose(s).unwrap_or(TradeStatus::Failed)
}

fn market_status_from_db(s: &str) -> MarketStatus {
    MarketStatus::from_str_loose(s).unwrap_or(MarketStatus::Active)
}

fn category_from_db(s: &str) -> MarketCategory {
    match s.to_lowercase().as_str() {
        "politics" => MarketCategory::Politics,
        "sports" => MarketCategory::Sports,
        "crypto" => MarketCategory::Crypto,
        "economics" => MarketCategory::Economics,
        "science" => MarketCategory::Science,
        "entertainment" => MarketCategory::Entertainment,
        _ => MarketCategory::Other,
    }
}

fn category_to_str(c: &MarketCategory) -> &'static str {
    match c {
        MarketCategory::Politics => "politics",
        MarketCategory::Sports => "sports",
        MarketCategory::Crypto => "crypto",
        MarketCategory::Economics => "economics",
        MarketCategory::Science => "science",
        MarketCategory::Entertainment => "entertainment",
        MarketCategory::Other => "other",
    }
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn opt_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.map(|v| parse_dt(&v))
}

// ---------------------------------------------------------------------------
// Database trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Database for SqliteDb {
    // -- Markets --------------------------------------------------------

    async fn upsert_market(&self, market: &Market) -> Result<()> {
        let conn = self.conn()?;
        let platforms_json = serde_json::to_string(&market.platforms)?;
        conn.execute(
            "INSERT INTO markets (id, question, category, status, resolution_date, platforms_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               question = excluded.question,
               category = excluded.category,
               status = excluded.status,
               resolution_date = excluded.resolution_date,
               platforms_json = excluded.platforms_json,
               updated_at = excluded.updated_at",
            rusqlite::params![
                market.id,
                market.question,
                category_to_str(&market.category),
                market.status.to_string(),
                market.resolution_date.map(|d| dt_to_str(&d)),
                platforms_json,
                dt_to_str(&market.created_at),
                dt_to_str(&market.updated_at),
            ],
        )?;
        Ok(())
    }

    async fn get_market(&self, id: &str) -> Result<Option<Market>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, question, category, status, resolution_date, platforms_json, created_at, updated_at
             FROM markets WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => {
                let platforms_str: String = row.get(5)?;
                let platforms: HashMap<Platform, PlatformMarketInfo> =
                    serde_json::from_str(&platforms_str).unwrap_or_default();
                Ok(Some(Market {
                    id: row.get(0)?,
                    question: row.get(1)?,
                    category: category_from_db(&row.get::<_, String>(2)?),
                    status: market_status_from_db(&row.get::<_, String>(3)?),
                    resolution_date: opt_dt(row.get(4)?),
                    platforms,
                    created_at: parse_dt(&row.get::<_, String>(6)?),
                    updated_at: parse_dt(&row.get::<_, String>(7)?),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_active_markets(&self) -> Result<Vec<Market>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, question, category, status, resolution_date, platforms_json, created_at, updated_at
             FROM markets WHERE status = 'active'",
        )?;
        let mut rows = stmt.query([])?;
        let mut markets = Vec::new();
        while let Some(row) = rows.next()? {
            let platforms_str: String = row.get(5)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> =
                serde_json::from_str(&platforms_str).unwrap_or_default();
            markets.push(Market {
                id: row.get(0)?,
                question: row.get(1)?,
                category: category_from_db(&row.get::<_, String>(2)?),
                status: market_status_from_db(&row.get::<_, String>(3)?),
                resolution_date: opt_dt(row.get(4)?),
                platforms,
                created_at: parse_dt(&row.get::<_, String>(6)?),
                updated_at: parse_dt(&row.get::<_, String>(7)?),
            });
        }
        Ok(markets)
    }

    // -- Trades ---------------------------------------------------------

    async fn insert_trade(&self, trade: &TradeResult) -> Result<()> {
        let conn = self.conn()?;
        let legs_json = serde_json::to_string(&trade.legs)?;
        conn.execute(
            "INSERT INTO trades (id, opportunity_id, market_id, legs_json, gross_spread, net_spread, total_fees, gas_cost, profit, status, failure_reason, execution_ms, bankroll_after, bankroll_change_pct, executed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            rusqlite::params![
                trade.id.to_string(),
                trade.opportunity_id.to_string(),
                trade.market_id,
                legs_json,
                dec_to_string(&trade.gross_spread),
                dec_to_string(&trade.net_spread),
                dec_to_string(&trade.total_fees),
                dec_to_string(&trade.gas_cost),
                dec_to_string(&trade.profit),
                trade.status.to_string(),
                trade.failure_reason,
                trade.execution_ms as i64,
                dec_to_string(&trade.bankroll_after),
                dec_to_string(&trade.bankroll_change_pct),
                dt_to_str(&trade.executed_at),
            ],
        )?;
        Ok(())
    }

    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, opportunity_id, market_id, legs_json, gross_spread, net_spread, total_fees, gas_cost, profit, status, failure_reason, execution_ms, bankroll_after, bankroll_change_pct, executed_at
             FROM trades WHERE executed_at >= ?1 ORDER BY executed_at ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![dt_to_str(&since)])?;
        let mut trades = Vec::new();
        while let Some(row) = rows.next()? {
            trades.push(row_to_trade(row)?);
        }
        Ok(trades)
    }

    async fn get_trades_for_date(&self, date: &str) -> Result<Vec<TradeResult>> {
        let conn = self.conn()?;
        let start = format!("{date}T00:00:00+00:00");
        let end = format!("{date}T23:59:59+00:00");
        let mut stmt = conn.prepare(
            "SELECT id, opportunity_id, market_id, legs_json, gross_spread, net_spread, total_fees, gas_cost, profit, status, failure_reason, execution_ms, bankroll_after, bankroll_change_pct, executed_at
             FROM trades WHERE executed_at >= ?1 AND executed_at <= ?2 ORDER BY executed_at ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![start, end])?;
        let mut trades = Vec::new();
        while let Some(row) = rows.next()? {
            trades.push(row_to_trade(row)?);
        }
        Ok(trades)
    }

    async fn get_trade_count(&self) -> Result<u64> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM trades", [], |r| r.get(0))?;
        Ok(count as u64)
    }

    // -- Positions ------------------------------------------------------

    async fn upsert_position(&self, pos: &Position) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO positions (id, market_id, platform, side, size, entry_price, current_price, unrealized_pnl, opened_at, closed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(id) DO UPDATE SET
               current_price = excluded.current_price,
               unrealized_pnl = excluded.unrealized_pnl,
               closed_at = excluded.closed_at",
            rusqlite::params![
                pos.id.to_string(),
                pos.market_id,
                pos.platform.to_string(),
                pos.side.to_string(),
                dec_to_string(&pos.size),
                dec_to_string(&pos.entry_price),
                pos.current_price.map(|d| dec_to_string(&d)),
                dec_to_string(&pos.unrealized_pnl),
                dt_to_str(&pos.opened_at),
                pos.closed_at.map(|d| dt_to_str(&d)),
            ],
        )?;
        Ok(())
    }

    async fn get_open_positions(&self) -> Result<Vec<Position>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, market_id, platform, side, size, entry_price, current_price, unrealized_pnl, opened_at, closed_at
             FROM positions WHERE closed_at IS NULL",
        )?;
        let mut rows = stmt.query([])?;
        let mut positions = Vec::new();
        while let Some(row) = rows.next()? {
            positions.push(row_to_position(row)?);
        }
        Ok(positions)
    }

    async fn close_position(&self, id: &Uuid, close_time: DateTime<Utc>) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE positions SET closed_at = ?1 WHERE id = ?2",
            rusqlite::params![dt_to_str(&close_time), id.to_string()],
        )?;
        Ok(())
    }

    // -- Balances -------------------------------------------------------

    async fn update_balance(&self, bal: &PlatformBalance) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO balances (platform, balance, reserved, available, updated_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(platform) DO UPDATE SET
               balance = excluded.balance,
               reserved = excluded.reserved,
               available = excluded.available,
               updated_at = excluded.updated_at",
            rusqlite::params![
                bal.platform.to_string(),
                dec_to_string(&bal.balance),
                dec_to_string(&bal.reserved),
                dec_to_string(&bal.available),
                dt_to_str(&bal.updated_at),
            ],
        )?;
        Ok(())
    }

    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, balance, reserved, available, updated_at FROM balances WHERE platform = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![platform.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(PlatformBalance {
                platform: platform_from_db(&row.get::<_, String>(0)?),
                balance: dec(&row.get::<_, String>(1)?),
                reserved: dec(&row.get::<_, String>(2)?),
                available: dec(&row.get::<_, String>(3)?),
                updated_at: parse_dt(&row.get::<_, String>(4)?),
            })),
            None => Ok(None),
        }
    }

    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, balance, reserved, available, updated_at FROM balances",
        )?;
        let mut rows = stmt.query([])?;
        let mut balances = Vec::new();
        while let Some(row) = rows.next()? {
            balances.push(PlatformBalance {
                platform: platform_from_db(&row.get::<_, String>(0)?),
                balance: dec(&row.get::<_, String>(1)?),
                reserved: dec(&row.get::<_, String>(2)?),
                available: dec(&row.get::<_, String>(3)?),
                updated_at: parse_dt(&row.get::<_, String>(4)?),
            });
        }
        Ok(balances)
    }

    // -- Daily snapshots ------------------------------------------------

    async fn insert_daily_snapshot(&self, snap: &DailySnapshot) -> Result<()> {
        let conn = self.conn()?;
        let balances_json = serde_json::to_string(&snap.platform_balances)?;
        conn.execute(
            "INSERT INTO daily_snapshots (date, total_bankroll, total_pnl, trade_count, win_count, loss_count, best_trade_pnl, worst_trade_pnl, avg_spread_captured, max_drawdown_pct, platform_balances, report_sent, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(date) DO UPDATE SET
               total_bankroll = excluded.total_bankroll,
               total_pnl = excluded.total_pnl,
               trade_count = excluded.trade_count,
               win_count = excluded.win_count,
               loss_count = excluded.loss_count,
               best_trade_pnl = excluded.best_trade_pnl,
               worst_trade_pnl = excluded.worst_trade_pnl,
               avg_spread_captured = excluded.avg_spread_captured,
               max_drawdown_pct = excluded.max_drawdown_pct,
               platform_balances = excluded.platform_balances",
            rusqlite::params![
                snap.date,
                dec_to_string(&snap.total_bankroll),
                dec_to_string(&snap.total_pnl),
                snap.trade_count,
                snap.win_count,
                snap.loss_count,
                dec_to_string(&snap.best_trade_pnl),
                dec_to_string(&snap.worst_trade_pnl),
                dec_to_string(&snap.avg_spread_captured),
                dec_to_string(&snap.max_drawdown_pct),
                balances_json,
                snap.report_sent as i32,
                dt_to_str(&snap.created_at),
            ],
        )?;
        Ok(())
    }

    async fn get_daily_snapshot(&self, date: &str) -> Result<Option<DailySnapshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT date, total_bankroll, total_pnl, trade_count, win_count, loss_count, best_trade_pnl, worst_trade_pnl, avg_spread_captured, max_drawdown_pct, platform_balances, report_sent, created_at
             FROM daily_snapshots WHERE date = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![date])?;
        match rows.next()? {
            Some(row) => {
                let balances_str: String = row.get(10)?;
                let platform_balances: HashMap<Platform, Decimal> =
                    serde_json::from_str(&balances_str).unwrap_or_default();
                Ok(Some(DailySnapshot {
                    date: row.get(0)?,
                    total_bankroll: dec(&row.get::<_, String>(1)?),
                    total_pnl: dec(&row.get::<_, String>(2)?),
                    trade_count: row.get(3)?,
                    win_count: row.get(4)?,
                    loss_count: row.get(5)?,
                    best_trade_pnl: dec(&row.get::<_, String>(6)?),
                    worst_trade_pnl: dec(&row.get::<_, String>(7)?),
                    avg_spread_captured: dec(&row.get::<_, String>(8)?),
                    max_drawdown_pct: dec(&row.get::<_, String>(9)?),
                    platform_balances,
                    report_sent: row.get::<_, i32>(11)? != 0,
                    created_at: parse_dt(&row.get::<_, String>(12)?),
                }))
            }
            None => Ok(None),
        }
    }

    async fn mark_report_sent(&self, date: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE daily_snapshots SET report_sent = 1 WHERE date = ?1",
            rusqlite::params![date],
        )?;
        Ok(())
    }

    // -- Audit ----------------------------------------------------------

    async fn append_audit(&self, action: &str, details: &str) -> Result<()> {
        let conn = self.conn()?;
        let id = Uuid::new_v4().to_string();
        let ts = dt_to_str(&Utc::now());
        conn.execute(
            "INSERT INTO audit_log (id, timestamp, action, details) VALUES (?1,?2,?3,?4)",
            rusqlite::params![id, ts, action, details],
        )?;
        Ok(())
    }

    async fn log_config_change(&self, field: &str, old_val: &str, new_val: &str) -> Result<()> {
        let conn = self.conn()?;
        let id = Uuid::new_v4().to_string();
        let ts = dt_to_str(&Utc::now());
        conn.execute(
            "INSERT INTO config_history (id, timestamp, field, old_value, new_value) VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![id, ts, field, old_val, new_val],
        )?;
        Ok(())
    }

    // -- Meta -----------------------------------------------------------

    async fn db_size_bytes(&self) -> Result<u64> {
        let conn = self.conn()?;
        let page_count: i64 =
            conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 =
            conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((page_count * page_size) as u64)
    }
}

// ---------------------------------------------------------------------------
// Row-to-struct helpers
// ---------------------------------------------------------------------------

fn row_to_trade(row: &rusqlite::Row) -> Result<TradeResult> {
    let legs_str: String = row.get(3)?;
    let legs: Vec<LegDetail> = serde_json::from_str(&legs_str).unwrap_or_default();
    Ok(TradeResult {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
        opportunity_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::new_v4()),
        market_id: row.get(2)?,
        legs,
        gross_spread: dec(&row.get::<_, String>(4)?),
        net_spread: dec(&row.get::<_, String>(5)?),
        total_fees: dec(&row.get::<_, String>(6)?),
        gas_cost: dec(&row.get::<_, String>(7)?),
        profit: dec(&row.get::<_, String>(8)?),
        status: status_from_db(&row.get::<_, String>(9)?),
        failure_reason: row.get(10)?,
        execution_ms: row.get::<_, i64>(11)? as u64,
        bankroll_after: dec(&row.get::<_, String>(12)?),
        bankroll_change_pct: dec(&row.get::<_, String>(13)?),
        executed_at: parse_dt(&row.get::<_, String>(14)?),
    })
}

fn row_to_position(row: &rusqlite::Row) -> Result<Position> {
    Ok(Position {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
        market_id: row.get(1)?,
        platform: platform_from_db(&row.get::<_, String>(2)?),
        side: side_from_db(&row.get::<_, String>(3)?),
        size: dec(&row.get::<_, String>(4)?),
        entry_price: dec(&row.get::<_, String>(5)?),
        current_price: opt_dec(row.get(6)?),
        unrealized_pnl: dec(&row.get::<_, String>(7)?),
        opened_at: parse_dt(&row.get::<_, String>(8)?),
        closed_at: opt_dt(row.get(9)?),
    })
}

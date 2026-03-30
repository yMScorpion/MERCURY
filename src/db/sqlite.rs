use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate, Utc};
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

        // Apply PRAGMAs on every connection created by the pool via with_init.
        // WAL mode is file-level (persistent) but is idempotent to set again.
        // synchronous, busy_timeout, and foreign_keys are connection-level and
        // must be set on each connection, not just a single borrowed one.
        let manager = SqliteConnectionManager::file(path).with_init(move |conn| {
            conn.execute_batch(&format!(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA busy_timeout={busy_timeout_ms};
                 PRAGMA foreign_keys=ON;"
            ))
        });
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(manager)
            .context("failed to build SQLite connection pool")?;

        // Run migrations on first connection
        {
            let conn = pool.get().context("failed to get connection from pool")?;
            migrations::run_migrations(&conn)?;
        }

        Ok(Self { pool })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dec(s: &str) -> rusqlite::Result<Decimal> {
    Decimal::from_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })
}

fn dec_to_string(d: &Decimal) -> String {
    d.to_string()
}

fn platform_from_db(s: &str) -> Result<Platform> {
    match s {
        "Polymarket" => Ok(Platform::Polymarket),
        "Polymarket US" => Ok(Platform::PolymarketUs),
        "Kalshi" => Ok(Platform::Kalshi),
        "CDNA" => Ok(Platform::Cdna),
        "ForecastEx" => Ok(Platform::ForecastEx),
        _ => Err(anyhow::anyhow!("Unknown platform string in DB: {}", s)),
    }
}

fn side_from_db(s: &str) -> Side {
    match s {
        "YES" => Side::Yes,
        "NO" => Side::No,
        _ => Side::Yes,
    }
}

/// Shared upsert logic used by both `upsert_position` and `upsert_position_pair`.
fn upsert_position_on(conn: &rusqlite::Connection, pos: &Position) -> Result<()> {
    if pos.id == 0 {
        conn.execute(
            "INSERT INTO positions (market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                pos.market_id.to_string(),
                pos.platform.to_string(),
                pos.side.to_string(),
                dec_to_string(&pos.quantity),
                dec_to_string(&pos.avg_entry_price),
                dec_to_string(&pos.unrealized_pnl),
                dt_to_str(&pos.opened_at),
                dt_to_str(&pos.updated_at),
            ],
        )?;
    } else {
        conn.execute(
            "INSERT INTO positions (id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET
               quantity = excluded.quantity,
               avg_entry_price = excluded.avg_entry_price,
               unrealized_pnl = excluded.unrealized_pnl,
               updated_at = excluded.updated_at",
            rusqlite::params![
                pos.id,
                pos.market_id.to_string(),
                pos.platform.to_string(),
                pos.side.to_string(),
                dec_to_string(&pos.quantity),
                dec_to_string(&pos.avg_entry_price),
                dec_to_string(&pos.unrealized_pnl),
                dt_to_str(&pos.opened_at),
                dt_to_str(&pos.updated_at),
            ],
        )?;
    }
    Ok(())
}

fn trade_status_from_db(s: &str) -> TradeStatus {
    match s {
        "success" => TradeStatus::Success,
        "fail" => TradeStatus::Fail,
        "partial" => TradeStatus::Partial,
        _ => TradeStatus::Fail,
    }
}

fn market_status_from_db(s: &str) -> MarketStatus {
    match s {
        "active" => MarketStatus::Active,
        "suspended" => MarketStatus::Suspended,
        "resolved" => MarketStatus::Resolved,
        "expired" => MarketStatus::Expired,
        _ => MarketStatus::Active,
    }
}

fn category_from_db(s: &str) -> MarketCategory {
    match s {
        "sports" => MarketCategory::Sports,
        "politics" => MarketCategory::Politics,
        "finance" => MarketCategory::Finance,
        "crypto" => MarketCategory::Crypto,
        "weather" => MarketCategory::Weather,
        "culture" => MarketCategory::Culture,
        _ => MarketCategory::Other,
    }
}

fn category_to_str(c: &MarketCategory) -> &'static str {
    match c {
        MarketCategory::Sports => "sports",
        MarketCategory::Politics => "politics",
        MarketCategory::Finance => "finance",
        MarketCategory::Crypto => "crypto",
        MarketCategory::Weather => "weather",
        MarketCategory::Culture => "culture",
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

fn parse_naive_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|_| Utc::now().date_naive())
}

// ---------------------------------------------------------------------------
// Database trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Database for SqliteDb {
    // -- Markets --------------------------------------------------------

    async fn upsert_market(&self, market: &Market) -> Result<()> {
        let pool = self.pool.clone();
        let market = market.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            let platforms_json = serde_json::to_string(&market.platforms)?;
            conn.execute(
                "INSERT INTO markets (unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(unified_id) DO UPDATE SET
                   question = excluded.question,
                   resolution_source = excluded.resolution_source,
                   expiration = excluded.expiration,
                   platforms = excluded.platforms,
                   category = excluded.category,
                   confidence = excluded.confidence,
                   status = excluded.status,
                   updated_at = excluded.updated_at",
                rusqlite::params![
                    market.unified_id.to_string(),
                    market.question,
                    market.resolution_source,
                    dt_to_str(&market.expiration),
                    platforms_json,
                    category_to_str(&market.category),
                    market.confidence,
                    market.status.to_string(),
                    dt_to_str(&market.created_at),
                    dt_to_str(&market.updated_at),
                ],
            )?;
            Ok(())
        })
        .await
        .context("upsert_market db task panicked")?
    }

    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>> {
        let pool = self.pool.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || -> Result<Option<Market>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
                 FROM markets WHERE unified_id = ?1",
            )?;
            let mut rows = stmt.query(rusqlite::params![id.to_string()])?;
            match rows.next()? {
                Some(row) => {
                    let platforms_str: String = row.get(4)?;
                    let platforms: HashMap<Platform, PlatformMarketInfo> =
                        serde_json::from_str(&platforms_str).unwrap_or_default();
                    Ok(Some(Market {
                        unified_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                        question: row.get(1)?,
                        resolution_source: row.get(2)?,
                        expiration: parse_dt(&row.get::<_, String>(3)?),
                        platforms,
                        category: category_from_db(&row.get::<_, String>(5)?),
                        confidence: row.get(6)?,
                        status: market_status_from_db(&row.get::<_, String>(7)?),
                        created_at: parse_dt(&row.get::<_, String>(8)?),
                        updated_at: parse_dt(&row.get::<_, String>(9)?),
                    }))
                }
                None => Ok(None),
            }
        })
        .await
        .context("get_market db task panicked")?
    }

    async fn get_active_markets(&self) -> Result<Vec<Market>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Market>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
                 FROM markets WHERE status = 'active'",
            )?;
            let mut rows = stmt.query([])?;
            let mut markets = Vec::new();
            while let Some(row) = rows.next()? {
                let platforms_str: String = row.get(4)?;
                let platforms: HashMap<Platform, PlatformMarketInfo> =
                    serde_json::from_str(&platforms_str).unwrap_or_default();
                markets.push(Market {
                    unified_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                    question: row.get(1)?,
                    resolution_source: row.get(2)?,
                    expiration: parse_dt(&row.get::<_, String>(3)?),
                    platforms,
                    category: category_from_db(&row.get::<_, String>(5)?),
                    confidence: row.get(6)?,
                    status: market_status_from_db(&row.get::<_, String>(7)?),
                    created_at: parse_dt(&row.get::<_, String>(8)?),
                    updated_at: parse_dt(&row.get::<_, String>(9)?),
                });
            }
            Ok(markets)
        })
        .await
        .context("get_active_markets db task panicked")?
    }

    // -- Trades ---------------------------------------------------------

    async fn insert_trade(&self, trade: &TradeResult) -> Result<i64> {
        let pool = self.pool.clone();
        let trade = trade.clone();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = pool.get().context("failed to get db connection")?;
            conn.execute(
                "INSERT INTO trades (opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
                rusqlite::params![
                    trade.opp_id.to_string(),
                    trade.market_id.to_string(),
                    trade.market_question,
                    trade.leg_a_platform.to_string(),
                    trade.leg_a_side.to_string(),
                    dec_to_string(&trade.leg_a_price),
                    dec_to_string(&trade.leg_a_size),
                    dec_to_string(&trade.leg_a_fill_price),
                    dec_to_string(&trade.leg_a_fee),
                    trade.leg_b_platform.to_string(),
                    trade.leg_b_side.to_string(),
                    dec_to_string(&trade.leg_b_price),
                    dec_to_string(&trade.leg_b_size),
                    dec_to_string(&trade.leg_b_fill_price),
                    dec_to_string(&trade.leg_b_fee),
                    dec_to_string(&trade.raw_spread),
                    dec_to_string(&trade.net_spread),
                    dec_to_string(&trade.profit),
                    trade.status.to_string(),
                    trade.failure_reason.as_deref(),
                    trade.execution_ms as i64,
                    dt_to_str(&trade.executed_at),
                    dec_to_string(&trade.bankroll_after),
                    dec_to_string(&trade.bankroll_change_pct),
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .context("insert_trade db task panicked")?
    }

    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<TradeResult>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
                 FROM trades WHERE executed_at >= ?1 ORDER BY executed_at ASC",
            )?;
            let mut rows = stmt.query(rusqlite::params![dt_to_str(&since)])?;
            let mut trades = Vec::new();
            while let Some(row) = rows.next()? {
                trades.push(row_to_trade(row)?);
            }
            Ok(trades)
        })
        .await
        .context("get_trades_since db task panicked")?
    }

    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<TradeResult>> {
            let conn = pool.get().context("failed to get db connection")?;
            let start = format!("{}T00:00:00+00:00", date);
            // Use the next day as an exclusive upper bound to capture all sub-second
            // trades on `date` (23:59:59+00:00 would miss trades after that second).
            let end = format!("{}T00:00:00+00:00", date + Days::new(1));
            let mut stmt = conn.prepare(
                "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
                 FROM trades WHERE executed_at >= ?1 AND executed_at < ?2 ORDER BY executed_at ASC",
            )?;
            let mut rows = stmt.query(rusqlite::params![start, end])?;
            let mut trades = Vec::new();
            while let Some(row) = rows.next()? {
                trades.push(row_to_trade(row)?);
            }
            Ok(trades)
        })
        .await
        .context("get_trades_for_date db task panicked")?
    }

    async fn get_trade_count(&self) -> Result<i64> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let conn = pool.get().context("failed to get db connection")?;
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM trades", [], |r| r.get(0))?;
            Ok(count)
        })
        .await
        .context("get_trade_count db task panicked")?
    }

    // -- Positions ------------------------------------------------------

    async fn upsert_position(&self, pos: &Position) -> Result<()> {
        let pool = self.pool.clone();
        let pos = pos.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            upsert_position_on(&conn, &pos)
        })
        .await
        .context("upsert_position db task panicked")?
    }

    async fn upsert_position_pair(&self, pos_a: &Position, pos_b: &Position) -> Result<()> {
        let pool = self.pool.clone();
        let pos_a = pos_a.clone();
        let pos_b = pos_b.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = pool.get().context("failed to get db connection")?;
            let tx = conn.transaction()?;
            upsert_position_on(&tx, &pos_a)?;
            upsert_position_on(&tx, &pos_b)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert_position_pair db task panicked")?
    }

    async fn get_open_positions(&self) -> Result<Vec<Position>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Position>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at
                 FROM positions WHERE closed = 0",
            )?;
            let mut rows = stmt.query([])?;
            let mut positions = Vec::new();
            while let Some(row) = rows.next()? {
                positions.push(row_to_position(row)?);
            }
            Ok(positions)
        })
        .await
        .context("get_open_positions db task panicked")?
    }

    async fn get_open_arb_count(&self) -> Result<usize> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<usize> {
            let conn = pool.get().context("failed to get db connection")?;
            // Fix: We must count the total number of legs and divide by 2 to get the active arb count.
            // DISTINCT market_id masks when we hold 2 or 3 arbs on the same underlying market.
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM positions WHERE closed = 0",
                [],
                |r| r.get(0),
            )?;
            Ok((count / 2) as usize)
        })
        .await
        .context("get_open_arb_count db task panicked")?
    }

    async fn close_position(&self, id: i64) -> Result<()> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            conn.execute(
                "UPDATE positions SET closed = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })
        .await
        .context("close_position db task panicked")?
    }

    // -- Balances -------------------------------------------------------

    async fn update_balance(&self, bal: &PlatformBalance) -> Result<()> {
        let pool = self.pool.clone();
        let bal = bal.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            conn.execute(
                "INSERT INTO platform_balances (platform, available, reserved, pending_settlement, total, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(platform) DO UPDATE SET
                   available = excluded.available,
                   reserved = excluded.reserved,
                   pending_settlement = excluded.pending_settlement,
                   total = excluded.total,
                   updated_at = excluded.updated_at",
                rusqlite::params![
                    bal.platform.to_string(),
                    dec_to_string(&bal.available),
                    dec_to_string(&bal.reserved),
                    dec_to_string(&bal.pending_settlement),
                    dec_to_string(&bal.total),
                    dt_to_str(&bal.updated_at),
                ],
            )?;
            Ok(())
        })
        .await
        .context("update_balance db task panicked")?
    }

    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<PlatformBalance>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT platform, available, reserved, pending_settlement, total, updated_at
                 FROM platform_balances WHERE platform = ?1",
            )?;
            let mut rows = stmt.query(rusqlite::params![platform.to_string()])?;
            match rows.next()? {
                Some(row) => Ok(Some(PlatformBalance {
                    platform: platform_from_db(&row.get::<_, String>(0)?),
                    available: dec(&row.get::<_, String>(1)?)?,
                    reserved: dec(&row.get::<_, String>(2)?)?,
                    pending_settlement: dec(&row.get::<_, String>(3)?)?,
                    total: dec(&row.get::<_, String>(4)?)?,
                    updated_at: parse_dt(&row.get::<_, String>(5)?),
                })),
                None => Ok(None),
            }
        })
        .await
        .context("get_balance db task panicked")?
    }

    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<PlatformBalance>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT platform, available, reserved, pending_settlement, total, updated_at
                 FROM platform_balances",
            )?;
            let mut rows = stmt.query([])?;
            let mut balances = Vec::new();
            while let Some(row) = rows.next()? {
                balances.push(PlatformBalance {
                    platform: platform_from_db(&row.get::<_, String>(0)?),
                    available: dec(&row.get::<_, String>(1)?)?,
                    reserved: dec(&row.get::<_, String>(2)?)?,
                    pending_settlement: dec(&row.get::<_, String>(3)?)?,
                    total: dec(&row.get::<_, String>(4)?)?,
                    updated_at: parse_dt(&row.get::<_, String>(5)?),
                });
            }
            Ok(balances)
        })
        .await
        .context("get_all_balances db task panicked")?
    }

    // -- Daily snapshots ------------------------------------------------

    async fn insert_daily_snapshot(&self, snap: &DailySnapshot) -> Result<()> {
        let pool = self.pool.clone();
        let snap = snap.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            // report_sent intentionally excluded: it defaults to 0 on insert and
            // ON CONFLICT must not reset it (mark_report_sent owns that flag).
            conn.execute(
                "INSERT INTO daily_snapshots (date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
                 ON CONFLICT(date) DO UPDATE SET
                   bankroll = excluded.bankroll,
                   gross_pnl = excluded.gross_pnl,
                   fees_paid = excluded.fees_paid,
                   net_pnl = excluded.net_pnl,
                   trades_count = excluded.trades_count,
                   success_count = excluded.success_count,
                   fail_count = excluded.fail_count,
                   success_rate = excluded.success_rate,
                   peak_bankroll = excluded.peak_bankroll,
                   drawdown_pct = excluded.drawdown_pct,
                   kelly_utilization = excluded.kelly_utilization",
                rusqlite::params![
                    snap.date.to_string(),
                    dec_to_string(&snap.bankroll),
                    dec_to_string(&snap.gross_pnl),
                    dec_to_string(&snap.fees_paid),
                    dec_to_string(&snap.net_pnl),
                    snap.trades_count,
                    snap.success_count,
                    snap.fail_count,
                    dec_to_string(&snap.success_rate),
                    dec_to_string(&snap.peak_bankroll),
                    dec_to_string(&snap.drawdown_pct),
                    dec_to_string(&snap.kelly_utilization),
                ],
            )?;
            Ok(())
        })
        .await
        .context("insert_daily_snapshot db task panicked")?
    }

    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<DailySnapshot>> {
            let conn = pool.get().context("failed to get db connection")?;
            let mut stmt = conn.prepare(
                "SELECT date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization, report_sent
                 FROM daily_snapshots WHERE date = ?1",
            )?;
            let mut rows = stmt.query(rusqlite::params![date.to_string()])?;
            match rows.next()? {
                Some(row) => {
                    Ok(Some(DailySnapshot {
                        date: parse_naive_date(&row.get::<_, String>(0)?),
                        bankroll: dec(&row.get::<_, String>(1)?)?,
                        gross_pnl: dec(&row.get::<_, String>(2)?)?,
                        fees_paid: dec(&row.get::<_, String>(3)?)?,
                        net_pnl: dec(&row.get::<_, String>(4)?)?,
                        trades_count: row.get(5)?,
                        success_count: row.get(6)?,
                        fail_count: row.get(7)?,
                        success_rate: dec(&row.get::<_, String>(8)?)?,
                        peak_bankroll: dec(&row.get::<_, String>(9)?)?,
                        drawdown_pct: dec(&row.get::<_, String>(10)?)?,
                        kelly_utilization: dec(&row.get::<_, String>(11)?)?,
                        report_sent: row.get::<_, i32>(12)? != 0,
                    }))
                }
                None => Ok(None),
            }
        })
        .await
        .context("get_daily_snapshot db task panicked")?
    }

    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            conn.execute(
                "UPDATE daily_snapshots SET report_sent = 1 WHERE date = ?1",
                rusqlite::params![date.to_string()],
            )?;
            Ok(())
        })
        .await
        .context("mark_report_sent db task panicked")?
    }

    // -- Audit ----------------------------------------------------------

    async fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let pool = self.pool.clone();
        let entry = entry.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            let data_str = serde_json::to_string(&entry.data)?;
            conn.execute(
                "INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES (?1,?2,?3,?4)",
                rusqlite::params![
                    entry.timestamp_ns as i64,
                    entry.module,
                    entry.event_type,
                    data_str,
                ],
            )?;
            Ok(())
        })
        .await
        .context("append_audit db task panicked")?
    }

    async fn append_audit_batch(&self, entries: &[AuditEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let pool = self.pool.clone();
        let entries: Vec<AuditEntry> = entries.to_vec();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = pool.get().context("failed to get db connection")?;
            let tx = conn.transaction()?;
            for entry in &entries {
                let data_str = serde_json::to_string(&entry.data)?;
                tx.execute(
                    "INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES (?1,?2,?3,?4)",
                    rusqlite::params![
                        entry.timestamp_ns as i64,
                        entry.module, entry.event_type, data_str,
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("append_audit_batch db task panicked")?
    }

    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        let old_val = old_val.to_owned();
        let new_val = new_val.to_owned();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            let ts = dt_to_str(&Utc::now());
            conn.execute(
                "INSERT INTO config_history (changed_at, key, old_value, new_value) VALUES (?1,?2,?3,?4)",
                rusqlite::params![ts, key, old_val, new_val],
            )?;
            Ok(())
        })
        .await
        .context("log_config_change db task panicked")?
    }

    // -- Meta -----------------------------------------------------------

    async fn db_size_bytes(&self) -> Result<u64> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> Result<u64> {
            let conn = pool.get().context("failed to get db connection")?;
            let page_count: i64 =
                conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            let page_size: i64 =
                conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
            Ok((page_count * page_size) as u64)
        })
        .await
        .context("db_size_bytes db task panicked")?
    }

    async fn backup_to_file(&self, dest_path: &str) -> Result<()> {
        let pool = self.pool.clone();
        let dest = dest_path.to_owned();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = pool.get().context("failed to get db connection")?;
            // Use the SQLite Online Backup API which does NOT lock the source
            // database. It copies pages incrementally, allowing concurrent reads
            // and writes during the backup (unlike VACUUM INTO which holds a lock
            // for the entire operation).
            let mut dest_conn = rusqlite::Connection::open(&dest)
                .context("failed to open backup destination")?;
            let backup = rusqlite::backup::Backup::new(&conn, &mut dest_conn)
                .context("failed to initialize SQLite backup")?;
            // Copy 256 pages at a time, sleeping 10ms between batches to avoid
            // starving active queries on the source connection pool.
            backup.run_to_completion(256, std::time::Duration::from_millis(10), None)
                .context("SQLite backup failed")?;
            Ok(())
        })
        .await
        .context("backup_to_file db task panicked")?
    }
}



// ---------------------------------------------------------------------------
// Row-to-struct helpers
// ---------------------------------------------------------------------------

fn row_to_trade(row: &rusqlite::Row) -> Result<TradeResult> {
    Ok(TradeResult {
        trade_id: row.get(0)?,
        opp_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::new_v4()),
        market_id: Uuid::parse_str(&row.get::<_, String>(2)?).unwrap_or_else(|_| Uuid::new_v4()),
        market_question: row.get(3)?,
        leg_a_platform: platform_from_db(&row.get::<_, String>(4)?),
        leg_a_side: side_from_db(&row.get::<_, String>(5)?),
        leg_a_price: dec(&row.get::<_, String>(6)?)?,
        leg_a_size: dec(&row.get::<_, String>(7)?)?,
        leg_a_fill_price: dec(&row.get::<_, String>(8)?)?,
        leg_a_fee: dec(&row.get::<_, String>(9)?)?,
        leg_b_platform: platform_from_db(&row.get::<_, String>(10)?),
        leg_b_side: side_from_db(&row.get::<_, String>(11)?),
        leg_b_price: dec(&row.get::<_, String>(12)?)?,
        leg_b_size: dec(&row.get::<_, String>(13)?)?,
        leg_b_fill_price: dec(&row.get::<_, String>(14)?)?,
        leg_b_fee: dec(&row.get::<_, String>(15)?)?,
        raw_spread: dec(&row.get::<_, String>(16)?)?,
        net_spread: dec(&row.get::<_, String>(17)?)?,
        profit: dec(&row.get::<_, String>(18)?)?,
        status: trade_status_from_db(&row.get::<_, String>(19)?),
        failure_reason: row.get(20)?,
        execution_ms: row.get::<_, i64>(21)? as u64,
        executed_at: parse_dt(&row.get::<_, String>(22)?),
        bankroll_after: dec(&row.get::<_, String>(23)?)?,
        bankroll_change_pct: dec(&row.get::<_, String>(24)?)?,
        // Not stored in DB — zero is correct; only meaningful in-flight during the event loop.
        approved_size: Decimal::ZERO,
    })
}

fn row_to_position(row: &rusqlite::Row) -> Result<Position> {
    Ok(Position {
        id: row.get(0)?,
        market_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_else(|_| Uuid::new_v4()),
        platform: platform_from_db(&row.get::<_, String>(2)?),
        side: side_from_db(&row.get::<_, String>(3)?),
        quantity: dec(&row.get::<_, String>(4)?)?,
        avg_entry_price: dec(&row.get::<_, String>(5)?)?,
        unrealized_pnl: dec(&row.get::<_, String>(6)?)?,
        opened_at: parse_dt(&row.get::<_, String>(7)?),
        updated_at: parse_dt(&row.get::<_, String>(8)?),
    })
}

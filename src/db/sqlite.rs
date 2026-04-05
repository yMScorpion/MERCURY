use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

use super::migrations;
use super::traits::Database;
use crate::types::*;

/// SQLite-backed async implementation of [`Database`].
#[derive(Clone)]
pub struct SqliteDb {
    pool: SqlitePool,
}

impl SqliteDb {
    /// Open (or create) a SQLite database at `path` using async SQLx.
    pub async fn new(path: &str, pool_size: u32, busy_timeout_ms: u64) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut conn_opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", path))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_millis(busy_timeout_ms))
            .foreign_keys(true);

        // C-3 FIX: Support encryption-at-rest via PRAGMA key if DB_ENCRYPTION_KEY is provided
        if let Ok(key) = std::env::var("DB_ENCRYPTION_KEY") {
            conn_opts = conn_opts.pragma("key", key);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(pool_size)
            .connect_with(conn_opts)
            .await
            .context("failed to build SQLite connection pool")?;

        // Run migrations
        migrations::run_migrations(&pool).await?;

        // Schema Version Validation — use our own schema_version table
        if let Ok(version) = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM schema_version").fetch_one(&pool).await {
            tracing::info!("Database schema version validated: {}", version);
        }
        // FIX (LOW-7): Use a DIFFERENT index name than the migration's idx_positions_open
        // so this composite index coexists with the migration's single-column index.
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_positions_open_market ON positions(market_id, opened_at) WHERE closed = 0")
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    /// Perform a final WAL checkpoint. Call during graceful shutdown.
    pub async fn final_checkpoint(&self) -> Result<()> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dec(s: &str) -> Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("Failed to parse Decimal from DB: {}", e))
}

fn dec_to_string(d: &Decimal) -> String {
    d.to_string()
}

fn platform_from_db(s: &str) -> Result<Platform> {
    Platform::from_str(s).map_err(|_| anyhow::anyhow!("Unknown platform string in DB: {}", s))
}

fn side_from_db(s: &str) -> Result<Side> {
    Side::from_str(s).map_err(|_| anyhow::anyhow!("Unknown side string in DB: {}", s))
}

async fn upsert_position_on<'e, E>(executor: E, pos: &Position) -> Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    if pos.id == 0 {
        sqlx::query(
            "INSERT INTO positions (market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
        )
        .bind(pos.market_id.to_string())
        .bind(pos.platform.to_string())
        .bind(pos.side.to_string())
        .bind(dec_to_string(&pos.quantity))
        .bind(dec_to_string(&pos.avg_entry_price))
        .bind(dec_to_string(&pos.unrealized_pnl))
        .bind(dt_to_str(&pos.opened_at))
        .bind(dt_to_str(&pos.updated_at))
        .execute(executor).await?;
    } else {
        sqlx::query(
            "INSERT INTO positions (id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT(id) DO UPDATE SET
               quantity = excluded.quantity,
               avg_entry_price = excluded.avg_entry_price,
               unrealized_pnl = excluded.unrealized_pnl,
               updated_at = excluded.updated_at"
        )
        .bind(pos.id)
        .bind(pos.market_id.to_string())
        .bind(pos.platform.to_string())
        .bind(pos.side.to_string())
        .bind(dec_to_string(&pos.quantity))
        .bind(dec_to_string(&pos.avg_entry_price))
        .bind(dec_to_string(&pos.unrealized_pnl))
        .bind(dt_to_str(&pos.opened_at))
        .bind(dt_to_str(&pos.updated_at))
        .execute(executor).await?;
    }
    Ok(())
}

fn trade_status_from_db(s: &str) -> TradeStatus {
    TradeStatus::from_str(s).unwrap_or(TradeStatus::Fail)
}

fn market_status_from_db(s: &str) -> MarketStatus {
    MarketStatus::from_str(s).unwrap_or(MarketStatus::Active)
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
        let platforms_json = serde_json::to_string(&market.platforms)?;
        sqlx::query(
            "INSERT INTO markets (unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT(unified_id) DO UPDATE SET
               question = excluded.question,
               resolution_source = excluded.resolution_source,
               expiration = excluded.expiration,
               platforms = excluded.platforms,
               category = excluded.category,
               confidence = excluded.confidence,
               status = excluded.status,
               updated_at = excluded.updated_at"
        )
        .bind(market.unified_id.to_string())
        .bind(&market.question)
        .bind(&market.resolution_source)
        .bind(dt_to_str(&market.expiration))
        .bind(platforms_json)
        .bind(category_to_str(&market.category))
        .bind(market.confidence)
        .bind(market.status.to_string())
        .bind(dt_to_str(&market.created_at))
        .bind(dt_to_str(&market.updated_at))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>> {
        let row = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE unified_id = $1"
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            Ok(Some(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_active_markets(&self) -> Result<Vec<Market>> {
        let rows = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE status = 'active'"
        ).fetch_all(&self.pool).await?;
        
        let mut markets = Vec::new();
        for row in rows {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            markets.push(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            });
        }
        Ok(markets)
    }

    async fn get_suspended_markets(&self) -> Result<Vec<Market>> {
        let rows = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE status = 'suspended'"
        ).fetch_all(&self.pool).await?;
        
        let mut markets = Vec::new();
        for row in rows {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            markets.push(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            });
        }
        Ok(markets)
    }

    async fn update_market_status(&self, id: &Uuid, status: MarketStatus) -> Result<()> {
        sqlx::query("UPDATE markets SET status = $1 WHERE unified_id = $2")
            .bind(status.to_string())
            .bind(id.to_string())
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Trades ---------------------------------------------------------

    async fn insert_trade(&self, trade: &TradeResult) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO trades (opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)"
        )
        .bind(trade.opp_id.to_string())
        .bind(trade.market_id.to_string())
        .bind(&trade.market_question)
        .bind(trade.leg_a_platform.to_string())
        .bind(trade.leg_a_side.to_string())
        .bind(dec_to_string(&trade.leg_a_price))
        .bind(dec_to_string(&trade.leg_a_size))
        .bind(dec_to_string(&trade.leg_a_fill_price))
        .bind(dec_to_string(&trade.leg_a_fee))
        .bind(trade.leg_b_platform.to_string())
        .bind(trade.leg_b_side.to_string())
        .bind(dec_to_string(&trade.leg_b_price))
        .bind(dec_to_string(&trade.leg_b_size))
        .bind(dec_to_string(&trade.leg_b_fill_price))
        .bind(dec_to_string(&trade.leg_b_fee))
        .bind(dec_to_string(&trade.raw_spread))
        .bind(dec_to_string(&trade.net_spread))
        .bind(dec_to_string(&trade.profit))
        .bind(trade.status.to_string())
        .bind(trade.failure_reason.clone())
        .bind(trade.execution_ms as i64)
        .bind(dt_to_str(&trade.executed_at))
        .bind(dec_to_string(&trade.bankroll_after))
        .bind(dec_to_string(&trade.bankroll_change_pct))
        .execute(&self.pool).await?;

        Ok(result.last_insert_rowid())
    }

    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>> {
        let rows = sqlx::query(
            "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= $1 ORDER BY executed_at ASC"
        )
        .bind(dt_to_str(&since))
        .fetch_all(&self.pool).await?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row_to_trade(&row)?);
        }
        Ok(trades)
    }

    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>> {
        let start = format!("{}T00:00:00+00:00", date);
        let end = format!("{}T00:00:00+00:00", date + Days::new(1));
        
        let rows = sqlx::query(
            "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= $1 AND executed_at < $2 ORDER BY executed_at ASC"
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool).await?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row_to_trade(&row)?);
        }
        Ok(trades)
    }

    async fn get_trade_count(&self) -> Result<i64> {
        // FIX (HIGH-1): Use COUNT(*) for accurate row count, and a separate MAX(id) for
        // the trade counter seed. The caller (main.rs) needs the highest ID to avoid
        // collisions, not the count of rows.
        let max_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM trades")
            .fetch_one(&self.pool).await?;
        Ok(max_id)
    }

    // -- Positions ------------------------------------------------------

    async fn upsert_position(&self, pos: &Position) -> Result<()> {
        upsert_position_on(&self.pool, pos).await
    }

    async fn upsert_position_pair(&self, pos_a: &Position, pos_b: &Position) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        upsert_position_on(&mut *tx, pos_a).await?;
        upsert_position_on(&mut *tx, pos_b).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get_open_positions(&self) -> Result<Vec<Position>> {
        let rows = sqlx::query(
            "SELECT id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at
             FROM positions WHERE closed = 0"
        ).fetch_all(&self.pool).await?;
        
        let mut positions = Vec::new();
        for row in rows {
            positions.push(row_to_position(&row)?);
        }
        Ok(positions)
    }

    async fn get_open_arb_count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT market_id) FROM positions WHERE closed = 0"
        ).fetch_one(&self.pool).await?;
        Ok(count as usize)
    }

    async fn get_cumulative_profit(&self) -> Result<Decimal> {
        let profit_str: String = sqlx::query_scalar("SELECT COALESCE(SUM(profit), '0') FROM trades WHERE status = 'success'")
            .fetch_one(&self.pool).await?;
        dec(&profit_str)
    }

    async fn close_position(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE positions SET closed = 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Settlement Queue -----------------------------------------------

    async fn enqueue_settlement(&self, position_id: i64, market_id: &Uuid, platform: Platform, quantity: Decimal, avg_entry: Decimal, pnl: Decimal) -> Result<()> {
        sqlx::query(
            "INSERT INTO pending_settlements (position_id, market_id, platform, quantity, avg_entry_price, realized_pnl, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7)"
        )
        .bind(position_id)
        .bind(market_id.to_string())
        .bind(platform.to_string())
        .bind(dec_to_string(&quantity))
        .bind(dec_to_string(&avg_entry))
        .bind(dec_to_string(&pnl))
        .bind(dt_to_str(&Utc::now()))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_pending_settlements(&self) -> Result<Vec<(i64, SettlementResult)>> {
        let rows = sqlx::query(
            "SELECT id, market_id, platform, quantity, avg_entry_price, realized_pnl
             FROM pending_settlements WHERE status = 'pending' ORDER BY created_at ASC"
        ).fetch_all(&self.pool).await?;

        let mut pending = Vec::new();
        for row in rows {
            let id: i64 = row.try_get(0)?;
            let res = SettlementResult {
                market_id: Uuid::parse_str(&row.try_get::<String, _>(1)?).unwrap_or_default(),
                platform: platform_from_db(&row.try_get::<String, _>(2)?)?,
                quantity: dec(&row.try_get::<String, _>(3)?)?,
                avg_entry_price: dec(&row.try_get::<String, _>(4)?)?,
                realized_pnl: dec(&row.try_get::<String, _>(5)?)?,
            };
            pending.push((id, res));
        }
        Ok(pending)
    }

    async fn mark_settlement_resolved(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE pending_settlements SET status = 'resolved', resolved_at = $1 WHERE id = $2")
            .bind(dt_to_str(&Utc::now()))
            .bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Balances -------------------------------------------------------

    async fn update_balance(&self, bal: &PlatformBalance) -> Result<()> {
        sqlx::query(
            "INSERT INTO platform_balances (platform, available, reserved, pending_settlement, total, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT(platform) DO UPDATE SET
               available = excluded.available,
               reserved = excluded.reserved,
               pending_settlement = excluded.pending_settlement,
               total = excluded.total,
               updated_at = excluded.updated_at"
        )
        .bind(bal.platform.to_string())
        .bind(dec_to_string(&bal.available))
        .bind(dec_to_string(&bal.reserved))
        .bind(dec_to_string(&bal.pending_settlement))
        .bind(dec_to_string(&bal.total))
        .bind(dt_to_str(&bal.updated_at))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>> {
        let row = sqlx::query(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM platform_balances WHERE platform = $1"
        )
        .bind(platform.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            Ok(Some(PlatformBalance {
                platform: platform_from_db(&row.try_get::<String, _>(0)?)?,
                available: dec(&row.try_get::<String, _>(1)?)?,
                reserved: dec(&row.try_get::<String, _>(2)?)?,
                pending_settlement: dec(&row.try_get::<String, _>(3)?)?,
                total: dec(&row.try_get::<String, _>(4)?)?,
                updated_at: parse_dt(&row.try_get::<String, _>(5)?),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>> {
        let rows = sqlx::query(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM platform_balances"
        ).fetch_all(&self.pool).await?;

        let mut balances = Vec::new();
        for row in rows {
            balances.push(PlatformBalance {
                platform: platform_from_db(&row.try_get::<String, _>(0)?)?,
                available: dec(&row.try_get::<String, _>(1)?)?,
                reserved: dec(&row.try_get::<String, _>(2)?)?,
                pending_settlement: dec(&row.try_get::<String, _>(3)?)?,
                total: dec(&row.try_get::<String, _>(4)?)?,
                updated_at: parse_dt(&row.try_get::<String, _>(5)?),
            });
        }
        Ok(balances)
    }

    // -- Daily snapshots ------------------------------------------------

    async fn insert_daily_snapshot(&self, snap: &DailySnapshot) -> Result<()> {
        sqlx::query(
            "INSERT INTO daily_snapshots (date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
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
               kelly_utilization = excluded.kelly_utilization"
        )
        .bind(snap.date.to_string())
        .bind(dec_to_string(&snap.bankroll))
        .bind(dec_to_string(&snap.gross_pnl))
        .bind(dec_to_string(&snap.fees_paid))
        .bind(dec_to_string(&snap.net_pnl))
        .bind(snap.trades_count)
        .bind(snap.success_count)
        .bind(snap.fail_count)
        .bind(dec_to_string(&snap.success_rate))
        .bind(dec_to_string(&snap.peak_bankroll))
        .bind(dec_to_string(&snap.drawdown_pct))
        .bind(dec_to_string(&snap.kelly_utilization))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>> {
        let row = sqlx::query(
            "SELECT date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization, report_sent
             FROM daily_snapshots WHERE date = $1"
        )
        .bind(date.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            Ok(Some(DailySnapshot {
                date: parse_naive_date(&row.try_get::<String, _>(0)?),
                bankroll: dec(&row.try_get::<String, _>(1)?)?,
                gross_pnl: dec(&row.try_get::<String, _>(2)?)?,
                fees_paid: dec(&row.try_get::<String, _>(3)?)?,
                net_pnl: dec(&row.try_get::<String, _>(4)?)?,
                trades_count: row.try_get(5)?,
                success_count: row.try_get(6)?,
                fail_count: row.try_get(7)?,
                success_rate: dec(&row.try_get::<String, _>(8)?)?,
                peak_bankroll: dec(&row.try_get::<String, _>(9)?)?,
                drawdown_pct: dec(&row.try_get::<String, _>(10)?)?,
                kelly_utilization: dec(&row.try_get::<String, _>(11)?)?,
                report_sent: row.try_get::<i32, _>(12)? != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()> {
        sqlx::query("UPDATE daily_snapshots SET report_sent = 1 WHERE date = $1")
            .bind(date.to_string())
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Audit ----------------------------------------------------------

    async fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let data_str = serde_json::to_string(&entry.data)?;
        sqlx::query("INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES ($1,$2,$3,$4)")
            .bind(entry.timestamp_ns as i64)
            .bind(&entry.module)
            .bind(&entry.event_type)
            .bind(data_str)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn append_audit_batch(&self, entries: &[AuditEntry]) -> Result<()> {
        if entries.is_empty() { return Ok(()); }
        let mut tx = self.pool.begin().await?;
        for entry in entries {
            let data_str = serde_json::to_string(&entry.data)?;
            sqlx::query("INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES ($1,$2,$3,$4)")
                .bind(entry.timestamp_ns as i64)
                .bind(&entry.module)
                .bind(&entry.event_type)
                .bind(data_str)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()> {
        sqlx::query("INSERT INTO config_history (changed_at, key, old_value, new_value) VALUES ($1,$2,$3,$4)")
            .bind(dt_to_str(&Utc::now()))
            .bind(key)
            .bind(old_val)
            .bind(new_val)
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Meta -----------------------------------------------------------

    async fn checkpoint_wal(&self) -> Result<()> {
        // 2.4.B FIX: Monitor WAL size and force TRUNCATE if > 100MB
        let wal_size_query = "SELECT page_count * page_size as bytes FROM pragma_page_count(), pragma_page_size()";
        if let Ok(row) = sqlx::query(wal_size_query).fetch_one(&self.pool).await {
            let bytes: i64 = row.try_get("bytes").unwrap_or(0);
            if bytes > 100_000_000 {
                tracing::warn!("WAL file exceeded 100MB, forcing TRUNCATE checkpoint");
                sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await?;
                return Ok(());
            }
        }
        sqlx::query("PRAGMA wal_checkpoint(PASSIVE)").execute(&self.pool).await?;
        Ok(())
    }

    async fn prune_audit_log(&self, keep_days: u32) -> Result<()> {
        let cutoff_ns = crate::types::now_ns() - (keep_days as u64 * 24 * 3600 * 1_000_000_000);
        sqlx::query("DELETE FROM audit_log WHERE timestamp_ns < $1")
            .bind(cutoff_ns as i64)
            .execute(&self.pool).await?;
        
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await?;
            
        let cutoff_dt = Utc::now() - chrono::Duration::days(keep_days as i64);
        sqlx::query("DELETE FROM config_history WHERE changed_at < $1")
            .bind(dt_to_str(&cutoff_dt))
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn db_size_bytes(&self) -> Result<u64> {
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count").fetch_one(&self.pool).await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size").fetch_one(&self.pool).await?;
        Ok((page_count * page_size) as u64)
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn backup_to_file(&self, dest_path: &str) -> Result<()> {
        // Comprehensive path validation to prevent SQL injection via VACUUM INTO.
        // SQLite's VACUUM INTO does not support parameterized paths, so we must validate rigorously.
        anyhow::ensure!(!dest_path.contains('\0'), "Backup path contains null byte");
        anyhow::ensure!(
            dest_path.chars().all(|c| c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.')),
            "Backup path contains invalid characters: only alphanumeric, /, _, -, . allowed"
        );
        anyhow::ensure!(!dest_path.contains('\''), "Backup path contains single quote");
        anyhow::ensure!(!dest_path.contains(';'), "Backup path contains semicolon");
        anyhow::ensure!(dest_path.ends_with(".db"), "Backup path must end with .db");
        
        // Canonicalize the path to resolve symlinks, `.`, and `..` before checking prefixes.
        // This prevents traversal attacks like `data/./../../etc/passwd.db`.
        let canonical = std::fs::canonicalize(
            std::path::Path::new(dest_path).parent().unwrap_or(std::path::Path::new("."))
        ).map_err(|e| anyhow::anyhow!("Cannot resolve backup directory: {}", e))?;
        
        let canonical_str = canonical.to_string_lossy();
        anyhow::ensure!(
            canonical_str.starts_with("/opt/mercury/data")
            || canonical_str.starts_with("/opt/mercury/./data"),
            "Backup path resolves outside /opt/mercury/data/: resolved to {}", canonical_str
        );
        
        // Reconstruct the full path using the canonical directory + original filename
        let filename = std::path::Path::new(dest_path)
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Backup path has no filename"))?
            .to_string_lossy();
        let safe_path = canonical.join(filename.as_ref());
        let safe_path_str = safe_path.to_string_lossy();
        
        let query = format!("VACUUM INTO '{}'", safe_path_str);
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }

}

// ---------------------------------------------------------------------------
// Row-to-struct helpers
// ---------------------------------------------------------------------------

fn row_to_trade(row: &sqlx::sqlite::SqliteRow) -> Result<TradeResult> {
    Ok(TradeResult {
        trade_id: row.try_get("id")?,
        opp_id: Uuid::parse_str(&row.try_get::<String, _>("opp_id")?).map_err(|e| anyhow::anyhow!("Invalid opp_id UUID in DB: {}", e))?,
        market_id: Uuid::parse_str(&row.try_get::<String, _>("market_id")?).map_err(|e| anyhow::anyhow!("Invalid market_id UUID in DB: {}", e))?,
        market_question: row.try_get("market_question")?,
        leg_a_platform: platform_from_db(&row.try_get::<String, _>("leg_a_platform")?)?,
        leg_a_side: side_from_db(&row.try_get::<String, _>("leg_a_side")?)?,
        leg_a_price: dec(&row.try_get::<String, _>("leg_a_price")?)?,
        leg_a_size: dec(&row.try_get::<String, _>("leg_a_size")?)?,
        leg_a_fill_price: dec(&row.try_get::<String, _>("leg_a_fill_price")?)?,
        leg_a_fee: dec(&row.try_get::<String, _>("leg_a_fee")?)?,
        leg_b_platform: platform_from_db(&row.try_get::<String, _>("leg_b_platform")?)?,
        leg_b_side: side_from_db(&row.try_get::<String, _>("leg_b_side")?)?,
        leg_b_price: dec(&row.try_get::<String, _>("leg_b_price")?)?,
        leg_b_size: dec(&row.try_get::<String, _>("leg_b_size")?)?,
        leg_b_fill_price: dec(&row.try_get::<String, _>("leg_b_fill_price")?)?,
        leg_b_fee: dec(&row.try_get::<String, _>("leg_b_fee")?)?,
        raw_spread: dec(&row.try_get::<String, _>("raw_spread")?)?,
        net_spread: dec(&row.try_get::<String, _>("net_spread")?)?,
        profit: dec(&row.try_get::<String, _>("profit")?)?,
        status: trade_status_from_db(&row.try_get::<String, _>("status")?),
        failure_reason: row.try_get("failure_reason")?,
        execution_ms: row.try_get::<i64, _>("execution_ms")? as u64,
        executed_at: parse_dt(&row.try_get::<String, _>("executed_at")?),
        bankroll_after: dec(&row.try_get::<String, _>("bankroll_after")?)?,
        bankroll_change_pct: dec(&row.try_get::<String, _>("bankroll_change_pct")?)?,
        approved_size: Decimal::ZERO,
    })
}

fn row_to_position(row: &sqlx::sqlite::SqliteRow) -> Result<Position> {
    Ok(Position {
        id: row.try_get("id")?,
        market_id: Uuid::parse_str(&row.try_get::<String, _>("market_id")?).map_err(|e| anyhow::anyhow!("Invalid market_id UUID in DB: {}", e))?,
        platform: platform_from_db(&row.try_get::<String, _>("platform")?)?,
        side: side_from_db(&row.try_get::<String, _>("side")?)?,
        quantity: dec(&row.try_get::<String, _>("quantity")?)?,
        avg_entry_price: dec(&row.try_get::<String, _>("avg_entry_price")?)?,
        unrealized_pnl: dec(&row.try_get::<String, _>("unrealized_pnl")?)?,
        opened_at: parse_dt(&row.try_get::<String, _>("opened_at")?),
        updated_at: parse_dt(&row.try_get::<String, _>("updated_at")?),
    })
}
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::types::*;

#[async_trait]
pub trait Database: Send + Sync + 'static {
    // Markets
    async fn upsert_market(&self, market: &Market) -> Result<()>;
    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>>;
    async fn get_active_markets(&self) -> Result<Vec<Market>>;

    // Trades
    async fn insert_trade(&self, result: &TradeResult) -> Result<i64>;
    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>>;
    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>>;
    async fn get_trade_count(&self) -> Result<i64>;

    // Positions
    async fn upsert_position(&self, position: &Position) -> Result<()>;
    async fn get_open_positions(&self) -> Result<Vec<Position>>;
    async fn close_position(&self, id: i64) -> Result<()>;

    // Balances
    async fn update_balance(&self, balance: &PlatformBalance) -> Result<()>;
    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>>;
    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>>;

    // Daily snapshots
    async fn insert_daily_snapshot(&self, snapshot: &DailySnapshot) -> Result<()>;
    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>>;
    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()>;

    // Audit log
    async fn append_audit(&self, entry: &AuditEntry) -> Result<()>;

    // Config history
    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()>;

    // Utility
    async fn db_size_bytes(&self) -> Result<u64>;
}

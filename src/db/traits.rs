use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::types::{
    DailySnapshot, Market, PlatformBalance, Position, Platform, TradeResult,
};

#[async_trait]
pub trait Database: Send + Sync {
    // Markets
    async fn upsert_market(&self, market: &Market) -> Result<()>;
    async fn get_market(&self, id: &str) -> Result<Option<Market>>;
    async fn get_active_markets(&self) -> Result<Vec<Market>>;

    // Trades
    async fn insert_trade(&self, trade: &TradeResult) -> Result<()>;
    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>>;
    async fn get_trades_for_date(&self, date: &str) -> Result<Vec<TradeResult>>;
    async fn get_trade_count(&self) -> Result<u64>;

    // Positions
    async fn upsert_position(&self, position: &Position) -> Result<()>;
    async fn get_open_positions(&self) -> Result<Vec<Position>>;
    async fn close_position(&self, id: &uuid::Uuid, close_time: DateTime<Utc>) -> Result<()>;

    // Balances
    async fn update_balance(&self, balance: &PlatformBalance) -> Result<()>;
    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>>;
    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>>;

    // Daily snapshots
    async fn insert_daily_snapshot(&self, snapshot: &DailySnapshot) -> Result<()>;
    async fn get_daily_snapshot(&self, date: &str) -> Result<Option<DailySnapshot>>;
    async fn mark_report_sent(&self, date: &str) -> Result<()>;

    // Audit
    async fn append_audit(&self, action: &str, details: &str) -> Result<()>;
    async fn log_config_change(&self, field: &str, old_val: &str, new_val: &str) -> Result<()>;

    // Meta
    async fn db_size_bytes(&self) -> Result<u64>;
}

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::types::*;

#[async_trait]
pub trait Database: Send + Sync + 'static {
    // Markets
    async fn upsert_market(&self, market: &Market) -> Result<()>;
    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>>;
    async fn get_active_markets(&self) -> Result<Vec<Market>>;
    async fn get_suspended_markets(&self) -> Result<Vec<Market>>;
    async fn update_market_status(&self, id: &Uuid, status: MarketStatus) -> Result<()>;

    // Trades
    async fn insert_trade(&self, result: &TradeResult) -> Result<i64>;
    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>>;
    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>>;
    async fn get_trade_count(&self) -> Result<i64>;
    /// Count distinct in-flight arbitrage pairs (not individual position records).
    async fn get_open_arb_count(&self) -> Result<usize>;
    /// Sum cumulative profit of all successful trades to recover state post-crash
    async fn get_cumulative_profit(&self) -> Result<Decimal>;

    // Positions
    async fn upsert_position(&self, position: &Position) -> Result<()>;
    /// Upsert two positions (both legs of an arbitrage) atomically.
    async fn upsert_position_pair(&self, pos_a: &Position, pos_b: &Position) -> Result<()>;
    async fn get_open_positions(&self) -> Result<Vec<Position>>;
    async fn close_position(&self, id: i64) -> Result<()>;
    
    // Settlement Queue
    async fn enqueue_settlement(&self, position_id: i64, market_id: &Uuid, platform: Platform, quantity: Decimal, avg_entry: Decimal, pnl: Decimal) -> Result<()>;
    async fn get_pending_settlements(&self) -> Result<Vec<(i64, SettlementResult)>>;
    async fn mark_settlement_resolved(&self, id: i64) -> Result<()>;

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
    /// Batch-insert multiple audit entries in a single transaction.
    async fn append_audit_batch(&self, entries: &[AuditEntry]) -> Result<()>;

    // Config history
    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()>;

    // Utility
    async fn prune_audit_log(&self, keep_days: u32) -> Result<()>;
    async fn checkpoint_wal(&self) -> Result<()>;
    async fn db_size_bytes(&self) -> Result<u64>;
    /// Create an atomic backup of the database to the given file path.
    async fn backup_to_file(&self, dest_path: &str) -> Result<()>;
}

/// Unit tests for Kalshi feed message parsing.
/// Declared from kalshi.rs as a child module (has access to private items).
use super::*;
use rust_decimal_macros::dec;
use tokio::sync::broadcast;
use uuid::Uuid;

// ─── Mock Database ───────────────────────────────────────────────────────────

struct MockDb;

#[async_trait::async_trait]
impl crate::db::Database for MockDb {
    async fn upsert_market(&self, _: &crate::types::Market) -> anyhow::Result<()> { Ok(()) }
    async fn get_market(&self, _: &Uuid) -> anyhow::Result<Option<crate::types::Market>> { Ok(None) }
    async fn get_active_markets(&self) -> anyhow::Result<Vec<crate::types::Market>> { Ok(vec![]) }
    async fn get_suspended_markets(&self) -> anyhow::Result<Vec<crate::types::Market>> { Ok(vec![]) }
    async fn update_market_status(&self, _: &Uuid, _: crate::types::MarketStatus) -> anyhow::Result<()> { Ok(()) }
    async fn insert_trade(&self, _: &crate::types::TradeResult) -> anyhow::Result<i64> { Ok(1) }
    async fn get_trades_since(&self, _: chrono::DateTime<chrono::Utc>) -> anyhow::Result<Vec<crate::types::TradeResult>> { Ok(vec![]) }
    async fn get_trades_for_date(&self, _: chrono::NaiveDate) -> anyhow::Result<Vec<crate::types::TradeResult>> { Ok(vec![]) }
    async fn get_trade_count(&self) -> anyhow::Result<i64> { Ok(0) }
    async fn get_open_arb_count(&self) -> anyhow::Result<usize> { Ok(0) }
    async fn get_cumulative_profit(&self) -> anyhow::Result<rust_decimal::Decimal> { Ok(rust_decimal::Decimal::ZERO) }
    async fn upsert_position(&self, _: &crate::types::Position) -> anyhow::Result<()> { Ok(()) }
    async fn upsert_position_pair(&self, _: &crate::types::Position, _: &crate::types::Position) -> anyhow::Result<()> { Ok(()) }
    async fn get_open_positions(&self) -> anyhow::Result<Vec<crate::types::Position>> { Ok(vec![]) }
    async fn close_position(&self, _: i64) -> anyhow::Result<()> { Ok(()) }
    async fn enqueue_settlement(&self, _: i64, _: &Uuid, _: crate::types::Platform, _: rust_decimal::Decimal, _: rust_decimal::Decimal, _: rust_decimal::Decimal) -> anyhow::Result<()> { Ok(()) }
    async fn get_pending_settlements(&self) -> anyhow::Result<Vec<(i64, crate::types::SettlementResult)>> { Ok(vec![]) }
    async fn mark_settlement_resolved(&self, _: i64) -> anyhow::Result<()> { Ok(()) }
    async fn update_balance(&self, _: &crate::types::PlatformBalance) -> anyhow::Result<()> { Ok(()) }
    async fn get_balance(&self, _: crate::types::Platform) -> anyhow::Result<Option<crate::types::PlatformBalance>> { Ok(None) }
    async fn get_all_balances(&self) -> anyhow::Result<Vec<crate::types::PlatformBalance>> { Ok(vec![]) }
    async fn insert_daily_snapshot(&self, _: &crate::types::DailySnapshot) -> anyhow::Result<()> { Ok(()) }
    async fn get_daily_snapshot(&self, _: chrono::NaiveDate) -> anyhow::Result<Option<crate::types::DailySnapshot>> { Ok(None) }
    async fn mark_report_sent(&self, _: chrono::NaiveDate) -> anyhow::Result<()> { Ok(()) }
    async fn append_audit(&self, _: &crate::types::AuditEntry) -> anyhow::Result<()> { Ok(()) }
    async fn append_audit_batch(&self, _: &[crate::types::AuditEntry]) -> anyhow::Result<()> { Ok(()) }
    async fn log_config_change(&self, _: &str, _: &str, _: &str) -> anyhow::Result<()> { Ok(()) }
    async fn prune_audit_log(&self, _: u32) -> anyhow::Result<()> { Ok(()) }
    async fn checkpoint_wal(&self) -> anyhow::Result<()> { Ok(()) }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> { Ok(0) }
    async fn backup_to_file(&self, _: &str) -> anyhow::Result<()> { Ok(()) }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_kalshi_feed(ticker: &str) -> (KalshiFeed, Uuid) {
    let config = crate::config::KalshiConfig {
        enabled: true,
        ws_url: "ws://test".into(),
        rest_url: "http://test".into(),
    };
    let market_id = Uuid::new_v4();
    let feed = KalshiFeed::new(
        config,
        None, // No auth for tests
        std::sync::Arc::new(MockDb),
        vec![(ticker.to_string(), market_id)],
    );
    (feed, market_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_snapshot_yes_bids_map_to_bids() {
    let (mut feed, _) = make_kalshi_feed("KBTC");
    let (tx, mut rx) = broadcast::channel(16);

    // YES bids should map to bids; NO bids should invert to YES asks
    let json = r#"{"type":"orderbook_snapshot","msg":{"market_ticker":"KBTC","yes":[["0.40","100"]],"no":[["0.35","50"]]}}"#;
    feed.handle_message(json, &tx).unwrap();

    let tick = rx.try_recv().expect("tick after snapshot");
    // YES bid at 0.40 → bid_price = 0.40
    assert_eq!(tick.bid_price, dec!(0.40));
    // NO bid at 0.35 → YES ask at 1 - 0.35 = 0.65
    assert_eq!(tick.ask_price, dec!(0.65));
}

#[test]
fn test_snapshot_no_inversion_correct() {
    let (mut feed, _) = make_kalshi_feed("KETH");
    let (tx, mut rx) = broadcast::channel(16);

    // NO bid at 0.60 → YES ask at 1 - 0.60 = 0.40
    let json = r#"{"type":"orderbook_snapshot","msg":{"market_ticker":"KETH","yes":[["0.35","100"]],"no":[["0.60","80"]]}}"#;
    feed.handle_message(json, &tx).unwrap();

    let tick = rx.try_recv().expect("tick after snapshot");
    assert_eq!(tick.bid_price, dec!(0.35));
    assert_eq!(tick.ask_price, dec!(0.40)); // 1 - 0.60 = 0.40
}

#[test]
fn test_delta_incremental_update() {
    let (mut feed, _) = make_kalshi_feed("KDELTA");
    let (tx, mut rx) = broadcast::channel(16);

    // First establish snapshot
    let snap = r#"{"type":"orderbook_snapshot","msg":{"market_ticker":"KDELTA","yes":[["0.40","100"]],"no":[["0.35","50"]]}}"#;
    feed.handle_message(snap, &tx).unwrap();
    let _ = rx.try_recv();

    // Apply delta: update YES bid to 0.42
    let delta = r#"{"type":"orderbook_delta","msg":{"market_ticker":"KDELTA","seq":1,"price_deltas":{"yes":[["0.42","80"]],"no":[]}}}"#;
    feed.handle_message(delta, &tx).unwrap();

    let tick = rx.try_recv().expect("tick after delta");
    assert_eq!(tick.bid_price, dec!(0.42), "bid should update to 0.42");
}

#[test]
fn test_delta_before_snapshot_returns_error() {
    let (mut feed, _) = make_kalshi_feed("KNOSNAP");
    let (tx, _rx) = broadcast::channel(16);

    // Send delta without any preceding snapshot
    let delta = r#"{"type":"orderbook_delta","msg":{"market_ticker":"KNOSNAP","seq":1,"price_deltas":{"yes":[["0.42","80"]],"no":[]}}}"#;
    let result = feed.handle_message(delta, &tx);
    assert!(result.is_err(), "delta before snapshot should return Err");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("snapshot") || err_str.contains("reconnect"),
        "error should mention snapshot: {}",
        err_str
    );
}

#[test]
fn test_sequence_gap_returns_error() {
    let (mut feed, _) = make_kalshi_feed("KGAP");
    let (tx, _rx) = broadcast::channel(16);

    // Snapshot first
    let snap = r#"{"type":"orderbook_snapshot","msg":{"market_ticker":"KGAP","yes":[["0.40","100"]],"no":[["0.35","50"]]}}"#;
    feed.handle_message(snap, &tx).unwrap();

    // Send seq=1 then skip to seq=5
    let delta1 = r#"{"type":"orderbook_delta","msg":{"market_ticker":"KGAP","seq":1,"price_deltas":{"yes":[],"no":[]}}}"#;
    feed.handle_message(delta1, &tx).unwrap();

    let delta_gap = r#"{"type":"orderbook_delta","msg":{"market_ticker":"KGAP","seq":5,"price_deltas":{"yes":[["0.45","50"]],"no":[]}}}"#;
    let result = feed.handle_message(delta_gap, &tx);
    assert!(result.is_err(), "sequence gap should return Err");
}

#[test]
fn test_dollar_formatted_prices_not_divided_by_100() {
    // Kalshi API v2 sends "0.4200" not "42" — must NOT divide by 100
    let (mut feed, _) = make_kalshi_feed("KPRICE");
    let (tx, mut rx) = broadcast::channel(16);

    let json = r#"{"type":"orderbook_snapshot","msg":{"market_ticker":"KPRICE","yes":[["0.4200","100"]],"no":[["0.3500","50"]]}}"#;
    feed.handle_message(json, &tx).unwrap();

    let tick = rx.try_recv().expect("tick after snapshot");
    // Must be 0.42, NOT 0.0042 (which would happen if /100 was applied)
    assert_eq!(tick.bid_price, dec!(0.42), "Kalshi price must not be divided by 100");
    assert!(tick.bid_price > dec!(0.10), "price should be in dollar format range");
}

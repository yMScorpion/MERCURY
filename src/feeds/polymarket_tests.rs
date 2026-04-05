/// Unit tests for Polymarket feed message parsing.
/// Declared from polymarket.rs as a child module (has access to private items).
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

fn make_feed(asset_id: &str) -> (PolymarketFeed, Uuid) {
    let config = crate::config::PolymarketConfig {
        enabled: true,
        ws_url: "ws://test".into(),
        rest_url: "http://test".into(),
        sports_ws_url: "ws://test".into(),
    };
    let market_id = Uuid::new_v4();
    let feed = PolymarketFeed::new(
        config,
        std::sync::Arc::new(MockDb),
        vec![(asset_id.to_string(), market_id, 200)],
    );
    (feed, market_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_book_snapshot_populates_bids_asks() {
    let (mut feed, _) = make_feed("asset1");
    let (tx, mut rx) = broadcast::channel(16);

    let json = r#"{"event_type":"book","asset_id":"asset1","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}]}"#;
    feed.handle_message(json, &tx).unwrap();

    let tick = rx.try_recv().expect("tick should be emitted after book snapshot");
    assert_eq!(tick.bid_price, dec!(0.40));
    assert_eq!(tick.ask_price, dec!(0.60));
    assert_eq!(tick.bid_size, dec!(100));
}

#[test]
fn test_price_change_incremental_update() {
    let (mut feed, _) = make_feed("asset2");
    let (tx, mut rx) = broadcast::channel(16);

    // Establish book via snapshot
    let snap = r#"{"event_type":"book","asset_id":"asset2","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}],"sequence":1}"#;
    feed.handle_message(snap, &tx).unwrap();
    let _ = rx.try_recv();

    // Apply incremental update — new bid at 0.42
    let update = r#"{"event_type":"price_change","asset_id":"asset2","changes":[{"side":"BUY","price":"0.42","size":"50"}],"sequence":2}"#;
    feed.handle_message(update, &tx).unwrap();

    let tick = rx.try_recv().expect("tick should be emitted after price_change");
    assert_eq!(tick.bid_price, dec!(0.42), "bid should update to new level");
}

#[test]
fn test_price_change_zero_size_removes_level() {
    let (mut feed, _) = make_feed("asset3");
    let (tx, mut rx) = broadcast::channel(16);

    let snap = r#"{"event_type":"book","asset_id":"asset3","bids":[{"price":"0.40","size":"100"},{"price":"0.38","size":"50"}],"asks":[{"price":"0.60","size":"80"}],"sequence":1}"#;
    feed.handle_message(snap, &tx).unwrap();
    let _ = rx.try_recv();

    // Remove bid level at 0.38 by sending size=0
    let update = r#"{"event_type":"price_change","asset_id":"asset3","changes":[{"side":"BUY","price":"0.38","size":"0"}],"sequence":2}"#;
    feed.handle_message(update, &tx).unwrap();

    // Verify tick is emitted; the removed level shouldn't affect the best bid if 0.40 remains
    let tick = rx.try_recv().expect("tick after remove");
    assert_eq!(tick.bid_price, dec!(0.40)); // Best bid unchanged, 0.38 was removed
}

#[test]
fn test_sequence_gap_returns_error() {
    let (mut feed, _) = make_feed("asset4");
    let (tx, _rx) = broadcast::channel(16);

    let snap = r#"{"event_type":"book","asset_id":"asset4","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}],"sequence":3}"#;
    feed.handle_message(snap, &tx).unwrap();

    // Skip seq 4 — send seq 6 directly
    let gap = r#"{"event_type":"price_change","asset_id":"asset4","changes":[{"side":"BUY","price":"0.41","size":"10"}],"sequence":6}"#;
    let result = feed.handle_message(gap, &tx);
    assert!(result.is_err(), "sequence gap should return Err");
    let err_str = result.unwrap_err().to_string();
    assert!(err_str.contains("sequence gap") || err_str.contains("gap"), "error should mention gap: {}", err_str);
}

#[test]
fn test_duplicate_sequence_silently_dropped() {
    let (mut feed, _) = make_feed("asset5");
    let (tx, mut rx) = broadcast::channel(16);

    let snap = r#"{"event_type":"book","asset_id":"asset5","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}],"sequence":5}"#;
    feed.handle_message(snap, &tx).unwrap();
    let _ = rx.try_recv();

    // Replay seq 5 — should be silently dropped, no error, no tick
    let dup = r#"{"event_type":"price_change","asset_id":"asset5","changes":[{"side":"BUY","price":"0.99","size":"999"}],"sequence":5}"#;
    let result = feed.handle_message(dup, &tx);
    assert!(result.is_ok(), "duplicate sequence should return Ok (silently dropped)");
    assert!(rx.try_recv().is_err(), "no tick should be emitted for duplicate sequence");
}

#[test]
fn test_last_trade_price_no_tick_emitted() {
    let (mut feed, _) = make_feed("asset6");
    let (tx, mut rx) = broadcast::channel(16);

    // Set up book first
    let snap = r#"{"event_type":"book","asset_id":"asset6","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}]}"#;
    feed.handle_message(snap, &tx).unwrap();
    let _ = rx.try_recv();

    // last_trade_price should NOT emit a tick
    let ltp = r#"{"event_type":"last_trade_price","asset_id":"asset6","price":"0.45"}"#;
    feed.handle_message(ltp, &tx).unwrap();

    assert!(rx.try_recv().is_err(), "last_trade_price should not emit a new tick");
}

#[test]
fn test_malformed_json_returns_ok_no_panic() {
    let (mut feed, _) = make_feed("asset7");
    let (tx, _rx) = broadcast::channel(16);

    // Malformed JSON must not panic and must return Ok
    let result = feed.handle_message("not valid json {{{{", &tx);
    assert!(result.is_ok(), "malformed JSON should return Ok (no panic): {:?}", result);
}

#[test]
fn test_empty_asset_id_and_market_skips_processing() {
    let (mut feed, _) = make_feed("asset8");
    let (tx, mut rx) = broadcast::channel(16);

    // Both asset_id and market are empty — should skip
    let json = r#"{"event_type":"book","asset_id":"","market":"","bids":[{"price":"0.40","size":"100"}],"asks":[{"price":"0.60","size":"80"}]}"#;
    let result = feed.handle_message(json, &tx);
    assert!(result.is_ok(), "empty asset_id should return Ok");
    assert!(rx.try_recv().is_err(), "no tick should be emitted");
}

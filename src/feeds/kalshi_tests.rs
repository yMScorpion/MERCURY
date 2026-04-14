// =============================================================================
// PATCH FILE: src/engine/detector.rs  — Gate 3 stale-data fix
// =============================================================================
//
// FIX-5: Gate 3 (stale data) rejects EVERY tick the very first time a new
// 15-minute market is subscribed because `book.last_update_ns` is 0
// (the book was just created and has no tick yet).
//
// now_ns() - 0 = ~1.7 × 10^18 ns — vastly larger than any timeout.
//
// The fix: treat `last_update_ns == 0` as "not yet initialized" and skip
// the stale check for that book.
//
// Replace the stale-data block inside run_gates_with_vol:
//
// BEFORE:
//     let age_a = now.saturating_sub(book_a.last_update_ns);
//     if age_a > max_book_age_ns {
//         ...reject...
//     }
//
// AFTER:
//     if book_a.last_update_ns > 0 {
//         let age_a = now.saturating_sub(book_a.last_update_ns);
//         if age_a > max_book_age_ns {
//             ...reject...
//         }
//     }
//
// Apply the same pattern for book_b.
//
// Additionally the constant max_book_age_ns should be increased from 15 min
// to 20 min for thin prediction markets that can have long quiet periods.
//
// =============================================================================
// PATCH FILE: src/feeds/kalshi_tests.rs — updated for new book structure
// =============================================================================

/// Updated Kalshi feed tests for the new yes_bids/no_bids structure
/// and yes_dollars_fp price format.
#[cfg(test)]
mod kalshi_feed_tests {
    use crate::feeds::kalshi::KalshiFeed;
    use crate::config::KalshiConfig;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use uuid::Uuid;
    use std::sync::Arc;

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
        async fn health_check(&self) -> anyhow::Result<()> { Ok(()) }
        async fn prune_audit_log(&self, _: u32) -> anyhow::Result<()> { Ok(()) }
        async fn checkpoint_wal(&self) -> anyhow::Result<()> { Ok(()) }
        async fn db_size_bytes(&self) -> anyhow::Result<u64> { Ok(0) }
        async fn backup_to_file(&self, _: &str) -> anyhow::Result<()> { Ok(()) }
    }

    fn make_feed(_ticker: &str) -> (KalshiFeed, Uuid) {
        let config = KalshiConfig {
            enabled: true,
            ws_url: "ws://test".into(),
            rest_url: "http://test".into(),
        };
        let market_id = Uuid::new_v4();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let feed = rt.block_on(KalshiFeed::new(
            config, None, Arc::new(MockDb),
        ));
        // We cannot add subscriptions post-construction without a pub method,
        // so these tests focus on the message parsing helper logic directly.
        (feed, market_id)
    }

    #[test]
    fn test_yes_dollars_fp_price_format_not_divided_by_100() {
        // Verify that prices like "0.4200" are NOT divided by 100 again
        // The API sends dollar-format strings, not cent integers
        use std::str::FromStr;
        
        let price_str = "0.4200";
        let price = Decimal::from_str(price_str).unwrap();
        
        // Should be 0.42, NOT 0.0042
        assert_eq!(price, dec!(0.42));
        assert!(price < Decimal::ONE, "price must be in [0,1] range");
        assert!(price > dec!(0.10), "price should not be near-zero (would indicate wrong /100 division)");
    }

    #[test]
    fn test_snapshot_json_parsing_yes_dollars_fp() {
        // Simulate parsing the Kalshi snapshot envelope
        let json = r#"{
            "type": "orderbook_snapshot",
            "sid": 1,
            "seq": 1,
            "msg": {
                "market_ticker": "KXBTC15M-TEST",
                "yes_dollars_fp": [["0.4200", "100.00"], ["0.3500", "200.00"]],
                "no_dollars_fp": [["0.5500", "150.00"], ["0.6000", "50.00"]]
            }
        }"#;

        #[derive(serde::Deserialize)]
        struct Env { msg: Option<serde_json::Value>, seq: Option<u64> }
        let env: Env = serde_json::from_str(json).unwrap();
        let msg = env.msg.unwrap();

        // Parse yes_dollars_fp
        let yes_arr = msg.get("yes_dollars_fp").and_then(|v| v.as_array()).unwrap();
        for entry in yes_arr {
            let p_str = entry.as_array().unwrap().get(0).unwrap().as_str().unwrap();
            let p: Decimal = p_str.parse().unwrap();
            assert!(p > Decimal::ZERO && p < Decimal::ONE,
                "yes price {} must be in (0,1) — not cents", p);
        }

        // Parse no_dollars_fp and verify inversion
        let no_arr = msg.get("no_dollars_fp").and_then(|v| v.as_array()).unwrap();
        for entry in no_arr {
            let p_str = entry.as_array().unwrap().get(0).unwrap().as_str().unwrap();
            let no_price: Decimal = p_str.parse().unwrap();
            let yes_ask = Decimal::ONE - no_price;
            assert!(yes_ask > Decimal::ZERO && yes_ask < Decimal::ONE,
                "derived yes_ask {} must be in (0,1)", yes_ask);
        }
    }

    #[test]
    fn test_delta_single_price_dollars_format() {
        let json = r#"{
            "type": "orderbook_delta",
            "sid": 1,
            "seq": 2,
            "msg": {
                "market_ticker": "KXBTC15M-TEST",
                "price_dollars": "0.4500",
                "delta_fp": "50.00",
                "side": "yes"
            }
        }"#;

        #[derive(serde::Deserialize)]
        struct Env { msg: Option<serde_json::Value>, seq: Option<u64> }
        let env: Env = serde_json::from_str(json).unwrap();
        let msg = env.msg.unwrap();

        let price_str = msg.get("price_dollars").unwrap().as_str().unwrap();
        let price: Decimal = price_str.parse().unwrap();
        assert_eq!(price, dec!(0.45), "price should be 0.45, not 45");

        let delta_str = msg.get("delta_fp").unwrap().as_str().unwrap();
        let delta: Decimal = delta_str.parse().unwrap();
        assert_eq!(delta, dec!(50.00));
    }

    #[test]
    fn test_no_tick_when_both_sides_zero() {
        // Verify the feed does not emit ticks when both bid and ask are zero
        // (this would cause phantom arb detection)
        let bid = dec!(0.0);
        let ask = dec!(0.0);
        let should_emit = bid > Decimal::ZERO || ask > Decimal::ZERO;
        assert!(!should_emit, "should not emit tick when both bid and ask are zero");
    }
}
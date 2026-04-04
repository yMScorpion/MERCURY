//! Integration tests for the full MERCURY pipeline:
//! WS tick → UnifiedOrderBook → SpreadEngine → ArbitrageDetector → Execution
//!
//! These tests use in-memory SQLite and mock platform clients to verify
//! end-to-end correctness without network dependencies.

#[cfg(test)]
mod pipeline_tests {
    use crate::db::SqliteDb;
    use crate::engine::detector::ArbitrageDetector;
    use crate::engine::market_registry::MarketRegistry;
    use crate::engine::order_book::UnifiedOrderBook;
    use crate::engine::spread::NetSpreadEngine;
    use crate::risk::bankroll::BankrollManager;
    use crate::risk::circuit_breaker::CircuitBreakers;
    use crate::risk::kelly::KellyCalculator;
    use crate::types::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Helper: create an in-memory SQLite DB for testing.
    async fn test_db() -> Arc<SqliteDb> {
        Arc::new(
            SqliteDb::new(":memory:", 1, 5000)
                .await
                .expect("Failed to create in-memory test DB"),
        )
    }

    /// Helper: register a two-platform market in the registry.
    fn register_test_market(
        registry: &mut MarketRegistry,
        market_id: Uuid,
        question: &str,
        poly_token: &str,
        kalshi_ticker: &str,
    ) {
        let mut platforms = HashMap::new();
        platforms.insert(
            Platform::Polymarket,
            PlatformMarketInfo {
                platform: Platform::Polymarket,
                platform_market_id: poly_token.into(),
                fee_rate_bps: 200,
                min_order_size: dec!(1.0),
                tick_size: dec!(0.01),
            },
        );
        platforms.insert(
            Platform::Kalshi,
            PlatformMarketInfo {
                platform: Platform::Kalshi,
                platform_market_id: kalshi_ticker.into(),
                fee_rate_bps: 175,
                min_order_size: dec!(1.0),
                tick_size: dec!(0.01),
            },
        );

        registry.register_market(Market {
            unified_id: market_id,
            question: question.into(),
            resolution_source: "test".into(),
            expiration: chrono::Utc::now() + chrono::Duration::hours(2),
            platforms,
            category: MarketCategory::Other,
            confidence: 0.99,
            status: MarketStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
    }

    /// Helper: create a normalized tick with specified BBO and depth.
    fn make_tick(
        platform: Platform,
        market_id: Uuid,
        bid: Decimal,
        bid_size: Decimal,
        ask: Decimal,
        ask_size: Decimal,
        fee_bps: u16,
        seq: u64,
    ) -> NormalizedTick {
        let mid = (bid + ask) / Decimal::from(2);
        NormalizedTick {
            platform,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid,
            bid_size,
            ask_price: ask,
            ask_size,
            mid_price: mid,
            last_trade_price: mid,
            last_trade_size: dec!(10),
            book_depth: {
                let mut depth = arrayvec::ArrayVec::new();
                depth.push(PriceLevel { price: ask, size: ask_size });
                depth.push(PriceLevel { price: bid, size: bid_size });
                depth
            },
            fee_rate_bps: fee_bps,
            sequence: seq,
        }
    }

    // ─── Test 1: Full pipeline detects arb opportunity ───

    #[tokio::test]
    async fn test_pipeline_detects_arb_opportunity() {
        let mut registry = MarketRegistry::new();
        let market_id = Uuid::new_v4();
        register_test_market(&mut registry, market_id, "Will BTC exceed $100k?", "poly_btc", "KBTC100K");

        let mut uob = UnifiedOrderBook::new();
        let spread_engine = NetSpreadEngine::new(dec!(0.01));
        let mut detector = ArbitrageDetector::new(dec!(0.01), dec!(1.0), 5000, 3);

        // Polymarket: Ask YES at 0.40 (cheap YES)
        uob.update(&make_tick(Platform::Polymarket, market_id, dec!(0.38), dec!(100), dec!(0.40), dec!(100), 200, 1));
        // Kalshi: Bid YES at 0.50 → implies NO can be bought at 0.50
        uob.update(&make_tick(Platform::Kalshi, market_id, dec!(0.50), dec!(100), dec!(0.52), dec!(100), 175, 1));

        let opps = detector.detect_for_market(&market_id, &registry, &uob, &spread_engine, dec!(10.0));

        assert!(!opps.is_empty(), "Should detect at least 1 arb opportunity");
        let opp = &opps[0];
        assert!(opp.net_spread > Decimal::ZERO, "Net spread should be positive");
        assert!(opp.raw_spread > opp.net_spread, "Raw spread should exceed net spread (fees reduce it)");
    }

    // ─── Test 2: No arb when spread is too tight ───

    #[tokio::test]
    async fn test_pipeline_no_arb_tight_spread() {
        let mut registry = MarketRegistry::new();
        let market_id = Uuid::new_v4();
        register_test_market(&mut registry, market_id, "Test tight spread", "poly_t", "KT");

        let mut uob = UnifiedOrderBook::new();
        let spread_engine = NetSpreadEngine::new(dec!(0.02));
        let mut detector = ArbitrageDetector::new(dec!(0.02), dec!(1.0), 5000, 3);

        // Both platforms at essentially the same price — no arb
        uob.update(&make_tick(Platform::Polymarket, market_id, dec!(0.49), dec!(100), dec!(0.51), dec!(100), 200, 1));
        uob.update(&make_tick(Platform::Kalshi, market_id, dec!(0.49), dec!(100), dec!(0.51), dec!(100), 175, 1));

        let opps = detector.detect_for_market(&market_id, &registry, &uob, &spread_engine, dec!(10.0));
        assert!(opps.is_empty(), "No arb should be detected with identical prices");
    }

    // ─── Test 3: Stale data rejects opportunity ───

    #[tokio::test]
    async fn test_pipeline_stale_data_rejection() {
        let mut registry = MarketRegistry::new();
        let market_id = Uuid::new_v4();
        register_test_market(&mut registry, market_id, "Test stale", "poly_s", "KS");

        let mut uob = UnifiedOrderBook::new();
        let spread_engine = NetSpreadEngine::new(dec!(0.01));
        // 100ms stale timeout — ticks will be stale by the time we detect
        let mut detector = ArbitrageDetector::new(dec!(0.01), dec!(1.0), 100, 3);

        // Insert ticks with old timestamps
        let mut tick_a = make_tick(Platform::Polymarket, market_id, dec!(0.30), dec!(100), dec!(0.32), dec!(100), 200, 1);
        tick_a.timestamp_ns = now_ns() - 200_000_000; // 200ms ago
        uob.update(&tick_a);

        let mut tick_b = make_tick(Platform::Kalshi, market_id, dec!(0.60), dec!(100), dec!(0.62), dec!(100), 175, 1);
        tick_b.timestamp_ns = now_ns() - 200_000_000;
        uob.update(&tick_b);

        let opps = detector.detect_for_market(&market_id, &registry, &uob, &spread_engine, dec!(10.0));
        assert!(opps.is_empty(), "Stale ticks should be rejected by gate 3");
        assert!(detector.stats.gate3_rejected > 0, "Gate 3 should have rejected");
    }

    // ─── Test 4: Circuit breaker halts trading ───

    #[tokio::test]
    async fn test_circuit_breaker_daily_loss_halt() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05), dec!(0.03), dec!(0.10), dec!(0.40), 100, 5, 20,
        );

        // daily_loss_pct is 4% (percent-scale), max is 3% (fraction-scale → 3% percent-scale)
        let trips = cb.check_all(
            dec!(100), dec!(10000), dec!(4.0), dec!(0), dec!(0), 0, false, 100, dec!(0),
        );

        assert!(!trips.is_empty(), "CB2 should trip on daily loss exceeding limit");
        assert!(cb.is_trading_halted(), "Trading should be halted after CB2 trip");
    }

    // ─── Test 5: Kelly sizing respects limits ───

    #[tokio::test]
    async fn test_kelly_sizing_respects_caps() {
        let kelly = KellyCalculator::new(dec!(0.25), dec!(0.02));

        let size = kelly.position_size(dec!(10000), dec!(0.90), dec!(0.05), dec!(0.02));
        let max_allowed = dec!(10000) * dec!(0.02); // $200
        assert!(size <= max_allowed, "Kelly size {} should not exceed max_single_trade_pct cap {}", size, max_allowed);
        assert!(size > Decimal::ZERO, "Kelly size should be positive for profitable opportunity");
    }

    // ─── Test 6: Order book crossed-book prevention ───

    #[tokio::test]
    async fn test_order_book_uncrossing() {
        let mut book = crate::engine::order_book::PlatformBook::new(Platform::Polymarket, Uuid::new_v4());

        // Insert normal book state
        book.bids.insert(dec!(0.45), dec!(50));
        book.bids.insert(dec!(0.44), dec!(30));
        book.asks.insert(dec!(0.55), dec!(50));
        book.asks.insert(dec!(0.56), dec!(30));

        // Apply a tick that would cross the book (bid at 0.57, above existing asks)
        let tick = NormalizedTick {
            platform: Platform::Polymarket,
            market_id: book.market_id,
            timestamp_ns: now_ns(),
            bid_price: dec!(0.57),
            bid_size: dec!(100),
            ask_price: dec!(0.58),
            ask_size: dec!(100),
            mid_price: dec!(0.575),
            last_trade_price: dec!(0.57),
            last_trade_size: dec!(10),
            book_depth: arrayvec::ArrayVec::new(),
            fee_rate_bps: 200,
            sequence: 1,
        };
        book.update_from_tick(&tick);

        // Verify asks below bid are removed
        assert!(book.asks.get(&dec!(0.55)).is_none(), "Ask at 0.55 should be removed (below new bid 0.57)");
        assert!(book.asks.get(&dec!(0.56)).is_none(), "Ask at 0.56 should be removed (below new bid 0.57)");
        assert!(book.asks.get(&dec!(0.58)).is_some(), "Ask at 0.58 should remain");

        // Verify no crossed state: best_bid < best_ask
        if let (Some((bb, _)), Some((ba, _))) = (book.best_bid(), book.best_ask()) {
            assert!(bb < ba, "Book must not be crossed: bid {} should be < ask {}", bb, ba);
        }
    }

    // ─── Test 7: Sequence-based tick dedup ───

    #[tokio::test]
    async fn test_sequence_dedup_rejects_old_ticks() {
        let mut book = crate::engine::order_book::PlatformBook::new(Platform::Kalshi, Uuid::new_v4());

        let tick1 = make_tick(Platform::Kalshi, book.market_id, dec!(0.40), dec!(50), dec!(0.42), dec!(50), 175, 5);
        book.update_from_tick(&tick1);
        assert_eq!(book.sequence, 5);

        // Replay an older sequence — should be ignored
        let tick_old = make_tick(Platform::Kalshi, book.market_id, dec!(0.30), dec!(100), dec!(0.32), dec!(100), 175, 3);
        book.update_from_tick(&tick_old);
        assert_eq!(book.sequence, 5, "Sequence should not regress");
        // Bid should still be from tick1, not tick_old
        assert_eq!(book.best_bid().unwrap().0, dec!(0.40));
    }

    // ─── Test 8: Bankroll settlement and exposure tracking ───

    #[tokio::test]
    async fn test_bankroll_exposure_lifecycle() {
        let mut bm = BankrollManager::new(dec!(10000));

        let market_id = Uuid::new_v4();
        bm.add_exposure(Platform::Polymarket, dec!(500));
        bm.add_exposure(Platform::Kalshi, dec!(500));
        bm.add_market_exposure(market_id, dec!(100));

        assert_eq!(bm.total_exposure(), dec!(1000));
        assert_eq!(bm.market_exposure(&market_id), dec!(100));

        bm.remove_exposure(Platform::Polymarket, dec!(500));
        bm.remove_market_exposure(market_id, dec!(100));

        assert_eq!(bm.total_exposure(), dec!(500));
        assert_eq!(bm.market_exposure(&market_id), Decimal::ZERO);
    }

    // ─── Test 9: Database round-trip for trades ───

    #[tokio::test]
    async fn test_db_trade_roundtrip() {
        let db = test_db().await;

        let trade = TradeResult {
            trade_id: 1,
            opp_id: Uuid::new_v4(),
            market_id: Uuid::new_v4(),
            market_question: "Test market".into(),
            leg_a_platform: Platform::Polymarket,
            leg_a_side: Side::Yes,
            leg_a_price: dec!(0.40),
            leg_a_size: dec!(10),
            leg_a_fill_price: dec!(0.40),
            leg_a_fee: dec!(0.08),
            leg_b_platform: Platform::Kalshi,
            leg_b_side: Side::No,
            leg_b_price: dec!(0.50),
            leg_b_size: dec!(10),
            leg_b_fill_price: dec!(0.50),
            leg_b_fee: dec!(0.07),
            raw_spread: dec!(0.10),
            net_spread: dec!(0.08),
            profit: dec!(0.85),
            status: TradeStatus::Success,
            failure_reason: None,
            execution_ms: 42,
            executed_at: chrono::Utc::now(),
            bankroll_after: dec!(10000.85),
            bankroll_change_pct: dec!(0.0085),
            approved_size: dec!(10),
        };

        let row_id = db.insert_trade(&trade).await.expect("insert_trade failed");
        assert!(row_id > 0);

        let trades = db.get_trades_since(chrono::Utc::now() - chrono::Duration::hours(1)).await.expect("get_trades_since failed");
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].profit, dec!(0.85));
        assert_eq!(trades[0].status, TradeStatus::Success);
    }

    // ─── Test 10: Database position pair atomicity ───

    #[tokio::test]
    async fn test_db_position_pair_atomic() {
        let db = test_db().await;
        let market_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let pos_a = Position {
            id: 0, market_id, platform: Platform::Polymarket, side: Side::Yes,
            quantity: dec!(10), avg_entry_price: dec!(0.40), unrealized_pnl: Decimal::ZERO,
            opened_at: now, updated_at: now,
        };
        let pos_b = Position {
            id: 0, market_id, platform: Platform::Kalshi, side: Side::No,
            quantity: dec!(10), avg_entry_price: dec!(0.50), unrealized_pnl: Decimal::ZERO,
            opened_at: now, updated_at: now,
        };

        db.upsert_position_pair(&pos_a, &pos_b).await.expect("upsert_position_pair failed");

        let open = db.get_open_positions().await.expect("get_open_positions failed");
        assert_eq!(open.len(), 2, "Both legs should be persisted atomically");

        let arb_count = db.get_open_arb_count().await.expect("get_open_arb_count failed");
        assert_eq!(arb_count, 1, "Two legs on same market = 1 arb pair");
    }

    // ─── Test 11: Slippage estimation edge cases ───

    #[tokio::test]
    async fn test_slippage_zero_target() {
        use crate::feeds::normalizer::estimate_slippage;

        let depth = vec![PriceLevel { price: dec!(0.50), size: dec!(10) }];

        // Zero target should return None (prevents divide-by-zero)
        assert_eq!(estimate_slippage(Decimal::ZERO, &depth), None);
        assert_eq!(estimate_slippage(dec!(-1), &depth), None);

        // Empty depth should return None
        assert_eq!(estimate_slippage(dec!(5), &[]), None);
    }

    // ─── Test 12: Market expiry < 60s blocks detection ───

    #[tokio::test]
    async fn test_near_expiry_market_blocked() {
        let mut registry = MarketRegistry::new();
        let market_id = Uuid::new_v4();

        let mut platforms = HashMap::new();
        platforms.insert(Platform::Polymarket, PlatformMarketInfo {
            platform: Platform::Polymarket, platform_market_id: "p".into(),
            fee_rate_bps: 200, min_order_size: dec!(1), tick_size: dec!(0.01),
        });
        platforms.insert(Platform::Kalshi, PlatformMarketInfo {
            platform: Platform::Kalshi, platform_market_id: "k".into(),
            fee_rate_bps: 175, min_order_size: dec!(1), tick_size: dec!(0.01),
        });

        // Market expires in 30 seconds — should be blocked
        registry.register_market(Market {
            unified_id: market_id, question: "Expiring".into(), resolution_source: "t".into(),
            expiration: chrono::Utc::now() + chrono::Duration::seconds(30),
            platforms, category: MarketCategory::Other, confidence: 0.99,
            status: MarketStatus::Active, created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        });

        let mut uob = UnifiedOrderBook::new();
        let spread_engine = NetSpreadEngine::new(dec!(0.01));
        let mut detector = ArbitrageDetector::new(dec!(0.01), dec!(1.0), 5000, 3);

        uob.update(&make_tick(Platform::Polymarket, market_id, dec!(0.30), dec!(100), dec!(0.32), dec!(100), 200, 1));
        uob.update(&make_tick(Platform::Kalshi, market_id, dec!(0.60), dec!(100), dec!(0.62), dec!(100), 175, 1));

        let opps = detector.detect_for_market(&market_id, &registry, &uob, &spread_engine, dec!(10));
        assert!(opps.is_empty(), "Markets expiring in <60s should be blocked");
    }

    // ─── Test 13: Backup path validation ───

    #[tokio::test]
    async fn test_backup_path_validation() {
        let db = test_db().await;

        // These should all be rejected
        let bad_paths = vec![
            "../../../etc/passwd",
            "data/test'.db",
            "/tmp/evil.db",
            "data/test;drop table trades.db",
            "data/test.txt", // wrong extension
        ];

        for path in bad_paths {
            let result = db.backup_to_file(path).await;
            assert!(result.is_err(), "Path '{}' should be rejected", path);
        }
    }

    // ─── Test 14: Concurrent CB7 consecutive failure detection ───

    #[tokio::test]
    async fn test_consecutive_failure_circuit_breaker() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05), dec!(0.10), dec!(0.20), dec!(0.40), 100, 5, 20,
        );

        // Record 5 consecutive failures
        for _ in 0..5 {
            cb.record_execution(false);
        }

        let trips = cb.check_all(
            dec!(100), dec!(10000), dec!(0), dec!(0), dec!(0), 0, false, 100, dec!(0),
        );

        let has_cb7 = trips.iter().any(|t| t.breaker_type.contains("CB7"));
        assert!(has_cb7, "CB7 should trip after 5 consecutive failures");
        assert!(cb.is_trading_halted(), "Trading should be halted");
    }
}
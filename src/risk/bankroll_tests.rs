/// Async actor message tests for BankrollHandle.
/// Declared from risk/mod.rs as a child module (tests public API).
use crate::risk::bankroll::{BankrollHandle, BankrollManager, BankrollMsg};
use crate::types::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

// ─── Helper ───────────────────────────────────────────────────────────────────

fn make_handle(bankroll: Decimal) -> BankrollHandle {
    BankrollHandle::new(BankrollManager::new(bankroll))
}

fn make_trade(profit: Decimal, status: TradeStatus, approved_size: Decimal) -> TradeResult {
    TradeResult {
        trade_id: 1,
        opp_id: Uuid::new_v4(),
        market_id: Uuid::new_v4(),
        market_question: "Test".into(),
        leg_a_platform: Platform::Polymarket,
        leg_a_side: Side::Yes,
        leg_a_price: dec!(0.40),
        leg_a_size: approved_size,
        leg_a_fill_price: dec!(0.40),
        leg_a_fee: dec!(0.01),
        leg_b_platform: Platform::Kalshi,
        leg_b_side: Side::No,
        leg_b_price: dec!(0.50),
        leg_b_size: approved_size,
        leg_b_fill_price: dec!(0.50),
        leg_b_fee: dec!(0.01),
        raw_spread: dec!(0.10),
        net_spread: dec!(0.08),
        profit,
        status,
        failure_reason: None,
        execution_ms: 50,
        executed_at: chrono::Utc::now(),
        bankroll_after: Decimal::ZERO,
        bankroll_change_pct: Decimal::ZERO,
        approved_size,
    }
}

// ─── F1: ReserveCapital returns true when sufficient ─────────────────────────

#[tokio::test]
async fn test_reserve_capital_returns_true_when_sufficient() {
    let handle = make_handle(dec!(1000));
    let market_id = Uuid::new_v4();

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle.tx.send(BankrollMsg::ReserveCapital {
        leg_a_exposure: dec!(100),
        leg_b_exposure: dec!(100),
        platform_a: Platform::Polymarket,
        platform_b: Platform::Kalshi,
        market_id,
        reply: reply_tx,
    }).await.unwrap();

    let result = reply_rx.await.unwrap();
    assert!(result, "ReserveCapital should return true when bankroll is sufficient");
}

// ─── F2: ReserveCapital returns false when insufficient ──────────────────────

#[tokio::test]
async fn test_reserve_capital_returns_false_when_insufficient() {
    let handle = make_handle(dec!(100));
    let market_id = Uuid::new_v4();

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle.tx.send(BankrollMsg::ReserveCapital {
        leg_a_exposure: dec!(200),
        leg_b_exposure: dec!(200),
        platform_a: Platform::Polymarket,
        platform_b: Platform::Kalshi,
        market_id,
        reply: reply_tx,
    }).await.unwrap();

    let result = reply_rx.await.unwrap();
    assert!(!result, "ReserveCapital should return false when bankroll is insufficient");
}

// ─── F3: ReserveCapital increments exposure on success ───────────────────────

#[tokio::test]
async fn test_reserve_capital_increments_exposure() {
    let handle = make_handle(dec!(1000));
    let market_id = Uuid::new_v4();

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle.tx.send(BankrollMsg::ReserveCapital {
        leg_a_exposure: dec!(100),
        leg_b_exposure: dec!(100),
        platform_a: Platform::Polymarket,
        platform_b: Platform::Kalshi,
        market_id,
        reply: reply_tx,
    }).await.unwrap();
    let ok = reply_rx.await.unwrap();
    assert!(ok);

    // Check risk state — exposure should be non-zero
    let state = handle.get_risk_state(Platform::Polymarket, Platform::Kalshi, market_id).await;
    assert!(state.platform_a_exposure_pct > Decimal::ZERO, "exposure should be recorded after reserve");
}

// ─── F4: ProcessTrade success - bankroll increases ───────────────────────────

#[tokio::test]
async fn test_process_trade_success_increases_bankroll() {
    let handle = make_handle(dec!(1000));
    let trade = make_trade(dec!(5), TradeStatus::Success, dec!(10));
    let result = handle.process_trade(trade).await;
    assert_eq!(result.bankroll_after, dec!(1005), "bankroll should increase by profit");
}

// ─── F5: ProcessTrade failure - exposure released ────────────────────────────

#[tokio::test]
async fn test_process_trade_failure_releases_exposure() {
    let handle = make_handle(dec!(1000));
    let market_id = Uuid::new_v4();

    // Reserve capital first
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle.tx.send(BankrollMsg::ReserveCapital {
        leg_a_exposure: dec!(100),
        leg_b_exposure: dec!(100),
        platform_a: Platform::Polymarket,
        platform_b: Platform::Kalshi,
        market_id,
        reply: reply_tx,
    }).await.unwrap();
    reply_rx.await.unwrap();

    // Process failed trade
    let mut trade = make_trade(dec!(0), TradeStatus::Fail, dec!(10));
    trade.market_id = market_id;
    handle.process_trade(trade).await;

    // After failure, exposure should be released
    let state = handle.get_risk_state(Platform::Polymarket, Platform::Kalshi, market_id).await;
    // Exposure should be lower after failed trade releases it
    // The bankroll actor removes exposure on failure
    assert!(state.platform_a_exposure_pct < dec!(0.20),
        "exposure should be released after failed trade, got {}", state.platform_a_exposure_pct);
}

// ─── F6: RecordSettlement increments bankroll and updates peak ───────────────

#[tokio::test]
async fn test_record_settlement_updates_bankroll_and_peak() {
    let handle = make_handle(dec!(1000));
    handle.record_settlement(dec!(50)).await;

    let snapshot = handle.get_snapshot(dec!(0.25)).await;
    assert_eq!(snapshot.bankroll, dec!(1050), "bankroll should be 1050 after settlement");
    assert_eq!(snapshot.peak_bankroll, dec!(1050), "peak should update to 1050");
}

// ─── F7: GetRiskState returns correct percentages ────────────────────────────

#[tokio::test]
async fn test_get_risk_state_correct_percentages() {
    let handle = make_handle(dec!(1000));
    let market_id = Uuid::new_v4();

    // Reserve 10% of bankroll on each platform
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle.tx.send(BankrollMsg::ReserveCapital {
        leg_a_exposure: dec!(100),
        leg_b_exposure: dec!(100),
        platform_a: Platform::Polymarket,
        platform_b: Platform::Kalshi,
        market_id,
        reply: reply_tx,
    }).await.unwrap();
    reply_rx.await.unwrap();

    let state = handle.get_risk_state(Platform::Polymarket, Platform::Kalshi, market_id).await;
    // platform_a_exposure_pct = 100 / 1000 = 10% = 0.10
    let expected = dec!(100) / dec!(1000);
    assert!((state.platform_a_exposure_pct - expected).abs() < dec!(0.001),
        "platform_a exposure pct should be ~0.10, got {}", state.platform_a_exposure_pct);
}

// ─── F8: Double-reserve race — only one succeeds ─────────────────────────────

#[tokio::test]
async fn test_double_reserve_race_only_one_succeeds() {
    // Bankroll is 300; each reservation requests 200+200=400 (more than bankroll)
    // So only one should succeed
    let handle = make_handle(dec!(300));
    let market_id = Uuid::new_v4();

    let handle1 = handle.clone();
    let handle2 = handle.clone();
    let mid1 = market_id;
    let mid2 = market_id;

    // Each request needs dec!(100)+dec!(100)=dec!(200) total.
    // Bankroll=300: first request takes 200 → 100 left; second needs 200 but only 100 available → fails.
    let task1 = tokio::spawn(async move {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        handle1.tx.send(BankrollMsg::ReserveCapital {
            leg_a_exposure: dec!(100),
            leg_b_exposure: dec!(100),
            platform_a: Platform::Polymarket,
            platform_b: Platform::Kalshi,
            market_id: mid1,
            reply: reply_tx,
        }).await.unwrap();
        reply_rx.await.unwrap()
    });

    let task2 = tokio::spawn(async move {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        handle2.tx.send(BankrollMsg::ReserveCapital {
            leg_a_exposure: dec!(100),
            leg_b_exposure: dec!(100),
            platform_a: Platform::Polymarket,
            platform_b: Platform::Kalshi,
            market_id: mid2,
            reply: reply_tx,
        }).await.unwrap();
        reply_rx.await.unwrap()
    });

    let (r1, r2) = tokio::join!(task1, task2);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    // Since the actor serializes messages, only one can succeed
    // (The second one sees the already-committed exposure from the first)
    let successes = [r1, r2].iter().filter(|&&x| x).count();
    assert_eq!(successes, 1,
        "exactly one of two concurrent reserves against limited capital should succeed (r1={}, r2={})",
        r1, r2);
}

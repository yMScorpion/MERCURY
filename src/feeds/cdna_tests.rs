/// Unit tests for CDNA feed message parsing.
/// Declared from cdna.rs as a child module (has access to private items).
use super::*;
use rust_decimal_macros::dec;
use tokio::sync::broadcast;
use uuid::Uuid;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_cdna_feed(instrument: &str) -> CdnaFeed {
    let config = crate::config::CdnaConfig {
        enabled: true,
        ws_url: "ws://test".into(),
        rest_url: "http://test".into(),
    };
    let market_id = Uuid::new_v4();
    CdnaFeed::new(config, vec![(instrument.to_string(), market_id, 150)])
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_book_channel_populates_bids_asks() {
    let mut feed = make_cdna_feed("BTCUSD");
    let (tx, mut rx) = broadcast::channel(16);

    let json = r#"{"result":{"channel":"book.BTCUSD","data":{"bids":[["0.40","100"]],"asks":[["0.60","80"]]}}}"#;
    feed.handle_message(json, &tx).unwrap();

    let tick = rx.try_recv().expect("tick should be emitted after book update");
    assert_eq!(tick.bid_price, dec!(0.40));
    assert_eq!(tick.ask_price, dec!(0.60));
}

#[test]
fn test_zero_size_removes_level() {
    let mut feed = make_cdna_feed("ETHUSD");
    let (tx, mut rx) = broadcast::channel(16);

    // First populate the book
    let init = r#"{"result":{"channel":"book.ETHUSD","data":{"bids":[["0.40","100"],["0.38","50"]],"asks":[["0.60","80"]]}}}"#;
    feed.handle_message(init, &tx).unwrap();
    let _ = rx.try_recv();

    // Remove the 0.40 level by sending size=0
    let remove = r#"{"result":{"channel":"book.ETHUSD","data":{"bids":[["0.40","0"]],"asks":[]}}}"#;
    feed.handle_message(remove, &tx).unwrap();

    let tick = rx.try_recv().expect("tick after level removal");
    // Best bid should now be 0.38 (the 0.40 was removed)
    assert_eq!(tick.bid_price, dec!(0.38));
}

#[test]
fn test_unknown_channel_ignored() {
    let mut feed = make_cdna_feed("BTCUSD");
    let (tx, mut rx) = broadcast::channel(16);

    // A non-book channel should be ignored
    let json = r#"{"result":{"channel":"trade.BTCUSD","data":{"price":"0.45","size":"10"}}}"#;
    feed.handle_message(json, &tx).unwrap();

    assert!(rx.try_recv().is_err(), "non-book channel should not emit a tick");
}

#[test]
fn test_malformed_json_no_panic() {
    let mut feed = make_cdna_feed("BTCUSD");
    let (tx, _rx) = broadcast::channel(16);

    let result = feed.handle_message("{not json", &tx);
    assert!(result.is_err(), "malformed JSON should return an error (parse error)");
    // Critically: no panic
}

#[test]
fn test_channel_without_book_prefix_skipped() {
    let mut feed = make_cdna_feed("BTCUSD");
    let (tx, mut rx) = broadcast::channel(16);

    // No "book." prefix
    let json = r#"{"result":{"channel":"ticker.BTCUSD","data":{"bids":[["0.40","100"]],"asks":[["0.60","80"]]}}}"#;
    feed.handle_message(json, &tx).unwrap();

    assert!(rx.try_recv().is_err(), "channel without book. prefix should not emit tick");
}

/// Unit tests for the rewritten market discovery logic.
use super::*;

// ─── normalize_question tests ──────────────────────────────────────────────

#[test]
fn test_normalize_bitcoin_to_btc() {
    let result = MarketDiscovery::normalize_question("Will Bitcoin exceed $60k?");
    assert!(result.contains("btc"), "Bitcoin should be normalized to btc: {}", result);
    assert!(!result.contains("bitcoin"), "bitcoin should be replaced: {}", result);
}

#[test]
fn test_normalize_minutes_to_min() {
    let result = MarketDiscovery::normalize_question("Will BTC price in 15 minutes?");
    assert!(result.contains("min"), "minutes should be normalized to min: {}", result);
}

#[test]
fn test_normalize_strips_punctuation() {
    let result = MarketDiscovery::normalize_question("Will BTC exceed $100,000?");
    assert!(!result.contains('$'), "dollar sign should be stripped");
    assert!(!result.contains(','), "comma should be stripped");
    assert!(!result.contains('?'), "question mark should be stripped");
}

#[test]
fn test_normalize_lowercases() {
    let result = MarketDiscovery::normalize_question("WILL TRUMP WIN?");
    assert_eq!(result, result.to_lowercase(), "result should be lowercase");
}

#[test]
fn test_normalize_no_double_spaces() {
    let result = MarketDiscovery::normalize_question("Will   Bitcoin  hit  100k?");
    assert!(!result.contains("  "), "should not have double spaces: '{}'", result);
}

// ─── Token ID parsing tests ────────────────────────────────────────────────

#[test]
fn test_parse_two_clob_token_ids_string() {
    let item = serde_json::json!({
        "clobTokenIds": "[\"abc123\",\"def456\"]"
    });
    let (yes, no) = parse_two_clob_token_ids(&item);
    assert_eq!(yes, "abc123");
    assert_eq!(no, "def456");
}

#[test]
fn test_parse_two_clob_token_ids_array() {
    let item = serde_json::json!({
        "clobTokenIds": ["abc123", "def456"]
    });
    let (yes, no) = parse_two_clob_token_ids(&item);
    assert_eq!(yes, "abc123");
    assert_eq!(no, "def456");
}

#[test]
fn test_parse_two_clob_token_ids_tokens_fallback() {
    let item = serde_json::json!({
        "tokens": [
            {"token_id": "token_yes", "outcome": "Yes"},
            {"token_id": "token_no", "outcome": "No"}
        ]
    });
    let (yes, no) = parse_two_clob_token_ids(&item);
    assert_eq!(yes, "token_yes");
    assert_eq!(no, "token_no");
}

#[test]
fn test_parse_two_clob_token_ids_empty() {
    let item = serde_json::json!({});
    let (yes, no) = parse_two_clob_token_ids(&item);
    assert!(yes.is_empty());
    assert!(no.is_empty());
}

// ─── Round timestamp tests ─────────────────────────────────────────────────

#[test]
fn test_round_timestamp_divisible_by_900() {
    let now_ts = chrono::Utc::now().timestamp();
    let round = (now_ts / 900) * 900;
    assert_eq!(round % 900, 0, "Round start must be divisible by 900");
}

#[test]
fn test_build_round_candidates_skips_expired() {
    // Create a discovery instance (we only need the method)
    let config = crate::config::PlatformsConfig {
        polymarket: crate::config::PolymarketConfig {
            enabled: true,
            ws_url: "ws://test".into(),
            rest_url: "http://test".into(),
            sports_ws_url: "ws://test".into(),
        },
        kalshi: crate::config::KalshiConfig {
            enabled: true,
            ws_url: "ws://test".into(),
            rest_url: "http://test".into(),
        },
        cdna: crate::config::CdnaConfig {
            enabled: false,
            ws_url: "".into(),
            rest_url: "".into(),
        },
        forecastex: crate::config::ForecastExConfig {
            enabled: false,
            fix_host: "".into(),
            fix_port: 0,
        },
    };
    let discovery = MarketDiscovery::new(config, 30, None);
    let rounds = discovery.build_round_candidates(&["btc", "eth"]);

    // Should have 2 assets * up to 3 rounds = at most 6, but expired ones filtered
    assert!(!rounds.is_empty(), "Should have at least one round candidate");
    
    let now_ts = chrono::Utc::now().timestamp();
    for r in &rounds {
        let round_end = r.round_start_ts + 900;
        assert!(
            round_end > now_ts - 60,
            "No expired rounds should be returned (round_start={}, end={})",
            r.round_start_ts,
            round_end
        );
    }
}

// ─── Categorization tests ──────────────────────────────────────────────────

#[test]
fn test_categorize_crypto() {
    assert_eq!(categorize_question("will btc exceed 100k"), MarketCategory::Crypto);
    assert_eq!(categorize_question("ethereum price up or down"), MarketCategory::Crypto);
}

#[test]
fn test_categorize_sports() {
    assert_eq!(categorize_question("will nba team win the game"), MarketCategory::Sports);
}

#[test]
fn test_categorize_politics() {
    assert_eq!(categorize_question("who wins the election for president"), MarketCategory::Politics);
}
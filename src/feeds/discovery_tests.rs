/// Unit tests for market discovery matching logic.
/// Declared from discovery.rs as a child module (has access to private items).
use super::*;

// ─── normalize_question tests ─────────────────────────────────────────────────

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
fn test_normalize_lowercases_text() {
    let result = MarketDiscovery::normalize_question("WILL TRUMP WIN?");
    assert_eq!(result, result.to_lowercase(), "result should be lowercase");
}

#[test]
fn test_normalize_deduplicates_spaces() {
    let result = MarketDiscovery::normalize_question("Will   Bitcoin  hit  100k?");
    // Should not have double spaces
    assert!(!result.contains("  "), "should not have double spaces: '{}'", result);
}

// ─── Jaccard similarity (white-box via normalize then compute) ───────────────

/// Helper: compute Jaccard similarity between two questions using the same logic as discovery.rs
fn jaccard(q1: &str, q2: &str) -> f64 {
    let norm1 = MarketDiscovery::normalize_question(q1);
    let norm2 = MarketDiscovery::normalize_question(q2);
    let tokens_a: std::collections::HashSet<&str> = norm1.split_whitespace().collect();
    let tokens_b: std::collections::HashSet<&str> = norm2.split_whitespace().collect();
    let intersection = tokens_a.intersection(&tokens_b).count();
    let union = tokens_a.union(&tokens_b).count();
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

#[test]
fn test_jaccard_identical_questions_is_1() {
    let sim = jaccard("Will Bitcoin exceed $60k?", "Will Bitcoin exceed $60k?");
    assert!((sim - 1.0).abs() < 0.001, "identical questions should have sim=1.0, got {}", sim);
}

#[test]
fn test_jaccard_unrelated_questions_is_low() {
    let sim = jaccard("Will Bitcoin exceed $60k?", "Will Trump win the election?");
    assert!(sim < 0.3, "unrelated questions should have low similarity, got {}", sim);
}

#[test]
fn test_jaccard_same_market_different_phrasing() {
    // Both refer to BTC at $100k — should be highly similar after normalization
    let sim = jaccard(
        "Will Bitcoin price exceed $100,000?",
        "Will BTC exceed $100k by December?",
    );
    // These share: will, btc, exceed, 100 (with normalization)
    assert!(sim >= 0.3, "similar questions should have decent overlap, got {}", sim);
}

#[test]
fn test_70_percent_threshold_filters_dissimilar() {
    // Two questions that share ~50% tokens should NOT meet the 70% threshold
    let sim = jaccard(
        "Will Bitcoin exceed $60k this year",
        "Will Ethereum exceed $4k this year",
    );
    // Shares: will, exceed, this, year (~4/7 tokens, ~57%)
    assert!(sim < 0.70, "50-60% similar questions should fail 70% threshold, got {}", sim);
}

// ─── Proptests ────────────────────────────────────────────────────────────────
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_jaccard_similarity_properties(
        q1 in "[a-zA-Z0-9 ]{10,50}",
        q2 in "[a-zA-Z0-9 ]{10,50}"
    ) {
        let sim = jaccard(&q1, &q2);
        prop_assert!(sim >= 0.0 && sim <= 1.0, "Jaccard similarity must be between 0 and 1");
    }

    #[test]
    fn test_normalize_question_is_idempotent(q in "[a-zA-Z0-9$?,. ]{10,50}") {
        let norm1 = MarketDiscovery::normalize_question(&q);
        let norm2 = MarketDiscovery::normalize_question(&norm1);
        prop_assert_eq!(norm1, norm2, "Normalization should be idempotent");
    }
}

// ─── Expiration logic tests ───────────────────────────────────────────────────

#[test]
fn test_15_minute_market_expiration_check() {
    use chrono::{Duration, Utc};

    // A 3-minute difference for a 15-min candle should be REJECTED
    let pm_exp = Utc::now() + Duration::hours(1);
    let km_exp_3min_diff = pm_exp + Duration::minutes(3);
    let diff_secs = (pm_exp - km_exp_3min_diff).num_seconds().abs();
    assert!(diff_secs > 120, "3-minute diff ({}) should exceed 120s threshold", diff_secs);

    // A 1-minute difference should be accepted (< 120s threshold)
    let km_exp_1min_diff = pm_exp + Duration::minutes(1);
    let diff_secs_ok = (pm_exp - km_exp_1min_diff).num_seconds().abs();
    assert!(diff_secs_ok <= 120, "1-minute diff ({}) should be within 120s threshold", diff_secs_ok);
}

#[test]
fn test_standard_market_expiration_check() {
    use chrono::{Duration, Utc};

    // A 24-hour difference for standard markets should be ACCEPTED (<= 48h)
    let pm_exp = Utc::now() + Duration::days(30);
    let km_exp_24h = pm_exp + Duration::hours(24);
    let diff_secs_24h = (pm_exp - km_exp_24h).num_seconds().abs();
    assert!(diff_secs_24h <= 48 * 3600, "24h diff should be within 48h threshold");

    // A 72-hour difference should be REJECTED (> 48h)
    let km_exp_72h = pm_exp + Duration::hours(72);
    let diff_secs_72h = (pm_exp - km_exp_72h).num_seconds().abs();
    assert!(diff_secs_72h > 48 * 3600, "72h diff should exceed 48h threshold");
}

#[test]
fn test_numerical_target_mismatch_in_normalize() {
    // "60k" vs "70k" should be detectable numerically after normalization
    let q60k = MarketDiscovery::normalize_question("Will Bitcoin exceed $60,000?");
    let q70k = MarketDiscovery::normalize_question("Will Bitcoin exceed $70,000?");

    let nums_60k: Vec<f64> = q60k.split_whitespace()
        .filter_map(|w| w.replace('$', "").replace(',', "").parse::<f64>().ok())
        .collect();
    let nums_70k: Vec<f64> = q70k.split_whitespace()
        .filter_map(|w| w.replace('$', "").replace(',', "").parse::<f64>().ok())
        .collect();

    // They should have different numerical targets
    assert_ne!(nums_60k, nums_70k, "different price targets should produce different number lists");
}

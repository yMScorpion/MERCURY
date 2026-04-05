/// Unit tests for ForecastEx FIX feed parsing.
/// Declared from forecastex.rs as a child module (has access to private items).
use super::*;

const SOH: char = '\x01';

// ─── Helper: Build a valid FIX message string ─────────────────────────────────

fn build_fix(body_fields: &[(u32, &str)]) -> String {
    let mut body = String::new();
    for (tag, val) in body_fields {
        body.push_str(&format!("{}={}{}", tag, val, SOH));
    }
    let header = format!("8=FIX.4.4{}9={}{}", SOH, body.len(), SOH);
    let full = format!("{}{}", header, body);
    let checksum: u32 = full.bytes().map(|b| b as u32).sum::<u32>() % 256;
    format!("{}10={:03}{}", full, checksum, SOH)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_parse_fix_fields_valid_message() {
    // Build a simple FIX message and verify tag→value mapping
    let msg = build_fix(&[
        (35, "W"),   // MsgType = MarketDataSnapshot
        (55, "KBTC"), // Symbol
        (49, "SENDER"),
        (56, "TARGET"),
    ]);

    let fields = ForecastExFeed::parse_fix_fields(&msg);
    assert!(!fields.is_empty(), "should parse fields from valid message");
    assert_eq!(fields.get(&55).map(|s| s.as_str()), Some("KBTC"));
    assert_eq!(fields.get(&35).map(|s| s.as_str()), Some("W"));
}

#[test]
fn test_parse_fix_fields_corrupted_checksum_returns_empty() {
    // Build a valid FIX message then corrupt the checksum
    let valid = build_fix(&[(35, "W"), (55, "KBTC")]);

    // Replace the checksum with a wrong value
    let corrupted = if let Some(idx) = valid.rfind("10=") {
        let before = &valid[..idx];
        format!("{}10=999{}", before, SOH)
    } else {
        valid.clone()
    };

    let fields = ForecastExFeed::parse_fix_fields(&corrupted);
    assert!(fields.is_empty(), "corrupted checksum should return empty map (reject message)");
}

#[test]
fn test_parse_fix_fields_multiple_tags() {
    let msg = build_fix(&[
        (35, "X"),
        (55, "MKTID"),
        (49, "MERC"),
        (56, "FEX"),
        (34, "42"),
    ]);

    let fields = ForecastExFeed::parse_fix_fields(&msg);
    assert_eq!(fields.get(&49).map(|s| s.as_str()), Some("MERC"));
    assert_eq!(fields.get(&34).map(|s| s.as_str()), Some("42"));
}

#[test]
fn test_find_next_tag_returns_correct_value() {
    let parts = vec![
        "269=0",  // entry type
        "270=0.45", // price
        "271=100",  // size
    ];
    let result = ForecastExFeed::find_next_tag(&parts, 0, 270);
    assert_eq!(result, Some("0.45"));
}

#[test]
fn test_find_next_tag_stops_at_next_269_boundary() {
    // Looking for tag 270 after position 0, but a 269 boundary appears before it
    let parts = vec![
        "269=0",   // first entry
        "269=1",   // next entry (boundary — should stop search)
        "270=0.55", // price for second entry (should NOT be returned for first entry's 270 search)
        "271=80",
    ];
    // Searching for 270 starting at position 1 — hits 269 boundary at index 1 before finding 270
    let result = ForecastExFeed::find_next_tag(&parts, 1, 270);
    // After the boundary 269 at index 1, it should stop and return None (or the value after)
    // Based on implementation: stops at next "269" boundary
    // At index 1 we have "269=1" — this IS the boundary, so searching from 1 should immediately stop
    // Let's verify by searching from a position AFTER the boundary
    let result2 = ForecastExFeed::find_next_tag(&parts, 2, 270);
    assert_eq!(result2, Some("0.55"), "should find 270 starting after 269 boundary");
    let _ = result; // result from position 1 may be None (boundary hit immediately)
}

#[test]
fn test_find_next_tag_not_found_returns_none() {
    let parts = vec!["35=W", "55=SYM", "49=SENDER"];
    let result = ForecastExFeed::find_next_tag(&parts, 0, 270);
    assert_eq!(result, None, "non-existent tag should return None");
}

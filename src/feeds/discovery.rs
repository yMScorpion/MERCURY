//! Market discovery: polls platform REST APIs, matches equivalent markets
//! cross-platform, and registers them in the MarketRegistry.

use anyhow::Result;
use chrono::{DateTime, Utc, Timelike};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use regex::Regex;

// Compiled once — avoids re-compiling the regex on every call
fn time_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(\d{1,2}):(\d{2})\s*(am|pm|a\.m\.|p\.m\.)?\s*(et|est|edt|brt|utc)?").unwrap())
}

use crate::config::PlatformsConfig;
use crate::feeds::normalizer::compute_unified_market_id;
use crate::types::*;

/// Discovered market from a single platform before cross-matching.
#[derive(Debug, Clone)]
struct DiscoveredMarket {
    platform: Platform,
    platform_market_id: String,
    question: String,
    /// Normalized question for fuzzy matching (lowercase, trimmed, stripped punctuation).
    question_normalized: String,
    resolution_source: String,
    expiration: DateTime<Utc>,
    category: MarketCategory,
    fee_rate_bps: u16,
    min_order_size: Decimal,
    tick_size: Decimal,
}

/// Result of the discovery cycle: a fully matched cross-platform market.
#[derive(Debug, Clone)]
pub struct MatchedMarket {
    pub market: Market,
}

pub struct MarketDiscovery {
    platforms_config: PlatformsConfig,
    http: reqwest::Client,
    poll_interval: Duration,
    kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>,
}

impl MarketDiscovery {
    // Helper to safely extract market arrays from Polymarket's inconsistent pagination JSON
    fn extract_markets_array(value: &serde_json::Value) -> &[serde_json::Value] {
        if let Some(arr) = value.as_array() {
            arr.as_slice()
        } else if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
            arr.as_slice()
        } else {
            &[]
        }
    }

    pub fn new(platforms_config: PlatformsConfig, poll_interval_secs: u64, kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"));
        
        let http = reqwest::Client::builder()
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .default_headers(headers)
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
            .build()
            .expect("failed to build discovery HTTP client");
        Self {
            platforms_config,
            http,
            poll_interval: Duration::from_secs(poll_interval_secs),
            kalshi_auth,
        }
    }

    /// Run the discovery loop, sending matched markets to the registry channel.
    pub async fn run(self, matched_tx: mpsc::Sender<MatchedMarket>) {
        info!(
            interval_secs = self.poll_interval.as_secs(),
            "Market discovery started"
        );

        // First fetch is immediate.
        let mut ticker = tokio::time::interval_at(
            tokio::time::Instant::now(),
            self.poll_interval,
        );

        loop {
            ticker.tick().await;

            match self.discover_and_match().await {
                Ok(matched) => {
                    let active = matched.iter().filter(|m| m.market.status == MarketStatus::Active).count();
                    info!(
                        total = matched.len(),
                        active,
                        suspended = matched.len() - active,
                        "Discovery cycle complete — sending to registry"
                    );
                    for m in matched {
                        if let Err(e) = matched_tx.send(m).await {
                            warn!(error = %e, "Matched market channel closed — skipping");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Market discovery cycle failed");
                }
            }
        }
    }

    /// Helper to extract time from title like "8:15AM" or "10 PM" and adjust the date.
    fn try_fix_expiration_from_title(title: &str, base_date: DateTime<Utc>) -> DateTime<Utc> {
        let lower = title.to_lowercase();
        // Regex compiled once via time_regex() — not re-compiled per call
        if let Some(caps) = time_regex().captures_iter(&lower).last() {
            let mut hour: u32 = caps.get(1).map_or(0, |m| m.as_str().parse().unwrap_or(0));
            let min: u32 = caps.get(2).map_or(0, |m| m.as_str().parse().unwrap_or(0));
            let ampm = caps.get(3).map(|m| m.as_str().replace(".", ""));
            let tz = caps.get(4).map(|m| m.as_str());

            if let Some(ampm_str) = ampm {
                if ampm_str == "pm" && hour < 12 {
                    hour += 12;
                } else if ampm_str == "am" && hour == 12 {
                    hour = 0;
                }
            }

            if hour < 24 && min < 60 {
                if let Some(mut new_date) = base_date.with_hour(hour).and_then(|d| d.with_minute(min)) {
                    if let Some(tz_str) = tz {
                        if tz_str == "et" || tz_str == "est" || tz_str == "edt" {
                            new_date += chrono::Duration::hours(4);
                        } else if tz_str == "brt" {
                            new_date += chrono::Duration::hours(3);
                        }
                    } else if lower.contains("et") || lower.contains("est") || lower.contains("edt") {
                        new_date += chrono::Duration::hours(4);
                    } else if lower.contains("brt") {
                        new_date += chrono::Duration::hours(3);
                    }

                    // Fix day wraparound if we applied an offset that pushed it too far or pulled it back
                    let diff = (new_date - base_date).num_hours();
                    if diff > 12 {
                        new_date -= chrono::Duration::days(1);
                    } else if diff < -12 {
                        new_date += chrono::Duration::days(1);
                    }

                    return new_date;
                }
            }
        }
        base_date
    }

    async fn discover_and_match(&self) -> Result<Vec<MatchedMarket>> {
        let mut poly_markets: Vec<DiscoveredMarket> = Vec::new();
        let mut kalshi_markets: Vec<DiscoveredMarket> = Vec::new();

        if self.platforms_config.polymarket.enabled {
            match self.fetch_polymarket_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Polymarket markets fetched");
                    poly_markets = markets;
                }
                Err(e) => warn!(error = %e, "Failed to fetch Polymarket markets"),
            }
        }

        if self.platforms_config.kalshi.enabled {
            match self.fetch_kalshi_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Kalshi markets fetched");
                    kalshi_markets = markets;
                }
                Err(e) => warn!(error = %e, "Failed to fetch Kalshi markets"),
            }
        }

        info!(
            poly_count = poly_markets.len(),
            kalshi_count = kalshi_markets.len(),
            "Starting cross-platform market matching (spawn_blocking)"
        );

        // Offload the CPU-intensive O(N×K) matching to a blocking thread pool so we
        // don't starve the Tokio async runtime for 30-90 seconds during each cycle.
        let matched = tokio::task::spawn_blocking(move || {
            Self::match_markets_sync(poly_markets, kalshi_markets)
        }).await??;

        Ok(matched)
    }

    /// Pure synchronous matching — safe to run on spawn_blocking thread.
    ///
    /// Strategy:
    ///  1. Build an inverted word index over Kalshi markets so we can find
    ///     candidates that share ≥1 word with a Poly market in O(|words|) time.
    ///  2. For each candidate pair, apply the expiration window and Jaccard check.
    ///
    /// This reduces comparisons from O(N×K) to O(N × avg_shared_word_candidates),
    /// which is typically 1–3 orders of magnitude fewer pairs.
    fn match_markets_sync(
        poly_markets: Vec<DiscoveredMarket>,
        kalshi_markets: Vec<DiscoveredMarket>,
    ) -> Result<Vec<MatchedMarket>> {
        // ── Stop-words that carry no semantic meaning ──────────────────────────
        // These are stripped from the inverted index so common words ("will",
        // "the", "by", "at") don't create massive candidate sets.
        // We preserve directional and numeric descriptors like "up", "above", "below".
        let stop_words: std::collections::HashSet<&str> = [
            "will", "the", "a", "an", "in", "on", "at", "by", "for", "to",
            "of", "be", "is", "are", "was", "were", "has", "have", "had",
            "do", "does", "did", "not", "or", "and", "if", "it", "its",
            "this", "that", "with", "from", "as", "so", "can",
            "may", "per", "vs", "end", "close", "open", "day", "week",
            "month", "year", "next", "last", "new", "more", "less", "most",
            "least", "than", "which", "who", "what", "when", "how", "between",
        ].iter().cloned().collect();

        // ── Build inverted index: word → sorted Vec<kalshi_idx> ───────────────
        let mut word_to_kalshi: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, km) in kalshi_markets.iter().enumerate() {
            for word in km.question_normalized.split_whitespace() {
                if !stop_words.contains(word) {
                    word_to_kalshi.entry(word.to_string()).or_default().push(i);
                }
            }
        }

        // ── Pre-compute token sets for Kalshi (for Jaccard computation) ───────
        let kalshi_tokens: Vec<std::collections::HashSet<String>> = kalshi_markets
            .iter()
            .map(|m| {
                m.question_normalized
                    .split_whitespace()
                    .filter(|w| !stop_words.contains(*w))
                    .map(|w| w.to_string())
                    .collect()
            })
            .collect();

        // ── Group Kalshi by expiration hour for expiration pre-filter ─────────
        let mut kalshi_by_hour: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, km) in kalshi_markets.iter().enumerate() {
            let hour = km.expiration.timestamp() / 3600;
            kalshi_by_hour.entry(hour).or_default().push(i);
        }

        // ── Diagnostic: show all Kalshi markets by series ─────────────────────
        {
            let mut by_series: HashMap<String, Vec<&DiscoveredMarket>> = HashMap::new();
            for km in kalshi_markets.iter() {
                let series = km.platform_market_id.split('-').next().unwrap_or("UNK").to_string();
                by_series.entry(series).or_default().push(km);
            }
            let mut series_list: Vec<(String, &Vec<&DiscoveredMarket>)> = by_series.iter()
                .map(|(k, v)| (k.clone(), v))
                .collect();
            series_list.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            for (series, markets) in &series_list {
                // Show first 2 sample questions for each series so we understand their format
                let samples: Vec<String> = markets.iter().take(2)
                    .map(|m| format!("{} [exp={}]", m.question_normalized, m.expiration.format("%m-%d %H:%M")))
                    .collect();
                tracing::info!(series = %series, count = markets.len(), samples = ?samples, "Kalshi series breakdown");
            }
        }

        // ── Count actual crypto markets (word-boundary) ────────────────────────
        let poly_crypto_count = poly_markets.iter()
            .filter(|m| m.category == MarketCategory::Crypto)
            .count();
        let kalshi_15m_count = kalshi_markets.iter()
            .filter(|m| m.platform_market_id.contains("15M"))
            .count();
        tracing::info!(
            poly_crypto = poly_crypto_count,
            kalshi_15m = kalshi_15m_count,
            total_poly = poly_markets.len(),
            total_kalshi = kalshi_markets.len(),
            "Diagnostic: market category counts"
        );

        // ── Show soonest-expiring Polymarket markets ───────────────────────────
        // These are the most likely candidates for Kalshi overlap. If ALL of these
        // expire weeks or months away, there is no overlap possible today.
        {
            let mut sorted_poly: Vec<&DiscoveredMarket> = poly_markets.iter().collect();
            sorted_poly.sort_by_key(|m| m.expiration);
            tracing::info!("Soonest-expiring Polymarket markets:");
            for pm in sorted_poly.iter().take(15) {
                tracing::info!(q = %pm.question_normalized, exp = %pm.expiration, "  [POLY soonest]");
            }
        }

        let mut matched: Vec<MatchedMarket> = Vec::new();
        let mut seen_pairs = std::collections::HashSet::new();

        for pm in poly_markets.iter() {
            // Attempt to improve midnight expirations from the title text
            let mut pm_expiration = pm.expiration;
            if (pm_expiration.hour() == 0 && pm_expiration.minute() == 0) || (pm_expiration.hour() == 23 && pm_expiration.minute() == 59) {
                pm_expiration = Self::try_fix_expiration_from_title(&pm.question, pm_expiration);
            }
            let pm_hour = pm_expiration.timestamp() / 3600;

            // ── Candidate discovery via inverted index ────────────────────────
            // Find all Kalshi markets that share ≥1 meaningful word with this
            // Poly market, without scanning the entire Kalshi market list.
            let mut candidate_set: std::collections::HashSet<usize> = std::collections::HashSet::new();

            // Also always include Kalshi markets in the ±1h window (catches 15m crypto)
            for hour_offset in -1i64..=1 {
                if let Some(idxs) = kalshi_by_hour.get(&(pm_hour + hour_offset)) {
                    candidate_set.extend(idxs);
                }
            }

            // Add word-match candidates regardless of time window
            for word in pm.question_normalized.split_whitespace() {
                if !stop_words.contains(word) {
                    if let Some(idxs) = word_to_kalshi.get(word) {
                        candidate_set.extend(idxs);
                    }
                }
            }

            // ── Evaluate each candidate ───────────────────────────────────────
            for ki in candidate_set {
                let km = &kalshi_markets[ki];
                let exp_diff_secs = (pm_expiration - km.expiration).num_seconds().abs();

                let extract_crypto = |q: &str| -> std::collections::HashSet<String> {
                    q.split_whitespace()
                        .filter(|w| matches!(*w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "hype" | "ada" | "crypto"))
                        .map(|w| w.to_string())
                        .collect()
                };
                let crypto_pm = extract_crypto(&pm.question_normalized);
                let crypto_km = extract_crypto(&km.question_normalized);
                let is_crypto = pm.category == MarketCategory::Crypto
                    || km.category == MarketCategory::Crypto
                    || !crypto_pm.is_empty()
                    || !crypto_km.is_empty();
                
                let shared_crypto = crypto_pm.intersection(&crypto_km).count() > 0;

                let is_15m_market = pm.question_normalized.contains("15 min")
                    || km.question_normalized.contains("15 min")
                    || km.platform_market_id.contains("15M")
                    || pm.question_normalized.split_whitespace().any(|w| w == "15m")
                    || km.question_normalized.split_whitespace().any(|w| w == "15m");

                if is_15m_market {
                    // Strict 120s threshold for 15-minute candles to avoid cross-matching adjacent candles
                    if exp_diff_secs > 120 { continue; }
                    
                    // Only log when at least one side is crypto (eliminates "NY 15 house seat" noise)
                    if is_crypto || shared_crypto {
                        tracing::info!(
                            poly_q = %pm.question_normalized,
                            kalshi_q = %km.question_normalized,
                            exp_diff_secs,
                            poly_exp = %pm_expiration,
                            kalshi_exp = %km.expiration,
                            "EVALUATING 15M CRYPTO CANDIDATE PAIR"
                        );
                    }
                } else if is_crypto {
                    if exp_diff_secs > 7 * 24 * 3600 { continue; }
                } else if exp_diff_secs > 48 * 3600 {
                    continue;
                }

                let sim;
                let same_category = pm.category == km.category;

                // HARDCODED MATCH FOR 15 MIN CRYPTO
                // Revert to strict 120s delta limit for safe execution
                if is_15m_market && is_crypto && shared_crypto && exp_diff_secs <= 120 {
                    tracing::info!("FORCING 15M CRYPTO MATCH: {}", pm.question);
                    sim = 1.0; 
                } else {
                    // ── Jaccard similarity ────────────────────────────────────────
                    let tokens_a: std::collections::HashSet<&str> = pm.question_normalized
                        .split_whitespace()
                        .filter(|w| !stop_words.contains(w))
                        .collect();
                    let tokens_b = &kalshi_tokens[ki];
                    let intersection = tokens_a.iter().filter(|w| tokens_b.contains(**w)).count();
                    let union = tokens_a.len() + tokens_b.len() - intersection;
                    sim = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };
                }

                let min_sim = if is_15m_market && is_crypto && shared_crypto {
                    0.01
                } else if same_category && exp_diff_secs <= 24 * 3600 {
                    0.35
                } else if same_category && exp_diff_secs <= 7 * 24 * 3600 {
                    0.40
                } else {
                    0.50
                };

                if sim < min_sim { continue; }

                tracing::debug!(
                    poly = %pm.question_normalized,
                    kalshi = %km.question_normalized,
                    exp_diff_secs,
                    sim = format!("{:.2}", sim),
                    min_sim,
                    "Candidate pair passed similarity gate"
                );

                // Numerical target guard — prevent mismatched strikes ($60k vs $70k)
                if !is_15m_market {
                    let extract_nums = |q: &str| -> std::collections::HashSet<i64> {
                        q.split_whitespace()
                            .filter_map(|w| {
                                let mut multiplier = 1.0;
                                let mut cleaned = w.replace(['$', ',', '%', '?', '(', ')', '[', ']', ':'], "");
                                if cleaned.ends_with('k') || cleaned.ends_with('K') {
                                    multiplier = 1000.0;
                                    cleaned.pop();
                                }
                                if let Ok(f) = cleaned.parse::<f64>() {
                                    // Ignore common year numbers to prevent false positive overlaps
                                    if f >= 2020.0 && f <= 2030.0 && f.fract() == 0.0 {
                                        return None;
                                    }
                                    Some((f * multiplier * 100.0).round() as i64)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    };
                    // Use the ORIGINAL question to preserve decimal points (e.g. 3.50)
                    let nums_pm = extract_nums(&pm.question);
                    let nums_km = extract_nums(&km.question);
                    
                    // If either question has a numeric target, there MUST be an overlap.
                    // This prevents matching a specific target ("over 3.50") with a generic binary ("rate cut").
                    if !nums_pm.is_empty() || !nums_km.is_empty() {
                        if nums_pm.intersection(&nums_km).next().is_none() {
                            continue;
                        }
                    }
                }

                let unified_id = compute_unified_market_id(
                    &pm.question,
                    "cross_platform",
                    &pm_expiration.to_rfc3339(),
                );
                if !seen_pairs.insert(unified_id) { continue; }

                let mut platform_infos = HashMap::new();
                platform_infos.insert(pm.platform, PlatformMarketInfo {
                    platform: pm.platform,
                    platform_market_id: pm.platform_market_id.clone(),
                    fee_rate_bps: pm.fee_rate_bps,
                    min_order_size: pm.min_order_size,
                    tick_size: pm.tick_size,
                });
                platform_infos.insert(km.platform, PlatformMarketInfo {
                    platform: km.platform,
                    platform_market_id: km.platform_market_id.clone(),
                    fee_rate_bps: km.fee_rate_bps,
                    min_order_size: km.min_order_size,
                    tick_size: km.tick_size,
                });

                let category = {
                    let q = pm.question_normalized.as_str();
                    if q.split_whitespace().any(|w| matches!(w, "trump" | "election" | "biden" | "harris" | "democrat" | "republican" | "congress" | "senate")) {
                        MarketCategory::Politics
                    } else if q.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "hype" | "ada" | "crypto")) {
                        MarketCategory::Crypto
                    } else if q.split_whitespace().any(|w| matches!(w, "nba" | "nfl" | "nhl" | "mlb" | "ufc" | "celtics" | "lakers" | "warriors" | "knicks" | "playoffs")) {
                        MarketCategory::Sports
                    } else {
                        MarketCategory::Other
                    }
                };

                let confidence = if sim >= 0.95 { 0.99 } else if sim >= 0.7 { 0.98 } else { 0.95 };

                info!(
                    poly_q = %pm.question_normalized,
                    kalshi_q = %km.question_normalized,
                    sim = format!("{:.2}", sim),
                    min_sim,
                    exp_diff_secs,
                    is_15m_market,
                    same_category,
                    poly_exp = %pm_expiration,
                    kalshi_exp = %km.expiration,
                    "MATCH CONFIRMED"
                );

                matched.push(MatchedMarket {
                    market: Market {
                        unified_id,
                        question: format!("{} / {}", pm.question, km.question),
                        resolution_source: "cross_platform".into(),
                        expiration: km.expiration,
                        platforms: platform_infos,
                        category,
                        confidence,
                        status: MarketStatus::Active,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    }
                });
            }
        }

        info!(
            total_matched = matched.len(),
            "Discovery cycle: cross-platform matching complete"
        );
        if matched.is_empty() {
            warn!(
                poly_count = poly_markets.len(),
                kalshi_count = kalshi_markets.len(),
                "WARNING: 0 markets matched. Sample questions for diagnosis:"
            );
            for pm in poly_markets.iter().take(5) {
                warn!(q = %pm.question_normalized, exp = %pm.expiration, "  [POLY sample]");
            }
            for km in kalshi_markets.iter().take(5) {
                warn!(q = %km.question_normalized, exp = %km.expiration, ticker = %km.platform_market_id, "  [KALSHI sample]");
            }
        }

        Ok(matched)
    }


pub(super) fn normalize_question(q: &str) -> String {
    // 1. Single initial allocation - strip commas and dollar signs immediately
    // to avoid splitting "$60,000" into "60 000" during the alphanumeric pass.
    let mut lower = q.to_lowercase().replace(',', "").replace('$', "");

    // 2. Fast zero-allocation lookahead: Only allocate a new string if the word actually exists
    let replacements = [
        ("bitcoin", "btc"),
        ("ethereum", "eth"),
        ("solana", "sol"),
        ("ripple", "xrp"),
        ("dogecoin", "doge"),
        ("binance coin", "bnb"),
        ("cardano", "ada"),
        ("minutes", "min"),
        ("minute", "min"),
        ("mins", "min"),
    ];

    for (from, to) in replacements {
        if lower.contains(from) {
            lower = lower.replace(from, to);
        }
    }

    // 3. Single-pass iteration to strip non-alphanumeric chars and deduplicate spaces without Vecs
    let mut result = String::with_capacity(lower.len());
    let mut last_was_space = true;

    for c in lower.chars() {
        if c.is_alphanumeric() {
            result.push(c);
            last_was_space = false;
        } else if !last_was_space {
            result.push(' ');
            last_was_space = true;
        }
    }

    // Clean up trailing space if the string ended with a special character
    if result.ends_with(' ') {
        result.pop();
    }

    result
}
    async fn fetch_polymarket_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let mut all_markets = Vec::new();
        let now = Utc::now();
        let gamma_url = "https://gamma-api.polymarket.com/markets";
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // ── Helper: parse a page and deduplicate ─────────────────────────────
        let dedup = |markets: Vec<DiscoveredMarket>, seen: &mut std::collections::HashSet<String>| -> Vec<DiscoveredMarket> {
            markets.into_iter().filter(|m| seen.insert(m.platform_market_id.clone())).collect()
        };

        // Pass 1: Near-term markets expiring within 7 days.
        {
            let week_out = now + chrono::Duration::days(7);
            let end_max = week_out.format("%Y-%m-%dT%H:%M:%SZ").to_string();
            let resp_result = self.http.get(gamma_url)
                .query(&[
                    ("active", "true"),
                    ("closed", "false"),
                    ("limit", "100"), // Reduced limit to prevent Cloudflare blocks
                    ("end_date_max", end_max.as_str()),
                ])
                .send().await;
            match resp_result {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        warn!("Polymarket near-term fetch returned HTTP {}", resp.status());
                    } else {
                        match resp.json::<serde_json::Value>().await {
                            Ok(data) => {
                                let page = Self::extract_markets_array(&data);
                                let markets = self.parse_gamma_polymarket_response(page, now);
                                let deduped = dedup(markets, &mut seen_ids);
                                info!(count = deduped.len(), "Polymarket near-term markets (≤7d expiry)");
                                all_markets.extend(deduped);
                            }
                            Err(e) => warn!(error = %e, "Polymarket near-term parse failed"),
                        }
                    }
                },
                Err(e) => warn!(error = %e, "Polymarket near-term fetch failed"),
            }
        }

        // Pass 2: Targeted keyword searches for topics that have Kalshi equivalents.
        let keywords: &[(&str, &str)] = &[
            ("btc",             "BTC up or down"),
            ("bitcoin",         "Bitcoin"),
            ("ethereum",        "ETH price"),
            ("eth price",       "ETH intraday"),
            ("15m",             "15m crypto"),
            ("15 min",          "15 min crypto"),
            ("up or down",      "Up/Down Crypto"),
            ("cpi",             "CPI inflation"),
            ("inflation",       "inflation rate"),
            ("federal reserve", "Fed rate"),
            ("interest rate",   "interest rate"),
            ("nba",             "NBA game"),
            ("playoffs",        "NBA playoffs"),
            ("nhl",             "NHL game"),
            ("mlb",             "MLB game"),
            ("ufc",             "UFC fight"),
        ];

        let keyword_futures: Vec<_> = keywords.iter().map(|(kw, label)| {
            let http = self.http.clone();
            let gamma_url = gamma_url.to_string();
            let kw = *kw;
            let label = *label;
            async move {
                let resp = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    http.get(&gamma_url)
                        .query(&[
                            ("active", "true"),
                            ("closed", "false"),
                            ("limit", "100"), // Reduced
                            ("q", kw),
                        ])
                        .send()
                ).await;
                match resp {
                    Ok(Ok(r)) => {
                        if !r.status().is_success() { None }
                        else {
                            match r.json::<serde_json::Value>().await {
                                Ok(data) => Some((kw, label, data)),
                                Err(_) => None,
                            }
                        }
                    },
                    _ => None,
                }
            }
        }).collect();

        let kw_results = futures_util::future::join_all(keyword_futures).await;
        for result in kw_results {
            if let Some((kw, label, data)) = result {
                let page = Self::extract_markets_array(&data);
                let markets = self.parse_gamma_polymarket_response(page, now);
                let deduped = dedup(markets, &mut seen_ids);
                if !deduped.is_empty() {
                    info!(keyword = kw, label, count = deduped.len(), "Polymarket keyword search results");
                }
                all_markets.extend(deduped);
            }
        }

        // Pass 3: General pagination — broad coverage of all active markets.
        {
            let mut offset = 0u64;
            const MAX_GENERAL: usize = 10_000;
            'general: loop {
                let offset_str = offset.to_string();
                let resp = match self.http.get(gamma_url)
                    .query(&[
                        ("active", "true"),
                        ("closed", "false"),
                        ("acceptingOrders", "true"),
                        ("limit", "100"), // Reduced
                        ("offset", offset_str.as_str()),
                    ])
                    .send().await {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!("Polymarket general fetch failed: {}", e);
                            break 'general;
                        }
                    };
                
                if !resp.status().is_success() {
                    tracing::warn!("Polymarket general fetch returned HTTP {}", resp.status());
                    break 'general;
                }

                let data = match resp.json::<serde_json::Value>().await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("Polymarket general parse failed: {}", e);
                        break 'general;
                    }
                };
                let page = Self::extract_markets_array(&data);
                if page.is_empty() { break 'general; }
                let page_len = page.len();
                let markets = self.parse_gamma_polymarket_response(page, now);
                let deduped = dedup(markets, &mut seen_ids);
                all_markets.extend(deduped);

                if all_markets.len() >= MAX_GENERAL || page_len < 100 { break 'general; }
                offset += 100;
            }
        }

        Ok(all_markets)
    }

    /// Parse a Polymarket Gamma API market array.
    /// Gamma markets use camelCase fields and `clobTokenIds` for the YES/NO token pair.
    fn parse_gamma_polymarket_response(
        &self,
        arr: &[serde_json::Value],
        now: DateTime<Utc>,
    ) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();

        for item in arr {
            let question = item
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            
            if question.is_empty() {
                continue;
            }

            let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");

            // Only process markets that are currently accepting orders
            let accepting = item.get("acceptingOrders")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // For 15-minute crypto markets, the order acceptance window closes before
            // the market resolves. Skipping them here prevents discovery from ever
            // seeing them. Allow non-accepting markets through so they can be matched
            // and subscribed to — the CLOB API rejects stale orders at submission time.
            // We only skip non-accepting markets that are clearly long-dated.
            if !accepting {
                // Peek at the question to check if this is a short-lived crypto candle
                let q_lower = question.to_lowercase();
                let is_short_lived_crypto = (q_lower.contains("up or down") || q_lower.contains("15 min") || q_lower.contains("15m") || slug.contains("-15m-"))
                    && (q_lower.contains("bitcoin") || q_lower.contains("btc") || q_lower.contains("ethereum") || q_lower.contains("eth")
                        || q_lower.contains("solana") || q_lower.contains("sol") || q_lower.contains("doge") || q_lower.contains("xrp")
                        || q_lower.contains("bnb") || q_lower.contains("hype"));
                if !is_short_lived_crypto {
                    continue;
                }
            }

            // Gamma API: clobTokenIds is a JSON-serialized string like
            // "[\"123456...\", \"789012...\"]" — NOT a native JSON array.
            // We must parse the string value to extract the token IDs.
            let (yes_token, no_token) = {
                let raw = item.get("clobTokenIds")
                    .map(|v| if v.is_string() { v.as_str().unwrap().to_string() } else { v.to_string() })
                    .unwrap_or_default();
                
                if !raw.is_empty() {
                    let parsed: Vec<String> = serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::from_str(&raw.replace("\\\"", "\"").trim_matches('"')).unwrap_or_default());
                    let yes = parsed.first().cloned().unwrap_or_default();
                    let no  = parsed.get(1).cloned().unwrap_or_default();
                    (yes, no)
                } else {
                    // Fallback: legacy CLOB format tokens:[{token_id:"..."}, ...]
                    let yes = item.get("tokens").and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|t| t.get("token_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("").to_string();
                    let no = item.get("tokens").and_then(|v| v.as_array())
                        .and_then(|a| a.get(1))
                        .and_then(|t| t.get("token_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("").to_string();
                    (yes, no)
                }
            };

            if yes_token.is_empty() || no_token.is_empty() {
                continue;
            }
            let token_id = format!("{},{}", yes_token, no_token);

            // Gamma uses endDateIso (ISO string), endDate (may be Unix timestamp OR ISO string),
            // or end_date_iso (CLOB format). Try each field, handling both string and numeric types.
            let end_date = {
                let mut resolved: Option<DateTime<Utc>> = None;

                // NEW LOGIC: Accurately extract timestamp from 15m crypto slugs
                if slug.contains("-15m-") || slug.contains("-updown-15m-") {
                    if let Some(last_dash) = slug.rfind('-') {
                        if let Ok(ts) = slug[last_dash + 1..].parse::<i64>() {
                            // Slug timestamp is the start of the 15m period. Expiration is 15 mins (900s) later.
                            if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, ts + 900, 0) {
                                resolved = Some(dt);
                            }
                        }
                    }
                }

                if resolved.is_none() {
                    let candidate_fields = ["endDateIso", "endDate", "end_date_iso", "closeTime", "close_time", "expiration"];
                    for field in &candidate_fields {
                    if let Some(v) = item.get(field) {
                        // Try as ISO string first
                        if let Some(s) = v.as_str() {
                            // Handle formats: RFC3339, "YYYY-MM-DD", "YYYY-MM-DDTHH:MM:SS"
                            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                                resolved = Some(dt.with_timezone(&Utc));
                                break;
                            } else if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
                                resolved = Some(DateTime::from_naive_utc_and_offset(naive, Utc));
                                break;
                            } else if let Ok(naive) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                                resolved = Some(DateTime::from_naive_utc_and_offset(naive.and_hms_opt(0, 0, 0).unwrap(), Utc));
                                break;
                            }
                        }
                        // Try as Unix timestamp (seconds or milliseconds)
                        if let Some(ts) = v.as_i64() {
                            let secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
                            if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, secs, 0) {
                                resolved = Some(dt);
                                break;
                            }
                        }
                        if let Some(ts) = v.as_f64() {
                            let secs = if ts > 1_000_000_000_000.0 { (ts / 1000.0) as i64 } else { ts as i64 };
                            if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, secs, 0) {
                                resolved = Some(dt);
                                break;
                            }
                        }
                    }
                }
            }
                resolved.unwrap_or_else(|| now + chrono::Duration::days(30))
            };

            // Skip already-expired markets
            if end_date <= now {
                continue;
            }

            let question_normalized = Self::normalize_question(&question);
            let category = if question_normalized.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "hype" | "ada" | "crypto")) {
                MarketCategory::Crypto
            } else if question_normalized.split_whitespace().any(|w| matches!(w, "trump" | "election" | "biden" | "harris" | "democrat" | "republican" | "congress" | "senate")) {
                MarketCategory::Politics
            } else {
                MarketCategory::Other
            };

            markets.push(DiscoveredMarket {
                platform: Platform::Polymarket,
                platform_market_id: token_id,
                question_normalized,
                question,
                resolution_source: "polymarket".into(),
                expiration: end_date,
                category,
                fee_rate_bps: 200,
                min_order_size: Decimal::ONE,
                tick_size: Decimal::new(1, 2),
            });
        }

        markets
    }

    async fn fetch_kalshi_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let url = format!("{}/markets", self.platforms_config.kalshi.rest_url);

        // 1. Generic open markets — first page only (200 markets, non-parlay).
        // We skip full cursor pagination since the Kalshi generic endpoint is dominated
        // by KXMVE complex parlays. One page gives us a representative non-parlay sample,
        // and the targeted series fetch (step 2) covers all actionable market types.
        let generic_markets = {
            let mut req = self.http.get(&url)
                .query(&[("status", "open"), ("limit", "200")]); // Reduced limit
            if let Some(auth) = &self.kalshi_auth {
                if let Ok(token) = auth.generate_token() {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
            }
            match req.send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let mut seen = std::collections::HashSet::new();
                        self.parse_kalshi_markets(&data, &mut seen)
                    }
                    Err(e) => { warn!(error = %e, "Kalshi generic page parse failed"); vec![] }
                },
                Err(e) => { warn!(error = %e, "Kalshi generic page fetch failed"); vec![] }
            }
        };
        info!(count = generic_markets.len(), "Kalshi generic markets fetched (non-parlay)");

        // 2. Targeted series — fetched in parallel, each with a 10s timeout.
        // These series are known to have Polymarket equivalents and are often absent
        // from or underrepresented in the generic endpoint.
        let targeted_series: &[(&str, &str)] = &[
            // ── Crypto 15-minute direction ──────────────────────────────────────
            ("KXBTC15M",  "BTC 15m"),
            ("KXETH15M",  "ETH 15m"),
            ("KXSOL15M",  "SOL 15m"),
            ("KXDOGE15M", "DOGE 15m"),
            ("KXXRP15M",  "XRP 15m"),
            ("KXBNB15M",  "BNB 15m"),
            ("KXHYPE15M", "HYPE 15m"),
            // ── Crypto price targets (daily/weekly close) ───────────────────────
            ("KXBTC",     "BTC price target"),
            ("KXETH",     "ETH price target"),
            ("KXSOL",     "SOL price target"),
            // ── Sports: single-game outcomes ────────────────────────────────────
            ("KXNBA",     "NBA game"),
            ("KXNBAPLAYOFFS", "NBA playoffs"),
            ("KXNHL",     "NHL game"),
            ("KXMLB",     "MLB game"),
            ("KXUFC",     "UFC bout"),
            // ── Political / economic ─────────────────────────────────────────────
            ("KXFED",     "Fed rate decision"),
            ("KXCPI",     "CPI inflation"),
            ("KXTRUMP",   "Trump event"),
        ];

        // Fire all series requests concurrently
        let series_futures: Vec<_> = targeted_series.iter().map(|(series, label)| {
            let http = self.http.clone();
            let url = url.clone();
            let auth_token = self.kalshi_auth.as_ref()
                .and_then(|a| a.generate_token().ok());
            let series = *series;
            let label = *label;
            async move {
                let mut req = http.get(&url)
                    .query(&[("status", "open"), ("series_ticker", series), ("limit", "200")]);
                if let Some(token) = auth_token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
                match tokio::time::timeout(
                    std::time::Duration::from_secs(25),
                    req.send()
                ).await {
                    Ok(Ok(resp)) => match resp.json::<serde_json::Value>().await {
                        Ok(data) => Some((series, label, data)),
                        Err(e) => { tracing::debug!(series, error = %e, "Kalshi series parse failed"); None }
                    },
                    Ok(Err(e)) => { tracing::debug!(series, error = %e, "Kalshi series fetch failed"); None }
                    Err(_) => { tracing::debug!(series, "Kalshi series fetch timed out"); None }
                }
            }
        }).collect();

        let series_results = futures_util::future::join_all(series_futures).await;

        // Merge all results — deduplicate across generic + series using a shared seen set
        let mut all_markets = generic_markets;
        let mut seen_tickers: std::collections::HashSet<String> =
            all_markets.iter().map(|m| m.platform_market_id.clone()).collect();

        for result in series_results {
            if let Some((series, label, data)) = result {
                let page_markets = self.parse_kalshi_markets_filtered(&data, &mut seen_tickers, true);
                if !page_markets.is_empty() {
                    info!(series, label, count = page_markets.len(), "Fetched targeted Kalshi series");
                }
                all_markets.extend(page_markets);
            }
        }

        Ok(all_markets)
    }

    fn parse_kalshi_markets(
        &self,
        resp: &serde_json::Value,
        seen_tickers: &mut std::collections::HashSet<String>,
    ) -> Vec<DiscoveredMarket> {
        self.parse_kalshi_markets_filtered(resp, seen_tickers, false)
    }

    fn parse_kalshi_markets_filtered(
        &self,
        resp: &serde_json::Value,
        seen_tickers: &mut std::collections::HashSet<String>,
        include_parlays: bool,
    ) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();
        if let Some(arr) = resp.get("markets").and_then(|v| v.as_array()) {
            for item in arr {
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if title.is_empty() { continue; }
                let ticker = item.get("ticker").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if ticker.is_empty() { continue; }

                // Skip complex multi-variable parlay markets — they resolve on combinations
                // of player props/game outcomes and have no equivalent on Polymarket.
                // These are identified by the KXMVE prefix (Multi-Variable Event).
                if !include_parlays && ticker.starts_with("KXMVE") {
                    continue;
                }

                if !seen_tickers.insert(ticker.clone()) { continue; } // deduplicate

                let close_time = item
                    .get("close_time")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

                // Skip markets that have already expired
                if close_time < Utc::now() {
                    continue;
                }

                let question_normalized = Self::normalize_question(&title);
                let category = if question_normalized.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "hype" | "ada" | "crypto")) {
                    MarketCategory::Crypto
                } else if question_normalized.split_whitespace().any(|w| matches!(w, "trump" | "election" | "biden" | "harris" | "democrat" | "republican" | "congress" | "senate")) {
                    MarketCategory::Politics
                } else {
                    MarketCategory::Other
                };

                markets.push(DiscoveredMarket {
                    platform: Platform::Kalshi,
                    platform_market_id: ticker,
                    question_normalized,
                    question: title,
                    resolution_source: "kalshi".into(),
                    expiration: close_time,
                    category,
                    fee_rate_bps: 175,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2),
                });
            }
        }
        markets
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod discovery_tests;
//! Market discovery: polls platform REST APIs, matches equivalent markets
//! cross-platform, and registers them in the MarketRegistry.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Timelike};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use regex::Regex;

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
    pub fn new(platforms_config: PlatformsConfig, poll_interval_secs: u64, kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>) -> Self {
        let http = reqwest::Client::builder()
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("mercury-discovery/1.0")
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
                    info!(count = matched.len(), "Discovery cycle complete");
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
        // Regex for patterns like "8:15AM", "8:15 AM", "8 PM", "8PM", "20:15"
        // We look for a pattern that looks like a time.
        let re = Regex::new(r"(\d{1,2})(?::(\d{2}))?\s*(am|pm)?").unwrap();
        
        if let Some(caps) = re.captures_iter(&lower).last() {
            let mut hour: u32 = caps.get(1).unwrap().as_str().parse::<u32>().unwrap_or(0);
            let min: u32 = caps.get(2).map(|m| m.as_str().parse::<u32>().unwrap_or(0)).unwrap_or(0);
            let ampm = caps.get(3).map(|m| m.as_str());

            if let Some(ampm_str) = ampm {
                if ampm_str == "pm" && hour < 12 {
                    hour += 12;
                } else if ampm_str == "am" && hour == 12 {
                    hour = 0;
                }
            }

            if hour < 24 && min < 60 {
                // If it's a "March 19, 8:15AM-8:20AM" title, we want the LAST time mentioned (the end of candle)
                // base_date is usually midnight UTC of the correct day.
                // We assume ET for these titles if not specified, but Kalshi uses UTC in API.
                // Polymarket titles are almost always ET.
                
                // For simplicity, we just set the hour/min on the base_date.
                // If base_date is midnight, this works.
                if let Some(new_date) = base_date.with_hour(hour).and_then(|d| d.with_minute(min)) {
                    // Adjust for ET to UTC (ET is UTC-4 or UTC-5). 
                    // Most Polymarket 15m crypto titles are ET.
                    // We'll assume ET for now as it's the most common case for these specific titles.
                    let is_et = lower.contains("et") || lower.contains("eastern");
                    if is_et {
                        // 4 hours difference (assuming summer time for now, or just generic offset)
                        // In March it's EDT (UTC-4).
                        return new_date + chrono::Duration::hours(4);
                    }
                    return new_date;
                }
            }
        }
        base_date
    }

    async fn discover_and_match(&self) -> Result<Vec<MatchedMarket>> {
        let mut all_discovered: Vec<DiscoveredMarket> = Vec::new();

        if self.platforms_config.polymarket.enabled {
            match self.fetch_polymarket_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Polymarket markets fetched");
                    all_discovered.extend(markets);
                }
                Err(e) => warn!(error = %e, "Failed to fetch Polymarket markets"),
            }
        }

        if self.platforms_config.kalshi.enabled {
            match self.fetch_kalshi_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Kalshi markets fetched");
                    all_discovered.extend(markets);
                }
                Err(e) => warn!(error = %e, "Failed to fetch Kalshi markets"),
            }
        }

        let mut matched = Vec::new();
        let mut seen_pairs = std::collections::HashSet::new();
        let poly_markets: Vec<_> = all_discovered.iter().filter(|m| m.platform == Platform::Polymarket).collect();
        let kalshi_markets: Vec<_> = all_discovered.iter().filter(|m| m.platform == Platform::Kalshi).collect();

        // H-3 FIX: Group Kalshi markets by expiration hour to reduce O(N^2) complexity to O(N * K)
        let mut kalshi_by_hour: HashMap<i64, Vec<&DiscoveredMarket>> = HashMap::new();
        for km in &kalshi_markets {
            let hour = km.expiration.timestamp() / 3600;
            kalshi_by_hour.entry(hour).or_default().push(km);
        }

        for pm in &poly_markets {
            // FIX: If Poly expiration is midnight, try to fix it from title
            let mut pm_expiration = pm.expiration;
            if pm_expiration.hour() == 0 && pm_expiration.minute() == 0 {
                pm_expiration = Self::try_fix_expiration_from_title(&pm.question, pm_expiration);
            }

            let pm_hour = pm_expiration.timestamp() / 3600;
            // Only compare with Kalshi markets expiring in the same, previous, or next 48 hours
            for hour_offset in -48..=48 {
                if let Some(kms) = kalshi_by_hour.get(&(pm_hour + hour_offset)) {
                    for km in kms {
                        // 1. Dual-Tier Expiration Check
                let exp_diff_secs = (pm_expiration - km.expiration).num_seconds().abs();
                
                let is_15m_market = pm.question_normalized.contains("15 min") || 
                                    km.question_normalized.contains("15 min") ||
                                    km.platform_market_id.contains("15M") ||
                                    pm.question_normalized.contains("15m") ||
                                    km.question_normalized.contains("15m");

                if is_15m_market {
                    // CRITICAL: 15-minute candles MUST expire at basically the exact same time (within 5 mins for safety)
                    if exp_diff_secs > 300 { continue; }
                } else {
                    // Standard generic markets (e.g. politics, yearly price targets)
                    if exp_diff_secs > 48 * 3600 { continue; }
                }

                // 2. Token Overlap Jaccard Similarity
                let tokens_a: std::collections::HashSet<&str> = pm.question_normalized.split_whitespace().collect();
                let tokens_b: std::collections::HashSet<&str> = km.question_normalized.split_whitespace().collect();
                let intersection = tokens_a.intersection(&tokens_b).count();
                let union = tokens_a.union(&tokens_b).count();
                let sim = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };

                // 70% overlap for safer automated cross-platform matching
                // CRITICAL FIX: Relax threshold for 15-min crypto markets that match on asset and expiration
                let is_crypto = pm.category == MarketCategory::Crypto || km.category == MarketCategory::Crypto ||
                                pm.question_normalized.contains("btc") || km.question_normalized.contains("btc") ||
                                pm.question_normalized.contains("eth") || km.question_normalized.contains("eth") ||
                                pm.question_normalized.contains("sol") || km.question_normalized.contains("sol");

                if is_15m_market && is_crypto && km.question_normalized.contains("btc") {
                    tracing::info!("Checking 15m crypto pair: Poly='{}' (exp: {}) vs Kalshi='{}' (exp: {}), diff: {}s, sim: {:.2}", 
                        pm.question_normalized, pm_expiration, km.question_normalized, km.expiration, exp_diff_secs, sim);
                }

                let match_confirmed = if is_15m_market && is_crypto && exp_diff_secs <= 120 {
                    // For 15m crypto, if they expire at the same time, they are almost certainly the same candle.
                    // We allow a much lower similarity to account for "Up or Down" vs "Target $X" phrasing.
                    sim >= 0.2
                } else {
                    sim >= 0.7
                };

                if match_confirmed {
                    // CRITICAL FIX: Extract numerical targets to prevent mismatched strikes (e.g. $60k vs $70k)
                    let nums_pm: Vec<f64> = pm.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();
                    let nums_km: Vec<f64> = km.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();

                    // For 15m crypto "Up or Down" markets, one platform often omits the price from the title.
                    // We allow the match if one list is empty, but if BOTH have numbers, they MUST match.
                    if !nums_pm.is_empty() && !nums_km.is_empty() && nums_pm != nums_km {
                        continue;
                    }

                    let unified_id = compute_unified_market_id(
                        &pm.question,
                        "cross_platform",
                        &pm_expiration.to_rfc3339(),
                    );

                    if !seen_pairs.insert(unified_id) {
                        continue;
                    }

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

                    matched.push(MatchedMarket {
                        market: Market {
                            unified_id,
                            question: format!("{} / {}", pm.question, km.question), // Store both for auditability
                            resolution_source: "cross_platform".into(),
                            expiration: km.expiration, // Use Kalshi's expiration as it's usually more precise
                            platforms: platform_infos,
                            // LOW-3 / MED-2 FIX: Better category extraction
                            category: {
                                let q = pm.question_normalized.as_str();
                                if q.contains("trump") || q.contains("election") || q.contains("biden") || q.contains("harris") { MarketCategory::Politics }
                                else if q.contains("bitcoin") || q.contains("btc") || q.contains("eth") || q.contains("crypto") || q.contains("sol") || q.contains("xrp") || q.contains("doge") || q.contains("bnb") { MarketCategory::Crypto }
                                else if q.contains("nba") || q.contains("nfl") || q.contains("super bowl") { MarketCategory::Sports }
                                else { MarketCategory::Other }
                            },
                            confidence: if sim > 0.7 { 0.98 } else { 0.95 },
                            // HIGH-1: All discovered markets start as Suspended to mandate human review, 
                            // EXCEPT for 15-minute crypto candles which move too fast for manual review.
                            status: if is_15m_market && is_crypto { MarketStatus::Active } else { MarketStatus::Suspended },
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                        }
                    });
                }
                    }
                }
            }
        }

        Ok(matched)
    }


pub(super) fn normalize_question(q: &str) -> String {
        // 1. Single initial allocation
        let mut lower = q.to_lowercase();
        
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
        // Fetch short-horizon markets first (soonest expiring = highest arb priority).
        // The Gamma API is the authoritative source for CURRENT active markets;
        // the old CLOB /markets endpoint only returns archived 2023 data.
        let gamma_url = "https://gamma-api.polymarket.com/markets";
        let mut offset = 0u64;
        // Cap at 10 000 markets per cycle to prevent runaway pagination.
        // Use acceptingOrders=true to filter server-side so we only get live markets.
        const MAX_MARKETS: usize = 10_000;
        const PAGE_SIZE: usize = 1_000;

        'gamma: loop {
            let offset_str = offset.to_string();
            let page_str = PAGE_SIZE.to_string();
            let query = vec![
                ("active", "true"),
                ("closed", "false"),
                ("acceptingOrders", "true"),
                ("limit", page_str.as_str()),
                ("offset", offset_str.as_str()),
            ];

            let resp: serde_json::Value = self
                .http
                .get(gamma_url)
                .query(&query)
                .send()
                .await
                .context("Polymarket Gamma markets fetch failed")?
                .json()
                .await
                .context("Polymarket Gamma markets parse failed")?;

            let arr = resp.as_array()
                .or_else(|| resp.get("data").and_then(|v| v.as_array()));

            let page = match arr {
                Some(a) => a,
                None => break 'gamma,
            };

            let page_len = page.len();
            let page_markets = self.parse_gamma_polymarket_response(page, now);
            all_markets.extend(page_markets);

            // Stop early if we've hit the cap or received a partial page
            if all_markets.len() >= MAX_MARKETS || page_len < PAGE_SIZE {
                break 'gamma;
            }

            offset += PAGE_SIZE as u64;
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
            // Only process markets that are currently accepting orders
            let accepting = item.get("acceptingOrders")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !accepting {
                continue;
            }

            let question = item
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if question.is_empty() {
                continue;
            }

            // Gamma API: clobTokenIds is a JSON-serialized string like
            // "[\"123456...\", \"789012...\"]" — NOT a native JSON array.
            // We must parse the string value to extract the token IDs.
            let (yes_token, no_token) = {
                let raw = item.get("clobTokenIds").and_then(|v| v.as_str()).unwrap_or("");
                if !raw.is_empty() {
                    // Parse the embedded JSON array string
                    let parsed: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
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

            if yes_token.is_empty() || yes_token.starts_with("0x") || no_token.is_empty() {
                continue;
            }
            let token_id = format!("{},{}", yes_token, no_token);

            // Gamma uses endDate or endDateIso (camelCase); CLOB uses end_date_iso
            let end_date = item.get("endDateIso")
                .or_else(|| item.get("endDate"))
                .or_else(|| item.get("end_date_iso"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| now + chrono::Duration::days(30));

            // Skip already-expired markets
            if end_date <= now {
                continue;
            }

            let question_normalized = Self::normalize_question(&question);
            let category = if question_normalized.contains("btc") || question_normalized.contains("eth") || question_normalized.contains("crypto") || question_normalized.contains("sol") || question_normalized.contains("xrp") || question_normalized.contains("doge") || question_normalized.contains("bnb") {
                MarketCategory::Crypto
            } else if question_normalized.contains("trump") || question_normalized.contains("election") || question_normalized.contains("biden") || question_normalized.contains("harris") {
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
        let mut all_markets = Vec::new();
        let mut seen_tickers = std::collections::HashSet::new();
        let url = format!("{}/markets", self.platforms_config.kalshi.rest_url);

        // 1. Generic open markets via cursor pagination (cap at 10k to prevent runaway)
        {
            let mut cursor = String::new();
            let mut generic_count = 0usize;
            const KALSHI_MAX_GENERIC: usize = 10_000;
            loop {
                let mut query = vec![("status", "open"), ("limit", "1000")];
                if !cursor.is_empty() {
                    query.push(("cursor", &cursor));
                }
                let mut req = self.http.get(&url).query(&query);
                if let Some(auth) = &self.kalshi_auth {
                    if let Ok(token) = auth.generate_token() {
                        req = req.header("Authorization", format!("Bearer {}", token));
                    }
                }
                let resp: serde_json::Value = req.send()
                    .await
                    .context("Kalshi markets fetch failed")?
                    .json()
                    .await
                    .context("Kalshi markets parse failed")?;

                let page_markets = self.parse_kalshi_markets(&resp, &mut seen_tickers);
                generic_count += page_markets.len();
                all_markets.extend(page_markets);

                cursor = resp.get("cursor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if cursor.is_empty() || cursor == "null" || generic_count >= KALSHI_MAX_GENERIC {
                    break;
                }
            }
        }

        // 2. CRITICAL: Explicitly fetch all crypto 15-minute series.
        // These short-lived markets are often not returned by the generic
        // cursor-paginated endpoint which favors longer-dated markets.
        let crypto_15m_series = [
            "KXBTC15M",  // Bitcoin
            "KXETH15M",  // Ethereum
            "KXSOL15M",  // Solana
            "KXBCH15M",  // Bitcoin Cash
            "KXADA15M",  // Cardano
            "KXDOGE15M", // Dogecoin
            "KXXRP15M",  // XRP
            "KXBNB15M",  // BNB
            "KXHYPE15M", // Hype
        ];

        for series in &crypto_15m_series {
            let query = vec![("status", "open"), ("series_ticker", *series), ("limit", "100")];
            let mut req = self.http.get(&url).query(&query);
            if let Some(auth) = &self.kalshi_auth {
                if let Ok(token) = auth.generate_token() {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
            }
            match req.send().await {
                Ok(resp) => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(data) => {
                            let page_markets = self.parse_kalshi_markets(&data, &mut seen_tickers);
                            if !page_markets.is_empty() {
                                info!(series = *series, count = page_markets.len(), "Fetched crypto 15-min series markets");
                            }
                            all_markets.extend(page_markets);
                        }
                        Err(e) => warn!(series = *series, error = %e, "Failed to parse Kalshi crypto series response"),
                    }
                }
                Err(e) => warn!(series = *series, error = %e, "Failed to fetch Kalshi crypto series"),
            }
        }

        Ok(all_markets)
    }

    fn parse_kalshi_markets(
        &self,
        resp: &serde_json::Value,
        seen_tickers: &mut std::collections::HashSet<String>,
    ) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();
        if let Some(arr) = resp.get("markets").and_then(|v| v.as_array()) {
            for item in arr {
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if title.is_empty() { continue; }
                let ticker = item.get("ticker").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if ticker.is_empty() { continue; }
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
                let category = if question_normalized.contains("btc") || question_normalized.contains("eth") || question_normalized.contains("crypto") || question_normalized.contains("sol") || question_normalized.contains("xrp") || question_normalized.contains("doge") || question_normalized.contains("bnb") {
                    MarketCategory::Crypto
                } else if question_normalized.contains("trump") || question_normalized.contains("election") || question_normalized.contains("biden") || question_normalized.contains("harris") {
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

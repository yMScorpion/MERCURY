//! Market discovery: polls platform REST APIs, matches equivalent markets
//! cross-platform, and registers them in the MarketRegistry.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

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
            let pm_hour = pm.expiration.timestamp() / 3600;
            // Only compare with Kalshi markets expiring in the same, previous, or next hour
            for hour_offset in -1..=1 {
                if let Some(kms) = kalshi_by_hour.get(&(pm_hour + hour_offset)) {
                    for km in kms {
                        // 1. Dual-Tier Expiration Check
                let exp_diff_secs = (pm.expiration - km.expiration).num_seconds().abs();
                let is_15m_market = pm.question_normalized.contains("15 min") || km.question_normalized.contains("15 min");

                if is_15m_market {
                    // CRITICAL: 15-minute candles MUST expire at basically the exact same time (within 2 mins)
                    // Otherwise a 14:00 candle might match with a 14:15 candle and result in naked exposure.
                    if exp_diff_secs > 120 { continue; }
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
                if sim >= 0.7 {
                    // CRITICAL FIX: Extract numerical targets to prevent mismatched strikes (e.g. $60k vs $70k)
                    let nums_pm: Vec<f64> = pm.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();
                    let nums_km: Vec<f64> = km.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();

                    if nums_pm != nums_km && (!nums_pm.is_empty() || !nums_km.is_empty()) {
                        tracing::warn!(
                            pm_question = %pm.question,
                            km_question = %km.question,
                            pm_nums = ?nums_pm,
                            km_nums = ?nums_km,
                            "DIFFERENT NUMERICAL TARGETS in fuzzy match — discarding match"
                        );
                        continue;
                    }

                    let unified_id = compute_unified_market_id(
                        &pm.question,
                        "cross_platform",
                        &pm.expiration.to_rfc3339(),
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
                            expiration: pm.expiration,
                            platforms: platform_infos,
                            // LOW-3 / MED-2 FIX: Better category extraction
                            category: {
                                let q = pm.question_normalized.as_str();
                                if q.contains("trump") || q.contains("election") || q.contains("biden") || q.contains("harris") { MarketCategory::Politics }
                                else if q.contains("bitcoin") || q.contains("btc") || q.contains("eth") || q.contains("crypto") { MarketCategory::Crypto }
                                else if q.contains("nba") || q.contains("nfl") || q.contains("super bowl") { MarketCategory::Sports }
                                else { MarketCategory::Other }
                            },
                            confidence: if sim > 0.7 { 0.98 } else { 0.95 },
                            // HIGH-1: All discovered markets start as Suspended to mandate human review, 
                            // preventing fuzzy matcher blindspots from executing mismatched strikes.
                            status: MarketStatus::Suspended,
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


fn normalize_question(q: &str) -> String {
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
            ("minutes", "min"),
            ("minute", "min"),
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
        let mut cursor = String::new();
        
        loop {
            let url = format!("{}/markets", self.platforms_config.polymarket.rest_url);
            let mut query = vec![("active", "true"), ("limit", "1000")];
            if !cursor.is_empty() {
                query.push(("next_cursor", &cursor));
            }
            
            let resp: serde_json::Value = self
                .http
                .get(&url)
                .query(&query)
                .send()
                .await
                .context("Polymarket markets fetch failed")?
                .json()
                .await
                .context("Polymarket markets parse failed")?;
            
            let page_markets = self.parse_polymarket_response(&resp);
            let page_count = page_markets.len();
            all_markets.extend(page_markets);
            
            // Check for pagination cursor
            cursor = resp.get("next_cursor")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            
            if cursor.is_empty() || page_count < 1000 {
                break;
            }
            
            // Safety: cap at 5000 markets to prevent infinite loops
            if all_markets.len() >= 5000 {
                tracing::warn!("Polymarket pagination capped at 5000 markets");
                break;
            }
        }
        
        Ok(all_markets)
    }
    
    fn parse_polymarket_response(&self, resp: &serde_json::Value) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();
        // Handle both top-level array and nested "data" array formats
        let arr = resp.as_array()
            .or_else(|| resp.get("data").and_then(|v| v.as_array()));
        
        if let Some(arr) = arr {
            for item in arr {
                let question = item
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if question.is_empty() {
                    continue;
                }
                
                // Polymarket CTF requires buying the specific NO token ID to short the market.
                // We extract both YES (index 0) and NO (index 1) token IDs and store them as a pair.
                let yes_token = item.get("tokens").and_then(|v| v.as_array()).and_then(|arr| arr.get(0)).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("");
                let no_token = item.get("tokens").and_then(|v| v.as_array()).and_then(|arr| arr.get(1)).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("");
                
                if yes_token.is_empty() || yes_token.starts_with("0x") || no_token.is_empty() {
                    continue; 
                }
                let token_id = format!("{},{}", yes_token, no_token);
                
                let end_date = item
                    .get("end_date_iso")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

                markets.push(DiscoveredMarket {
                    platform: Platform::Polymarket,
                    platform_market_id: token_id,
                    question_normalized: Self::normalize_question(&question),
                    question,
                    resolution_source: "polymarket".into(),
                    expiration: end_date,
                    category: MarketCategory::Other,
                    fee_rate_bps: 200,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2), // 0.01
                });
            }
        }
        
        if markets.len() >= 1000 {
            tracing::warn!("Polymarket returned 1000 markets — results may be truncated. Consider pagination.");
        }
        
        markets
    }

    async fn fetch_kalshi_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let url = format!("{}/markets", self.platforms_config.kalshi.rest_url);
        let mut req = self.http.get(&url).query(&[("status", "open"), ("limit", "1000")]);
        
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

        let mut markets = Vec::new();
        if let Some(arr) = resp
            .get("markets")
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                let ticker = item
                    .get("ticker")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if ticker.is_empty() {
                    continue;
                }
                let close_time = item
                    .get("close_time")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

                markets.push(DiscoveredMarket {
                    platform: Platform::Kalshi,
                    platform_market_id: ticker,
                    question_normalized: Self::normalize_question(&title),
                    question: title,
                    resolution_source: "kalshi".into(),
                    expiration: close_time,
                    category: MarketCategory::Other,
                    fee_rate_bps: 175,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2),
                });
            }
        }
        Ok(markets)
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod discovery_tests;

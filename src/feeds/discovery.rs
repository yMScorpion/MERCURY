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
}

impl MarketDiscovery {
    pub fn new(platforms_config: PlatformsConfig, poll_interval_secs: u64) -> Self {
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
                        if matched_tx.try_send(m).is_err() {
                            warn!("Matched market channel full — skipping");
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

        // Group by normalized question for cross-platform matching.
        let mut by_question: HashMap<String, Vec<DiscoveredMarket>> = HashMap::new();
        for dm in &all_discovered {
            by_question
                .entry(dm.question_normalized.clone())
                .or_default()
                .push(dm.clone());
        }

        let mut matched = Vec::new();
        for (norm_q, group) in &by_question {
            // Only create an arb-eligible market if ≥2 platforms carry it.
            let platforms_present: Vec<Platform> =
                group.iter().map(|g| g.platform).collect::<std::collections::HashSet<_>>()
                    .into_iter().collect();
            if platforms_present.len() < 2 {
                continue;
            }

            let first = &group[0];
            let unified_id = compute_unified_market_id(
                &first.question,
                &first.resolution_source,
                &first.expiration.to_rfc3339(),
            );

            let mut platform_infos: HashMap<Platform, PlatformMarketInfo> = HashMap::new();
            for dm in group {
                platform_infos.insert(
                    dm.platform,
                    PlatformMarketInfo {
                        platform: dm.platform,
                        platform_market_id: dm.platform_market_id.clone(),
                        fee_rate_bps: dm.fee_rate_bps,
                        min_order_size: dm.min_order_size,
                        tick_size: dm.tick_size,
                    },
                );
            }

            // Verify expirations are within 48 hours of each other.
            // Two markets with the same question but different expiry dates
            // are NOT the same event — trading them as an arb pair guarantees losses.
            let expirations: Vec<DateTime<Utc>> = group.iter().map(|dm| dm.expiration).collect();
            let min_exp = expirations.iter().min().copied().unwrap_or(first.expiration);
            let max_exp = expirations.iter().max().copied().unwrap_or(first.expiration);
            let exp_diff_hours = (max_exp - min_exp).num_hours().abs();
            if exp_diff_hours > 48 {
                warn!(
                    question = %first.question,
                    exp_diff_hours,
                    "Skipping cross-platform match — expiration mismatch ({} hours apart)",
                    exp_diff_hours
                );
                continue;
            }

            // Scale confidence by how close the expirations are
            let confidence = if exp_diff_hours == 0 { 0.98 } else { 0.95 };

            let market = Market {
                unified_id,
                question: first.question.clone(),
                resolution_source: first.resolution_source.clone(),
                expiration: first.expiration,
                platforms: platform_infos,
                category: first.category,
                confidence,
                status: MarketStatus::Active,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            matched.push(MatchedMarket { market });
        }

        Ok(matched)
    }

    fn normalize_question(q: &str) -> String {
        q.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ")
    }

    async fn fetch_polymarket_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        // Polymarket CLOB API: GET /markets
        let url = format!("{}/markets", self.platforms_config.polymarket.rest_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("active", "true"), ("limit", "100")])
            .send()
            .await
            .context("Polymarket markets fetch failed")?
            .json()
            .await
            .context("Polymarket markets parse failed")?;

        let mut markets = Vec::new();
        if let Some(arr) = resp.as_array() {
            for item in arr {
                let question = item
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if question.is_empty() {
                    continue;
                }
                let token_id = item
                    .get("condition_id")
                    .or_else(|| item.get("token_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if token_id.is_empty() {
                    continue;
                }
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
        Ok(markets)
    }

    async fn fetch_kalshi_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let url = format!("{}/markets", self.platforms_config.kalshi.rest_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("status", "open"), ("limit", "100")])
            .send()
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
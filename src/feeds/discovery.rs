// src/feeds/discovery.rs
//
// KEY FIXES IN THIS VERSION:
//
// FIX-1 (CRITICAL): 15-minute round rollover — when a new 15m window starts,
//        discovery must find the new market and upsert it. Previously the
//        confidence=1.0 match meant the same unified_id was reused across rounds
//        (because compute_unified_market_id hashes the slug which includes the
//        timestamp). That was correct, but the DB upsert was NOT updating the
//        platforms map with the new token pair / ticker for the new round.
//        Fixed: always upsert with updated platform info.
//
// FIX-2 (CRITICAL): Stale/resolved round cleanup — after a 15m round ends,
//        the discovery loop must mark those DB markets as Resolved so the
//        feed handlers unsubscribe and the MarketRegistry evicts them.
//        Previously resolved rounds stayed Active forever.
//
// FIX-3 (CRITICAL): Discovery poll interval reduced to 15s (from 30s).
//        15-minute markets only exist for 900 seconds. A 30s poll means
//        up to 2 full poll cycles wasted before the new market is discovered.
//
// FIX-4 (HIGH): The reference TS project computes the slug as
//        "{asset}-updown-15m-{roundedTimestamp}" where roundedTimestamp
//        is Math.floor(currentTime / 900) * 900. We now try current + ±1
//        offsets and pick the first active one.
//
// FIX-5 (HIGH): Token ID parsing now correctly handles the clobTokenIds
//        field from the Gamma event endpoint.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::PlatformsConfig;
use crate::feeds::normalizer::compute_unified_market_id;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct MatchedMarket {
    pub market: Market,
}

// ─── Internal structs ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CryptoRound {
    round_start_ts: i64,
    asset: String,
    poly_token_pair: Option<String>,
    poly_question: Option<String>,
    poly_expiration: Option<DateTime<Utc>>,
    kalshi_ticker: Option<String>,
    kalshi_question: Option<String>,
    kalshi_expiration: Option<DateTime<Utc>>,
    kalshi_fee_bps: u16,
}

#[derive(Debug, Clone)]
struct DiscoveredMarket {
    platform: Platform,
    platform_market_id: String,
    question: String,
    question_normalized: String,
    expiration: DateTime<Utc>,
    category: MarketCategory,
    fee_rate_bps: u16,
    min_order_size: Decimal,
    tick_size: Decimal,
}

// ─── Discovery entry point ─────────────────────────────────────────────────

pub struct MarketDiscovery {
    platforms_config: PlatformsConfig,
    http: reqwest::Client,
    poll_interval: Duration,
    kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>,
}

impl MarketDiscovery {
    pub fn new(
        platforms_config: PlatformsConfig,
        poll_interval_secs: u64,
        kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("Mozilla/5.0 (compatible; MERCURY/0.1)")
            .build()
            .expect("failed to build discovery HTTP client");
        Self {
            platforms_config,
            http,
            poll_interval: Duration::from_secs(poll_interval_secs),
            kalshi_auth,
        }
    }

    pub async fn run(self, matched_tx: mpsc::Sender<MatchedMarket>) {
        info!(
            interval_secs = self.poll_interval.as_secs(),
            "Market discovery started"
        );
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now(), self.poll_interval);

        loop {
            ticker.tick().await;
            match self.discover_and_match().await {
                Ok(matched) => {
                    let active = matched.iter().filter(|m| m.market.status == MarketStatus::Active).count();
                    info!(total = matched.len(), active, "Discovery cycle complete");
                    for m in matched {
                        if let Err(e) = matched_tx.send(m).await {
                            warn!(error = %e, "Matched market channel closed");
                        }
                    }
                }
                Err(e) => error!(error = %e, "Market discovery cycle failed"),
            }
        }
    }

    async fn discover_and_match(&self) -> Result<Vec<MatchedMarket>> {
        let mut results: Vec<MatchedMarket> = Vec::new();

        let crypto_assets = ["btc", "eth", "sol", "xrp"];
        let rounds = self.build_round_candidates(&crypto_assets);

        let poly_enabled = self.platforms_config.polymarket.enabled;
        let kalshi_enabled = self.platforms_config.kalshi.enabled;

        for mut round in rounds {
            if poly_enabled {
                if let Some(pair) = self.fetch_poly_event_tokens(&round.asset, round.round_start_ts).await {
                    round.poly_token_pair = Some(pair.0);
                    round.poly_question = Some(pair.1);
                    round.poly_expiration = Some(pair.2);
                }
            }

            if kalshi_enabled {
                if let Some(km) = self.fetch_kalshi_15m_market(&round.asset, round.round_start_ts).await {
                    round.kalshi_ticker = Some(km.0);
                    round.kalshi_question = Some(km.1);
                    round.kalshi_expiration = Some(km.2);
                    round.kalshi_fee_bps = km.3;
                }
            }

            if round.poly_token_pair.is_none() && round.kalshi_ticker.is_none() {
                continue;
            }

            if let Some(m) = self.build_crypto_matched_market(&round) {
                let now_ts = Utc::now().timestamp();
                let round_end = round.round_start_ts + 900;
                // Mark as resolved if the round ended more than 30 seconds ago
                // (give 30s grace for settlement messages to arrive)
                let status = if round_end < now_ts - 30 {
                    MarketStatus::Resolved
                } else {
                    MarketStatus::Active
                };

                let mut market_with_status = m.market.clone();
                market_with_status.status = status;

                info!(
                    asset = %round.asset,
                    round_start = round.round_start_ts,
                    round_end,
                    status = ?market_with_status.status,
                    poly = round.poly_token_pair.is_some(),
                    kalshi = round.kalshi_ticker.is_some(),
                    "15m crypto round matched"
                );
                results.push(MatchedMarket { market: market_with_status });
            }
        }

        // General Jaccard-based matching for sports/politics/finance
        if poly_enabled && kalshi_enabled {
            match self.fetch_general_markets().await {
                Ok((poly_markets, kalshi_markets)) => {
                    let general =
                        tokio::task::spawn_blocking(move || {
                            Self::match_general_markets_sync(poly_markets, kalshi_markets)
                        })
                        .await??;
                    results.extend(general);
                }
                Err(e) => warn!(error = %e, "Failed to fetch general markets for NLP matching"),
            }
        }

        Ok(results)
    }

    // ─── 15-minute crypto: build candidate round timestamps ──────────────

    fn build_round_candidates(&self, assets: &[&str]) -> Vec<CryptoRound> {
        let now_ts = Utc::now().timestamp();
        let round_secs: i64 = 900;

        let current_round = (now_ts / round_secs) * round_secs;

        // We care about: previous round (may need cleanup), current round, next round
        // This ensures we discover the new market as soon as the round boundary crosses
        let offsets: [i64; 3] = [-round_secs, 0, round_secs];

        let mut rounds = Vec::new();
        for asset in assets {
            for &offset in &offsets {
                let round_start = current_round + offset;
                let round_end = round_start + round_secs;

                // Skip rounds that ended more than 5 minutes ago (already cleaned up)
                if round_end < now_ts - 300 {
                    continue;
                }
                // Skip rounds that start more than 1 round in the future
                if round_start > now_ts + round_secs {
                    continue;
                }

                rounds.push(CryptoRound {
                    round_start_ts: round_start,
                    asset: asset.to_string(),
                    poly_token_pair: None,
                    poly_question: None,
                    poly_expiration: None,
                    kalshi_ticker: None,
                    kalshi_question: None,
                    kalshi_expiration: None,
                    kalshi_fee_bps: 5,
                });
            }
        }
        rounds
    }

    // ─── Polymarket: fetch event by slug ─────────────────────────────────

    async fn fetch_poly_event_tokens(
        &self,
        asset: &str,
        round_start_ts: i64,
    ) -> Option<(String, String, DateTime<Utc>)> {
        let slug = format!("{}-updown-15m-{}", asset, round_start_ts);
        let url = format!("https://gamma-api.polymarket.com/events/slug/{}", slug);

        let resp = match tokio::time::timeout(
            Duration::from_secs(8),
            self.http.get(&url).send(),
        ).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::debug!(error = %e, slug, "Polymarket event fetch failed");
                return None;
            }
            Err(_) => {
                tracing::debug!(slug, "Polymarket event fetch timed out");
                return None;
            }
        };

        if !resp.status().is_success() {
            tracing::debug!(status = %resp.status(), slug, "Polymarket event not found");
            return None;
        }

        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                tracing::debug!(error = %e, "Failed to parse Polymarket event JSON");
                return None;
            }
        };

        let markets = json.get("markets").and_then(|m| m.as_array())?;
        if markets.is_empty() {
            return None;
        }

        let mut up_token: Option<String> = None;
        let mut down_token: Option<String> = None;
        let mut expiration: Option<DateTime<Utc>> = None;

        for market in markets {
            let outcome = market
                .get("groupItemTitle")
                .or_else(|| market.get("question"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();

            let token_id = parse_first_clob_token_id(market);
            if token_id.is_empty() {
                continue;
            }

            if outcome.contains("up") || outcome.contains("higher") || outcome.contains("above") || outcome.contains("yes") {
                up_token = Some(token_id);
            } else if outcome.contains("down") || outcome.contains("lower") || outcome.contains("below") || outcome.contains("no") {
                down_token = Some(token_id);
            }

            if expiration.is_none() {
                expiration = parse_expiration_from_market(market);
            }
        }

        // Fallback: if we only have one market, parse both token IDs from it
        if up_token.is_none() || down_token.is_none() {
            let first_market = &markets[0];
            let (yes, no) = parse_two_clob_token_ids(first_market);
            if !yes.is_empty() { up_token = Some(yes); }
            if !no.is_empty() { down_token = Some(no); }
        }

        let up = up_token?;
        let down = down_token?;
        let token_pair = format!("{},{}", up, down);

        let exp = expiration.unwrap_or_else(|| {
            chrono::DateTime::from_timestamp(round_start_ts + 900, 0).unwrap_or_else(Utc::now)
        });

        let question = json
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Bitcoin Up or Down - 15 Minutes")
            .to_string();

        Some((token_pair, question, exp))
    }

    // ─── Kalshi: find the 15-minute market matching this round ───────────

    async fn fetch_kalshi_15m_market(
        &self,
        asset: &str,
        round_start_ts: i64,
    ) -> Option<(String, String, DateTime<Utc>, u16)> {
        let series_prefix = match asset {
            "btc" => "KXBTC15M",
            "eth" => "KXETH15M",
            "sol" => "KXSOL15M",
            "xrp" => "KXXRP15M",
            _ => return None,
        };

        // Fetch only open markets. limit=5 is sufficient since 15m markets roll
        // every 900s and we only care about the current and next rounds.
        let url = format!(
            "https://api.elections.kalshi.com/trade-api/v2/markets?series_ticker={}&status=open&limit=5",
            series_prefix
        );

        let auth_token = self
            .kalshi_auth
            .as_ref()
            .and_then(|a| a.generate_token().ok());

        let mut req = self.http.get(&url);
        if let Some(token) = auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = match tokio::time::timeout(Duration::from_secs(10), req.send()).await {
            Ok(Ok(r)) => r,
            _ => return None,
        };

        if !resp.status().is_success() {
            return None;
        }

        let data: serde_json::Value = match resp.json().await {
            Ok(d) => d,
            Err(_) => return None,
        };

        let markets = data.get("markets").and_then(|v| v.as_array())?;

        let target_close_ts = round_start_ts + 900;
        // 300s slack: covers late discovery cycles and minor clock skew between
        // MERCURY and Kalshi servers. Safe because 15m markets are 900s long —
        // a 300s window cannot accidentally match the wrong round.
        let slack: i64 = 300;

        for market in markets {
            let close_time_str = market.get("close_time").and_then(|v| v.as_str())?;
            let close_dt = DateTime::parse_from_rfc3339(close_time_str)
                .ok()
                .map(|d| d.with_timezone(&Utc))?;
            let close_ts = close_dt.timestamp();

            if (close_ts - target_close_ts).abs() <= slack {
                let ticker = market.get("ticker").and_then(|v| v.as_str())?.to_string();
                let title = market
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("BTC 15m")
                    .to_string();
                let fee_bps = 50u16;
                return Some((ticker, title, close_dt, fee_bps));
            }
        }

        tracing::debug!(asset, round_start_ts, "No Kalshi 15m market found for this round");
        None
    }

    // ─── Build MatchedMarket from a completed CryptoRound ────────────────

    fn build_crypto_matched_market(&self, round: &CryptoRound) -> Option<MatchedMarket> {
        let mut platforms: HashMap<Platform, PlatformMarketInfo> = HashMap::new();

        if let Some(ref token_pair) = round.poly_token_pair {
            platforms.insert(
                Platform::Polymarket,
                PlatformMarketInfo {
                    platform: Platform::Polymarket,
                    platform_market_id: token_pair.clone(),
                    fee_rate_bps: 200,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2),
                },
            );
        }

        if let Some(ref ticker) = round.kalshi_ticker {
            platforms.insert(
                Platform::Kalshi,
                PlatformMarketInfo {
                    platform: Platform::Kalshi,
                    platform_market_id: ticker.clone(),
                    fee_rate_bps: round.kalshi_fee_bps,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2),
                },
            );
        }

        if platforms.is_empty() {
            return None;
        }

        let question = match (&round.poly_question, &round.kalshi_question) {
            (Some(p), Some(k)) => format!("{} / {}", p, k),
            (Some(p), None) => p.clone(),
            (None, Some(k)) => k.clone(),
            (None, None) => format!("{} Up or Down - 15 Minutes ({})", round.asset.to_uppercase(), round.round_start_ts),
        };

        let expiration = round
            .kalshi_expiration
            .or(round.poly_expiration)
            .unwrap_or_else(|| {
                chrono::DateTime::from_timestamp(round.round_start_ts + 900, 0)
                    .unwrap_or_else(Utc::now)
            });

        let unified_id = compute_unified_market_id(
            &format!("{}-updown-15m-{}", round.asset, round.round_start_ts),
            "cross_platform_crypto_15m",
            &(round.round_start_ts + 900).to_string(),
        );

        Some(MatchedMarket {
            market: Market {
                unified_id,
                question,
                resolution_source: "cross_platform_crypto_15m".into(),
                expiration,
                platforms,
                category: MarketCategory::Crypto,
                confidence: 1.0,
                status: MarketStatus::Active,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        })
    }

    // ─── General market fetching ──────────────────────────────────────────

    async fn fetch_general_markets(&self) -> Result<(Vec<DiscoveredMarket>, Vec<DiscoveredMarket>)> {
        let poly_fut = self.fetch_polymarket_general_markets();
        let kalshi_fut = self.fetch_kalshi_general_markets();
        let (poly_result, kalshi_result) = tokio::join!(poly_fut, kalshi_fut);
        Ok((poly_result.unwrap_or_default(), kalshi_result.unwrap_or_default()))
    }

    async fn fetch_polymarket_general_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let mut markets = Vec::new();
        let gamma_url = "https://gamma-api.polymarket.com/markets";
        let now = Utc::now();
        let week_out = now + chrono::Duration::days(7);
        let end_max = week_out.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let resp = match tokio::time::timeout(
            Duration::from_secs(12),
            self.http
                .get(gamma_url)
                .query(&[
                    ("active", "true"),
                    ("closed", "false"),
                    ("acceptingOrders", "true"),
                    ("limit", "200"),
                    ("end_date_max", end_max.as_str()),
                ])
                .send(),
        ).await {
            Ok(Ok(r)) if r.status().is_success() => r,
            _ => return Ok(markets),
        };

        let data: serde_json::Value = resp.json().await?;
        let arr = if let Some(a) = data.as_array() {
            a.clone()
        } else if let Some(a) = data.get("data").and_then(|v| v.as_array()) {
            a.clone()
        } else {
            return Ok(markets);
        };

        for item in &arr {
            let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if slug.contains("-updown-15m-") || slug.contains("-updown-") {
                continue;
            }

            let question = item.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if question.is_empty() { continue; }

            let (yes, no) = parse_two_clob_token_ids(item);
            if yes.is_empty() || no.is_empty() { continue; }
            let token_pair = format!("{},{}", yes, no);

            let expiration = parse_expiration_from_market(item).unwrap_or_else(|| now + chrono::Duration::hours(24));
            if expiration <= now { continue; }

            let q_norm = Self::normalize_question(&question);
            let category = categorize_question(&q_norm);

            markets.push(DiscoveredMarket {
                platform: Platform::Polymarket,
                platform_market_id: token_pair,
                question_normalized: q_norm,
                question,
                expiration,
                category,
                fee_rate_bps: 200,
                min_order_size: Decimal::ONE,
                tick_size: Decimal::new(1, 2),
            });
        }

        info!(count = markets.len(), "Polymarket general markets fetched");
        Ok(markets)
    }

    async fn fetch_kalshi_general_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let url = format!("{}/markets?status=open&limit=500", self.platforms_config.kalshi.rest_url);
        let auth_token = self.kalshi_auth.as_ref().and_then(|a| a.generate_token().ok());
        let mut req = self.http.get(&url);
        if let Some(token) = auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = match tokio::time::timeout(Duration::from_secs(12), req.send()).await {
            Ok(Ok(r)) if r.status().is_success() => r,
            _ => return Ok(vec![]),
        };

        let data: serde_json::Value = resp.json().await?;
        let arr = match data.get("markets").and_then(|v| v.as_array()) {
            Some(a) => a.clone(),
            None => return Ok(vec![]),
        };

        let now = Utc::now();
        let mut markets = Vec::new();

        for item in &arr {
            let ticker = match item.get("ticker").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => continue,
            };

            if ticker.contains("15M") {
                continue;
            }

            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }

            let close_time = item
                .get("close_time")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|| now + chrono::Duration::days(30));

            if close_time <= now { continue; }

            let q_norm = Self::normalize_question(&title);
            let category = categorize_question(&q_norm);

            markets.push(DiscoveredMarket {
                platform: Platform::Kalshi,
                platform_market_id: ticker,
                question_normalized: q_norm,
                question: title,
                expiration: close_time,
                category,
                fee_rate_bps: 175,
                min_order_size: Decimal::ONE,
                tick_size: Decimal::new(1, 2),
            });
        }

        info!(count = markets.len(), "Kalshi general markets fetched");
        Ok(markets)
    }

    fn match_general_markets_sync(
        poly_markets: Vec<DiscoveredMarket>,
        kalshi_markets: Vec<DiscoveredMarket>,
    ) -> Result<Vec<MatchedMarket>> {
        let stop_words: std::collections::HashSet<&str> = [
            "will", "the", "a", "an", "in", "on", "at", "by", "for", "to",
            "of", "be", "is", "are", "was", "were", "has", "have", "had",
            "do", "does", "did", "not", "or", "and", "if", "it", "its",
            "this", "that", "with", "from", "as", "so", "can", "may", "per",
            "vs", "end", "close", "open", "day", "week", "month", "year",
        ].iter().cloned().collect();

        let mut matched: Vec<MatchedMarket> = Vec::new();
        let mut seen_pairs = std::collections::HashSet::new();
        let now = Utc::now();

        for pm in &poly_markets {
            for km in &kalshi_markets {
                let exp_diff = (pm.expiration - km.expiration).num_seconds().abs();
                if exp_diff > 48 * 3600 { continue; }
                if pm.category != km.category { continue; }

                let tokens_a: std::collections::HashSet<&str> = pm.question_normalized.split_whitespace()
                    .filter(|w| !stop_words.contains(*w)).collect();
                let tokens_b: std::collections::HashSet<&str> = km.question_normalized.split_whitespace()
                    .filter(|w| !stop_words.contains(*w)).collect();
                let intersection = tokens_a.intersection(&tokens_b).count();
                let union = tokens_a.union(&tokens_b).count();
                let sim = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };

                if sim < 0.40 { continue; }

                let pair_key = format!("{}/{}", pm.platform_market_id, km.platform_market_id);
                if !seen_pairs.insert(pair_key) { continue; }

                let mut platforms = HashMap::new();
                platforms.insert(pm.platform, PlatformMarketInfo {
                    platform: pm.platform,
                    platform_market_id: pm.platform_market_id.clone(),
                    fee_rate_bps: pm.fee_rate_bps,
                    min_order_size: pm.min_order_size,
                    tick_size: pm.tick_size,
                });
                platforms.insert(km.platform, PlatformMarketInfo {
                    platform: km.platform,
                    platform_market_id: km.platform_market_id.clone(),
                    fee_rate_bps: km.fee_rate_bps,
                    min_order_size: km.min_order_size,
                    tick_size: km.tick_size,
                });

                let expiration = pm.expiration.min(km.expiration);
                let unified_id = compute_unified_market_id(
                    &pm.question,
                    "cross_platform",
                    &expiration.to_rfc3339(),
                );

                let confidence = if sim >= 0.70 { 0.99 } else if sim >= 0.55 { 0.97 } else { 0.95 };

                matched.push(MatchedMarket {
                    market: Market {
                        unified_id,
                        question: format!("{} / {}", pm.question, km.question),
                        resolution_source: "cross_platform".into(),
                        expiration,
                        platforms,
                        category: pm.category,
                        confidence,
                        status: MarketStatus::Active,
                        created_at: now,
                        updated_at: now,
                    },
                });
            }
        }

        if !matched.is_empty() {
            info!(count = matched.len(), "General NLP-matched markets");
        }
        Ok(matched)
    }

    pub(super) fn normalize_question(q: &str) -> String {
        let mut lower = q.to_lowercase().replace(',', "").replace('$', "");
        let replacements = [
            ("bitcoin", "btc"), ("ethereum", "eth"), ("solana", "sol"),
            ("ripple", "xrp"), ("dogecoin", "doge"),
            ("minutes", "min"), ("minute", "min"),
        ];
        for (from, to) in replacements {
            if lower.contains(from) { lower = lower.replace(from, to); }
        }
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
        if result.ends_with(' ') { result.pop(); }
        result
    }
}

// ─── Token ID parsing helpers ─────────────────────────────────────────────

fn parse_first_clob_token_id(item: &serde_json::Value) -> String {
    if let Some(raw) = item.get("clobTokenIds").and_then(|v| v.as_str()) {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(raw) {
            if let Some(id) = ids.into_iter().next() { return id; }
        }
    }
    if let Some(arr) = item.get("clobTokenIds").and_then(|v| v.as_array()) {
        if let Some(id) = arr.first().and_then(|v| v.as_str()) { return id.to_string(); }
    }
    if let Some(tokens) = item.get("tokens").and_then(|v| v.as_array()) {
        if let Some(id) = tokens.first().and_then(|t| t.get("token_id")).and_then(|v| v.as_str()) {
            return id.to_string();
        }
    }
    String::new()
}

fn parse_two_clob_token_ids(item: &serde_json::Value) -> (String, String) {
    if let Some(raw) = item.get("clobTokenIds").and_then(|v| v.as_str()) {
        let to_parse = raw.trim().trim_matches('"');
        let unescaped = if to_parse.contains("\\\"") { to_parse.replace("\\\"", "\"") } else { to_parse.to_string() };
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&unescaped) {
            let yes = ids.first().cloned().unwrap_or_default();
            let no = ids.get(1).cloned().unwrap_or_default();
            if !yes.is_empty() && !no.is_empty() { return (yes, no); }
        }
    }
    if let Some(arr) = item.get("clobTokenIds").and_then(|v| v.as_array()) {
        let yes = arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
        let no = arr.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !yes.is_empty() && !no.is_empty() { return (yes, no); }
    }
    if let Some(tokens) = item.get("tokens").and_then(|v| v.as_array()) {
        let yes = tokens.first().and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let no = tokens.get(1).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        return (yes, no);
    }
    (String::new(), String::new())
}

fn parse_expiration_from_market(item: &serde_json::Value) -> Option<DateTime<Utc>> {
    for field in &["endDateIso", "endDate", "end_date_iso", "closeTime", "end_date"] {
        if let Some(s) = item.get(field).and_then(|v| v.as_str()) {
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) { return Some(dt.with_timezone(&Utc)); }
            if let Ok(ts) = s.parse::<i64>() {
                let secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
                if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, secs, 0) {
                    return Some(dt);
                }
            }
        }
        if let Some(ts) = item.get(field).and_then(|v| v.as_i64()) {
            let secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
            if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, secs, 0) {
                return Some(dt);
            }
        }
    }
    None
}

fn categorize_question(q_norm: &str) -> MarketCategory {
    if q_norm.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "ada" | "crypto")) {
        return MarketCategory::Crypto;
    }
    if q_norm.contains("election") || q_norm.contains("president") || q_norm.contains("senate") || q_norm.contains("congress") {
        return MarketCategory::Politics;
    }
    if q_norm.contains("nba") || q_norm.contains("nfl") || q_norm.contains("mlb") || q_norm.contains("nhl") || q_norm.contains("game") || q_norm.contains("championship") {
        return MarketCategory::Sports;
    }
    if q_norm.contains("fed") || q_norm.contains("rate") || q_norm.contains("gdp") || q_norm.contains("inflation") {
        return MarketCategory::Finance;
    }
    MarketCategory::Other
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod discovery_tests;
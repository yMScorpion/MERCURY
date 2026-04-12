// src/feeds/discovery.rs  — KEY CHANGES vs original
//
// FIX-1  Polymarket 15-min slug timestamp is the ROUND START, NOT the round end.
//        Expiration = slug_ts + 900s. Original code did this correctly only when
//        it found the slug field. But the `acceptingOrders` filter was discarding
//        all 15-min markets because they stop accepting orders before the round ends.
//        FIX: always allow 15-min crypto markets regardless of acceptingOrders.
//
// FIX-2  The Gamma API field for token IDs is `clobTokenIds` (a JSON-string of an
//        array like '["yes_id","no_id"]').  When parsing we must json-parse that
//        string.  The original code attempted this but had a fallback bug: it used
//        `m.tokens` which for events is an array of market objects, not token objects.
//        The fix makes the clobTokenIds parsing robust and skips markets with no tokens.
//
// FIX-3  Kalshi BTC 15m series is `KXBTC15M` — confirmed. But the market tickers
//        look like `KXBTC15M-26APR12T134500-134500` — the subscription must send
//        the FULL TICKER, not just the series prefix.  This was correct in original
//        but the REST fetch only got 200 markets. Increased to fetch more.
//
// FIX-4  Confidence threshold in match_markets_sync for 15-min crypto was correct
//        (forces sim=1.0 and min_sim=0.01) but the exp_diff_secs check used
//        pm_expiration which relied on the slug timestamp parsing. When slug parsing
//        fails the fallback was `now + 30 days`, giving exp_diff > 120s and rejecting
//        all 15-min pairs.  Fix: detect slug timestamps precisely.
//
// FIX-5  The `stale_data_timeout_ms` guard in detector.rs (Gate 3) was comparing
//        book age (time since last tick) vs `stale_data_timeout_ms` (5000ms default).
//        A fresh subscription with no ticks yet has age=now_ns (very large).
//        Fix: bypass stale check for the first 60s after subscription.
//        (This fix is in detector.rs, documented here for reference.)

use anyhow::Result;
use chrono::{DateTime, Utc, Timelike};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use regex::Regex;

fn time_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(\d{1,2}):(\d{2})\s*(am|pm|a\.m\.|p\.m\.)?\s*(et|est|edt|brt|utc)?").unwrap())
}

use crate::config::PlatformsConfig;
use crate::feeds::normalizer::compute_unified_market_id;
use crate::types::*;

#[derive(Debug, Clone)]
struct DiscoveredMarket {
    platform: Platform,
    platform_market_id: String,
    question: String,
    question_normalized: String,
    resolution_source: String,
    expiration: DateTime<Utc>,
    category: MarketCategory,
    fee_rate_bps: u16,
    min_order_size: Decimal,
    tick_size: Decimal,
}

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
    fn extract_markets_array(value: &serde_json::Value) -> &[serde_json::Value] {
        if let Some(arr) = value.as_array() { arr.as_slice() }
        else if let Some(arr) = value.get("data").and_then(|d| d.as_array()) { arr.as_slice() }
        else { &[] }
    }

    pub fn new(platforms_config: PlatformsConfig, poll_interval_secs: u64, kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>) -> Self {
        let http = reqwest::Client::builder()
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .build()
            .expect("failed to build discovery HTTP client");
        Self { platforms_config, http, poll_interval: Duration::from_secs(poll_interval_secs), kalshi_auth }
    }

    pub async fn run(self, matched_tx: mpsc::Sender<MatchedMarket>) {
        info!(interval_secs = self.poll_interval.as_secs(), "Market discovery started");
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now(), self.poll_interval);

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

    fn try_fix_expiration_from_title(title: &str, base_date: DateTime<Utc>) -> DateTime<Utc> {
        let lower = title.to_lowercase();
        if let Some(caps) = time_regex().captures_iter(&lower).last() {
            let mut hour: u32 = caps.get(1).map_or(0, |m| m.as_str().parse().unwrap_or(0));
            let min: u32 = caps.get(2).map_or(0, |m| m.as_str().parse().unwrap_or(0));
            let ampm = caps.get(3).map(|m| m.as_str().replace(".", ""));
            let tz = caps.get(4).map(|m| m.as_str());
            if let Some(ampm_str) = ampm {
                if ampm_str == "pm" && hour < 12 { hour += 12; }
                else if ampm_str == "am" && hour == 12 { hour = 0; }
            }
            if hour < 24 && min < 60 {
                if let Some(mut new_date) = base_date.with_hour(hour).and_then(|d| d.with_minute(min)) {
                    if let Some(tz_str) = tz {
                        if tz_str == "et" || tz_str == "est" || tz_str == "edt" { new_date += chrono::Duration::hours(4); }
                        else if tz_str == "brt" { new_date += chrono::Duration::hours(3); }
                    } else if lower.contains("et") || lower.contains("est") || lower.contains("edt") {
                        new_date += chrono::Duration::hours(4);
                    }
                    let diff = (new_date - base_date).num_hours();
                    if diff > 12 { new_date -= chrono::Duration::days(1); }
                    else if diff < -12 { new_date += chrono::Duration::days(1); }
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

        let matched = tokio::task::spawn_blocking(move || {
            Self::match_markets_sync(poly_markets, kalshi_markets)
        }).await??;

        Ok(matched)
    }

    fn match_markets_sync(
        poly_markets: Vec<DiscoveredMarket>,
        kalshi_markets: Vec<DiscoveredMarket>,
    ) -> Result<Vec<MatchedMarket>> {
        let stop_words: std::collections::HashSet<&str> = [
            "will", "the", "a", "an", "in", "on", "at", "by", "for", "to",
            "of", "be", "is", "are", "was", "were", "has", "have", "had",
            "do", "does", "did", "not", "or", "and", "if", "it", "its",
            "this", "that", "with", "from", "as", "so", "can", "may", "per",
            "vs", "end", "close", "open", "day", "week", "month", "year",
            "next", "last", "new", "more", "less", "most", "least", "than",
            "which", "who", "what", "when", "how", "between",
        ].iter().cloned().collect();

        let mut word_to_kalshi: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, km) in kalshi_markets.iter().enumerate() {
            for word in km.question_normalized.split_whitespace() {
                if !stop_words.contains(word) {
                    word_to_kalshi.entry(word.to_string()).or_default().push(i);
                }
            }
        }

        let kalshi_tokens: Vec<std::collections::HashSet<String>> = kalshi_markets.iter()
            .map(|m| m.question_normalized.split_whitespace()
                .filter(|w| !stop_words.contains(*w))
                .map(|w| w.to_string())
                .collect())
            .collect();

        let mut kalshi_by_hour: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, km) in kalshi_markets.iter().enumerate() {
            let hour = km.expiration.timestamp() / 3600;
            kalshi_by_hour.entry(hour).or_default().push(i);
        }

        // Diagnostics
        info!("=== DISCOVERY DIAGNOSTICS ===");
        let poly_15m: Vec<_> = poly_markets.iter()
            .filter(|m| m.platform_market_id.contains(',') &&
                (m.question_normalized.contains("15") || m.question_normalized.contains("btc")))
            .collect();
        info!("Polymarket 15-min/BTC candidates: {}", poly_15m.len());
        for pm in poly_15m.iter().take(5) {
            info!("  POLY: {} | exp={} | id_prefix={}", pm.question_normalized, pm.expiration, &pm.platform_market_id[..20.min(pm.platform_market_id.len())]);
        }

        let k_15m: Vec<_> = kalshi_markets.iter()
            .filter(|m| m.platform_market_id.contains("15M") || m.platform_market_id.contains("BTC"))
            .collect();
        info!("Kalshi BTC/15M candidates: {}", k_15m.len());
        for km in k_15m.iter().take(5) {
            info!("  KALSHI: {} | exp={} | ticker={}", km.question_normalized, km.expiration, km.platform_market_id);
        }
        info!("Total Kalshi markets: {} | Total Polymarket: {}", kalshi_markets.len(), poly_markets.len());

        let mut matched: Vec<MatchedMarket> = Vec::new();
        let mut seen_pairs = std::collections::HashSet::new();

        for pm in poly_markets.iter() {
            let mut pm_expiration = pm.expiration;
            if pm_expiration.hour() == 0 && pm_expiration.minute() == 0 {
                pm_expiration = Self::try_fix_expiration_from_title(&pm.question, pm_expiration);
            } else if pm.platform_market_id.contains("-15m-") || pm.platform_market_id.contains("-updown-15m-") {
                // Parse timestamp from Polymarket slug format "btc-updown-15m-TIMESTAMP"
                // Extract timestamp from end of slug
                if let Some(last_dash) = pm.platform_market_id.rfind('-') {
                    if let Ok(ts) = pm.platform_market_id[last_dash + 1..].parse::<i64>() {
                        // CRITICAL FIX: Convert millisecond timestamp to seconds
                        let ts_secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
                        if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, ts_secs + 900, 0) {
                            pm_expiration = dt;
                        }
                    }
                }
            }
            let pm_hour = pm_expiration.timestamp() / 3600;

            let mut candidate_set: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for hour_offset in -2i64..=2 {
                if let Some(idxs) = kalshi_by_hour.get(&(pm_hour + hour_offset)) {
                    candidate_set.extend(idxs);
                }
            }
            for word in pm.question_normalized.split_whitespace() {
                if !stop_words.contains(word) {
                    if let Some(idxs) = word_to_kalshi.get(word) {
                        candidate_set.extend(idxs);
                    }
                }
            }

            for ki in candidate_set {
                let km = &kalshi_markets[ki];
                let exp_diff_secs = (pm_expiration - km.expiration).num_seconds().abs();

                let extract_crypto = |q: &str| -> std::collections::HashSet<String> {
                    q.split_whitespace()
                        .filter(|w| matches!(*w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "ada" | "crypto"))
                        .map(|w| w.to_string())
                        .collect()
                };
                let crypto_pm = extract_crypto(&pm.question_normalized);
                let crypto_km = extract_crypto(&km.question_normalized);
                let shared_crypto = !crypto_pm.is_empty() && !crypto_km.is_empty()
                    && crypto_pm.intersection(&crypto_km).count() > 0;

                let is_15m_market = pm.question_normalized.contains("15 min")
                    || km.question_normalized.contains("15 min")
                    || km.platform_market_id.contains("15M")
                    || pm.question_normalized.split_whitespace().any(|w| w == "15m")
                    || km.question_normalized.split_whitespace().any(|w| w == "15m")
                    // FIX-4: detect from Poly slug format "btc-updown-15m-TIMESTAMP"
                    || pm.platform_market_id.contains(",")  // has tokens = is a binary market
                       && (pm.question_normalized.contains("up or down") || pm.question_normalized.contains("updown"));

                if is_15m_market {
                    // FIX-4: 15-min markets: use 180s window (was 120s) to handle clock skew
                    if exp_diff_secs > 180 { continue; }
                    if shared_crypto || (pm.category == MarketCategory::Crypto && km.category == MarketCategory::Crypto) {
                        info!(
                            "15M CRYPTO MATCH CANDIDATE: poly='{}' exp={} | kalshi='{}' ticker={} exp={} | diff={}s",
                            pm.question_normalized, pm_expiration,
                            km.question_normalized, km.platform_market_id, km.expiration,
                            exp_diff_secs
                        );
                    }
                } else if pm.category == MarketCategory::Crypto || km.category == MarketCategory::Crypto {
                    if exp_diff_secs > 7 * 24 * 3600 { continue; }
                } else if exp_diff_secs > 48 * 3600 {
                    continue;
                }

                let sim;
                let same_category = pm.category == km.category;

                if is_15m_market && shared_crypto && exp_diff_secs <= 180 {
                    info!("FORCING 15M CRYPTO MATCH: poly='{}' ↔ kalshi='{}'", pm.question, km.question);
                    sim = 1.0;
                } else {
                    let tokens_a: std::collections::HashSet<&str> = pm.question_normalized
                        .split_whitespace().filter(|w| !stop_words.contains(w)).collect();
                    let tokens_b = &kalshi_tokens[ki];
                    let intersection = tokens_a.iter().filter(|w| tokens_b.contains(**w)).count();
                    let union = tokens_a.len() + tokens_b.len() - intersection;
                    sim = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };
                }

                let min_sim = if is_15m_market && shared_crypto { 0.01 }
                    else if same_category && exp_diff_secs <= 24 * 3600 { 0.35 }
                    else if same_category && exp_diff_secs <= 7 * 24 * 3600 { 0.40 }
                    else { 0.50 };

                if sim < min_sim { continue; }

                if !is_15m_market {
                    let extract_nums = |q: &str| -> std::collections::HashSet<i64> {
                        q.split_whitespace().filter_map(|w| {
                            let mut multiplier = 1.0;
                            let mut cleaned = w.replace(['$', ',', '%', '?'], "");
                            if cleaned.ends_with('k') || cleaned.ends_with('K') {
                                multiplier = 1000.0; cleaned.pop();
                            }
                            if let Ok(f) = cleaned.parse::<f64>() {
                                if f >= 2020.0 && f <= 2030.0 && f.fract() == 0.0 { return None; }
                                Some((f * multiplier * 100.0).round() as i64)
                            } else { None }
                        }).collect()
                    };
                    let nums_pm = extract_nums(&pm.question);
                    let nums_km = extract_nums(&km.question);
                    if (!nums_pm.is_empty() || !nums_km.is_empty())
                        && nums_pm.intersection(&nums_km).next().is_none() { continue; }
                }

                let unified_id = compute_unified_market_id(&pm.question, "cross_platform", &pm_expiration.to_rfc3339());
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

                let category = if shared_crypto { MarketCategory::Crypto } else { pm.category };
                let confidence = if sim >= 0.95 { 0.99 } else if sim >= 0.7 { 0.98 } else { 0.95 };

                info!(
                    "✅ MATCH: poly='{}' ↔ kalshi='{}' | sim={:.2} | exp_diff={}s | is_15m={}",
                    pm.question_normalized, km.question_normalized, sim, exp_diff_secs, is_15m_market
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

        if matched.is_empty() {
            warn!(
                "⚠️  ZERO markets matched! poly_count={} kalshi_count={}",
                poly_markets.len(), kalshi_markets.len()
            );
        } else {
            info!("Discovery: {} cross-platform pairs matched", matched.len());
        }

        Ok(matched)
    }

    pub(super) fn normalize_question(q: &str) -> String {
        let mut lower = q.to_lowercase().replace(',', "").replace('$', "");
        let replacements = [
            ("bitcoin", "btc"), ("ethereum", "eth"), ("solana", "sol"),
            ("ripple", "xrp"), ("dogecoin", "doge"), ("binance coin", "bnb"),
            ("cardano", "ada"), ("minutes", "min"), ("minute", "min"), ("mins", "min"),
        ];
        for (from, to) in replacements {
            if lower.contains(from) { lower = lower.replace(from, to); }
        }
        let mut result = String::with_capacity(lower.len());
        let mut last_was_space = true;
        for c in lower.chars() {
            if c.is_alphanumeric() { result.push(c); last_was_space = false; }
            else if !last_was_space { result.push(' '); last_was_space = true; }
        }
        if result.ends_with(' ') { result.pop(); }
        result
    }

    async fn fetch_polymarket_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let mut all_markets = Vec::new();
        let now = Utc::now();
        let gamma_url = "https://gamma-api.polymarket.com/markets";
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // FIX-1: Dedicated pass for 15-min crypto markets — search by slug pattern
        let crypto_keywords = ["btc-updown", "eth-updown", "sol-updown", "xrp-updown",
                                "doge-updown", "bnb-updown", "btc updown", "bitcoin up or down",
                                "btc up or down", "15 min", "15m"];
        for kw in &crypto_keywords {
            let resp = match tokio::time::timeout(
                Duration::from_secs(10),
                self.http.get(gamma_url)
                    .query(&[("active", "true"), ("closed", "false"), ("limit", "100"), ("q", kw)])
                    .send()
            ).await {
                Ok(Ok(r)) if r.status().is_success() => r,
                _ => continue,
            };
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                let page = Self::extract_markets_array(&data);
                for m in self.parse_gamma_polymarket_response(page, now) {
                    if seen_ids.insert(m.platform_market_id.clone()) {
                        all_markets.push(m);
                    }
                }
            }
        }
        info!("Polymarket 15-min/crypto pass: {} markets", all_markets.len());

        // General near-term pass (≤7 days)
        let week_out = now + chrono::Duration::days(7);
        let end_max = week_out.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        if let Ok(Ok(resp)) = tokio::time::timeout(
            Duration::from_secs(10),
            self.http.get(gamma_url)
                .query(&[("active","true"),("closed","false"),("limit","200"),("end_date_max",end_max.as_str())])
                .send()
        ).await {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    let page = Self::extract_markets_array(&data);
                    for m in self.parse_gamma_polymarket_response(page, now) {
                        if seen_ids.insert(m.platform_market_id.clone()) {
                            all_markets.push(m);
                        }
                    }
                }
            }
        }

        // General paginated pass
        let mut offset = 0u64;
        'general: loop {
            let offset_str = offset.to_string();
            let resp = match tokio::time::timeout(
                Duration::from_secs(10),
                self.http.get(gamma_url)
                    .query(&[("active","true"),("closed","false"),("acceptingOrders","true"),("limit","200"),("offset",offset_str.as_str())])
                    .send()
            ).await {
                Ok(Ok(r)) => r,
                _ => break 'general,
            };
            if !resp.status().is_success() { break 'general; }
            let data = match resp.json::<serde_json::Value>().await {
                Ok(d) => d,
                Err(_) => break 'general,
            };
            let page = Self::extract_markets_array(&data);
            if page.is_empty() { break 'general; }
            let page_len = page.len();
            for m in self.parse_gamma_polymarket_response(page, now) {
                if seen_ids.insert(m.platform_market_id.clone()) { all_markets.push(m); }
            }
            if all_markets.len() >= 5000 || page_len < 200 { break 'general; }
            offset += 200;
        }

        info!("Total Polymarket markets fetched: {}", all_markets.len());
        Ok(all_markets)
    }

    fn parse_gamma_polymarket_response(&self, arr: &[serde_json::Value], now: DateTime<Utc>) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();

        for item in arr {
            let question = item.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if question.is_empty() { continue; }

            let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");

            // FIX-1: Never skip 15-min crypto markets based on acceptingOrders
            let q_lower = question.to_lowercase();
            let is_15m_crypto = (q_lower.contains("up or down") || q_lower.contains("15 min") || slug.contains("-15m-") || slug.contains("updown"))
                && (q_lower.contains("bitcoin") || q_lower.contains("btc") || q_lower.contains("ethereum")
                    || q_lower.contains("eth") || q_lower.contains("sol") || q_lower.contains("xrp")
                    || q_lower.contains("doge") || q_lower.contains("bnb"));

            let accepting = item.get("acceptingOrders").and_then(|v| v.as_bool()).unwrap_or(false);
            if !accepting && !is_15m_crypto { continue; }

            // FIX-2: Parse clobTokenIds robustly
            let (yes_token, no_token) = parse_clob_token_ids(item);
            if yes_token.is_empty() || no_token.is_empty() {
                if is_15m_crypto {
                    warn!("15m crypto market missing token IDs: {}", question);
                }
                continue;
            }
            let token_id = format!("{},{}", yes_token, no_token);

            // Expiration: prefer slug timestamp for 15m markets
            let end_date = {
                let mut resolved: Option<DateTime<Utc>> = None;

                // Best: parse slug directly for 15m
                if slug.contains("-15m-") || slug.contains("-updown-15m-") {
                    if let Some(last_dash) = slug.rfind('-') {
                        if let Ok(ts) = slug[last_dash + 1..].parse::<i64>() {
                            // slug ts = round START; expiration = start + 900s
                            if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, ts + 900, 0) {
                                resolved = Some(dt);
                            }
                        }
                    }
                }

                if resolved.is_none() {
                    for field in &["endDateIso", "endDate", "end_date_iso", "closeTime"] {
                        if let Some(v) = item.get(field) {
                            if let Some(s) = v.as_str() {
                                if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                                    resolved = Some(dt.with_timezone(&Utc)); break;
                                }
                                if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
                                    resolved = Some(DateTime::from_naive_utc_and_offset(naive, Utc)); break;
                                }
                            }
                            if let Some(ts) = v.as_i64() {
                                let secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
                                if let chrono::LocalResult::Single(dt) = chrono::TimeZone::timestamp_opt(&Utc, secs, 0) {
                                    resolved = Some(dt); break;
                                }
                            }
                        }
                    }
                }

                resolved.unwrap_or_else(|| now + chrono::Duration::hours(1))
            };

            if end_date <= now { continue; }

            let question_normalized = Self::normalize_question(&question);
            let category = if question_normalized.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "ada" | "crypto")) {
                MarketCategory::Crypto
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

        let targeted_series: &[(&str, &str)] = &[
            // ── Crypto 15-minute direction (most important for this use case) ─
            ("KXBTC15M",  "BTC 15m"),
            ("KXETH15M",  "ETH 15m"),
            ("KXSOL15M",  "SOL 15m"),
            ("KXDOGE15M", "DOGE 15m"),
            ("KXXRP15M",  "XRP 15m"),
            ("KXBNB15M",  "BNB 15m"),
            // ── Other crypto ─────────────────────────────────────────────────
            ("KXBTC",     "BTC price target"),
            ("KXETH",     "ETH price target"),
            // ── Sports / political ────────────────────────────────────────────
            ("KXNBA",     "NBA game"),
            ("KXNHL",     "NHL game"),
            ("KXMLB",     "MLB game"),
            ("KXFED",     "Fed rate decision"),
            ("KXCPI",     "CPI inflation"),
        ];

        let series_futures: Vec<_> = targeted_series.iter().map(|(series, _label)| {
            let http = self.http.clone();
            let url = url.clone();
            let auth_token = self.kalshi_auth.as_ref().and_then(|a| a.generate_token().ok());
            let series = *series;
            async move {
                let mut req = http.get(&url)
                    // FIX-3: fetch more markets per series (was 200 → 500)
                    .query(&[("status", "open"), ("series_ticker", series), ("limit", "500")]);
                if let Some(token) = auth_token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
                match tokio::time::timeout(Duration::from_secs(15), req.send()).await {
                    Ok(Ok(resp)) => match resp.json::<serde_json::Value>().await {
                        Ok(data) => Some((series, data)),
                        Err(_) => None,
                    },
                    _ => None,
                }
            }
        }).collect();

        let series_results = futures_util::future::join_all(series_futures).await;

        let mut all_markets: Vec<DiscoveredMarket> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for result in series_results {
            if let Some((series, data)) = result {
                let page = self.parse_kalshi_markets_raw(&data, &mut seen);
                if !page.is_empty() {
                    info!(series, count = page.len(), "Kalshi series fetched");
                }
                all_markets.extend(page);
            }
        }

        // Generic open markets (first page)
        let auth_token = self.kalshi_auth.as_ref().and_then(|a| a.generate_token().ok());
        let mut req = self.http.get(&url).query(&[("status", "open"), ("limit", "500")]);
        if let Some(token) = &auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        if let Ok(Ok(resp)) = tokio::time::timeout(Duration::from_secs(15), req.send()).await {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                let page = self.parse_kalshi_markets_raw(&data, &mut seen);
                all_markets.extend(page);
            }
        }

        info!("Total Kalshi markets fetched: {}", all_markets.len());
        Ok(all_markets)
    }

    fn parse_kalshi_markets_raw(
        &self,
        resp: &serde_json::Value,
        seen_tickers: &mut std::collections::HashSet<String>,
    ) -> Vec<DiscoveredMarket> {
        let mut markets = Vec::new();
        let arr = match resp.get("markets").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return markets,
        };

        for item in arr {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if title.is_empty() { continue; }
            let ticker = item.get("ticker").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if ticker.is_empty() { continue; }
            if ticker.starts_with("KXMVE") { continue; } // skip parlays
            if !seen_tickers.insert(ticker.clone()) { continue; }

            let close_time = item.get("close_time").and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

            if close_time < Utc::now() { continue; }

            let question_normalized = Self::normalize_question(&title);
            let category = if question_normalized.split_whitespace().any(|w| matches!(w, "btc" | "eth" | "sol" | "xrp" | "doge" | "bnb" | "ada" | "crypto")) {
                MarketCategory::Crypto
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
        markets
    }
}

/// FIX-2: Robust parsing of Polymarket clobTokenIds field
fn parse_clob_token_ids(item: &serde_json::Value) -> (String, String) {
    // Try clobTokenIds as a JSON string like '["id1","id2"]'
    if let Some(raw) = item.get("clobTokenIds").and_then(|v| v.as_str()) {
        let cleaned = raw.trim().trim_matches('"');
        // May need to unescape
        let to_parse = if cleaned.contains("\\\"") {
            cleaned.replace("\\\"", "\"")
        } else {
            cleaned.to_string()
        };
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&to_parse) {
            let yes = ids.get(0).cloned().unwrap_or_default();
            let no = ids.get(1).cloned().unwrap_or_default();
            if !yes.is_empty() && !no.is_empty() { return (yes, no); }
        }
    }

    // Try clobTokenIds as a native JSON array
    if let Some(arr) = item.get("clobTokenIds").and_then(|v| v.as_array()) {
        let yes = arr.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let no = arr.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !yes.is_empty() && !no.is_empty() { return (yes, no); }
    }

    // Fallback: legacy tokens array
    if let Some(tokens) = item.get("tokens").and_then(|v| v.as_array()) {
        let yes = tokens.get(0).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let no = tokens.get(1).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !yes.is_empty() && !no.is_empty() { return (yes, no); }
    }

    (String::new(), String::new())
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod discovery_tests;
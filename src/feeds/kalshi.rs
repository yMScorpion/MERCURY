use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::client::IntoClientRequest, tungstenite::Message, Connector};
use tracing::{error, info, warn};
use uuid::Uuid;
use arrayvec::ArrayVec;

use super::base::FeedHandler;
use crate::config::KalshiConfig;
use crate::crypto::jwt::KalshiAuth;
use crate::types::*;
use std::sync::Arc;

pub struct KalshiFeed {
    config: KalshiConfig,
    auth: Option<Arc<KalshiAuth>>,
    db: Arc<dyn crate::db::Database>,
    subscriptions: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, KalshiOrderBook>,
    /// HTTP client for REST book bootstrap calls
    http: reqwest::Client,
    latest_seq: u64,
}

#[derive(Debug, Default)]
struct KalshiOrderBook {
    // yes bids in dollar format [0.0 - 1.0]
    pub yes_bids: std::collections::BTreeMap<Decimal, Decimal>,
    // no bids in dollar format [0.0 - 1.0]
    pub no_bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub is_initialized: bool,
    pub seq: u64,
}

impl KalshiOrderBook {
    fn new() -> Self { Self::default() }

    /// Best YES bid price
    fn best_yes_bid(&self) -> Option<(Decimal, Decimal)> {
        self.yes_bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    /// Best YES ask = 1 - best NO bid (highest NO bid)
    fn best_yes_ask(&self) -> Option<(Decimal, Decimal)> {
        self.no_bids.iter().next_back().map(|(&no_price, &size)| {
            (Decimal::ONE - no_price, size)
        })
    }

    fn depth(&self) -> ArrayVec<PriceLevel, 20> {
        let mut levels = ArrayVec::new();
        for (&p, &s) in self.yes_bids.iter().rev().take(10) {
            if levels.try_push(PriceLevel { price: p, size: s }).is_err() { break; }
        }
        if let Some((ask, ask_size)) = self.best_yes_ask() {
            let _ = levels.try_push(PriceLevel { price: ask, size: ask_size });
        }
        levels
    }
}

// ── Wire message shapes ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct KalshiSubscribe {
    id: u64,
    cmd: String,
    params: KalshiSubParams,
}

#[derive(Debug, Serialize, Deserialize)]
struct KalshiSubParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct KalshiEnvelope {
    #[serde(rename = "type")]
    msg_type: String,
    seq: Option<u64>,
    msg: Option<serde_json::Value>,
}

/// REST API response for GET /markets/{ticker}/orderbook
#[derive(Debug, Deserialize)]
struct KalshiRestOrderbook {
    #[serde(default)]
    orderbook: KalshiRestBook,
}

#[derive(Debug, Deserialize, Default)]
struct KalshiRestBook {
    /// YES bids: [[price_cents, quantity], ...]
    #[serde(default)]
    yes: Vec<Vec<serde_json::Value>>,
    /// NO bids: [[price_cents, quantity], ...]
    #[serde(default)]
    no: Vec<Vec<serde_json::Value>>,
}

impl KalshiFeed {
    pub async fn new(
        config: KalshiConfig,
        auth: Option<KalshiAuth>,
        db: Arc<dyn crate::db::Database>,
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        if let Ok(markets) = db.get_active_markets().await {
            for m in markets {
                if let Some(info) = m.platforms.get(&Platform::Kalshi) {
                    subs_map.insert(info.platform_market_id.clone(), m.unified_id);
                }
            }
        }

        let http = reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build Kalshi REST client");

        Self {
            config,
            auth: auth.map(Arc::new),
            db,
            subscriptions: subs_map,
            books: std::collections::HashMap::new(),
            http,
            latest_seq: 0,
        }
        }

    // ── Message dispatcher ──────────────────────────────────────────────────

    fn handle_message(&mut self, text: &str, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let env: KalshiEnvelope = serde_json::from_str(text)
            .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

        match env.msg_type.as_str() {
            "orderbook_snapshot" => self.handle_snapshot(&env, tick_tx),
            "orderbook_delta"    => self.handle_delta(&env, tick_tx),
            "subscribed" => {
                if let Some(msg) = &env.msg {
                    let channel = msg.get("channel").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let sid = msg.get("sid").and_then(|v| v.as_u64()).unwrap_or(0);
                    tracing::debug!(channel, sid, "Kalshi subscription confirmed");
                }
                Ok(())
            }
            "error" => {
                let (code, msg_text) = if let Some(msg) = &env.msg {
                    let code = msg.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
                    let msg_str = msg.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown");
                    (code, msg_str.to_string())
                } else {
                    (0, "unknown error".to_string())
                };
                anyhow::bail!("Kalshi WS error {}: {}", code, msg_text);
            }
            _ => Ok(()),
        }
    }

    fn handle_snapshot(&mut self, env: &KalshiEnvelope, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let msg = env.msg.as_ref().context("missing msg in snapshot")?;
        let ticker = msg.get("market_ticker").and_then(|v| v.as_str())
            .context("missing market_ticker in snapshot")?;
        let seq = env.seq.unwrap_or(0);

        let book = self.books.entry(ticker.to_string()).or_insert_with(KalshiOrderBook::new);
        book.yes_bids.clear();
        book.no_bids.clear();

        for field in &["yes_dollars_fp", "yes"] {
            if let Some(arr) = msg.get(field).and_then(|v| v.as_array()) {
                for entry in arr {
                    let (p, s) = Self::parse_price_size_entry(entry)?;
                    if s > Decimal::ZERO {
                        book.yes_bids.insert(p, s);
                    }
                }
                break;
            }
        }

        for field in &["no_dollars_fp", "no"] {
            if let Some(arr) = msg.get(field).and_then(|v| v.as_array()) {
                for entry in arr {
                    let (p, s) = Self::parse_price_size_entry(entry)?;
                    if s > Decimal::ZERO {
                        book.no_bids.insert(p, s);
                    }
                }
                break;
            }
        }

        book.is_initialized = true;
        book.seq = seq;

        tracing::debug!(ticker, yes_levels = book.yes_bids.len(), no_levels = book.no_bids.len(), "Kalshi snapshot applied");

        if let Some(tick) = self.emit_tick(ticker) {
            let _ = tick_tx.send(tick);
        }
        Ok(())
    }

    fn handle_delta(&mut self, env: &KalshiEnvelope, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let msg = env.msg.as_ref().context("missing msg in delta")?;
        let ticker = msg.get("market_ticker").and_then(|v| v.as_str())
            .context("missing market_ticker in delta")?;
        let seq = env.seq.unwrap_or(0);

        let book = match self.books.get_mut(ticker) {
            Some(b) => b,
            None => return Ok(()),
        };

        if !book.is_initialized {
            tracing::debug!(ticker, "Skipping delta for uninitialized book — waiting for snapshot");
            return Ok(());
        }

        // Global sequence tracking
        if seq > 0 && self.latest_seq > 0 && seq < self.latest_seq {
            return Ok(());
        }

        if seq > 0 && self.latest_seq > 0 && seq > self.latest_seq + 1 {
            tracing::warn!(
                ticker,
                expected = self.latest_seq + 1,
                got = seq,
                "Kalshi sequence gap detected."
            );
        }
        self.latest_seq = seq.max(self.latest_seq);

        if let (Some(price_val), Some(delta_val), Some(side_val)) = (
            msg.get("price_dollars"),
            msg.get("delta_fp"),
            msg.get("side").and_then(|v| v.as_str()),
        ) {
            let price = Self::parse_decimal_field(price_val)?;
            let delta = Self::parse_decimal_field(delta_val)?;
            let price = if price > Decimal::ONE {
                price / Decimal::from(100)
            } else {
                price
            };
            let target = if side_val == "yes" { &mut book.yes_bids } else { &mut book.no_bids };
            let current = target.get(&price).copied().unwrap_or(Decimal::ZERO);
            let new_qty = (current + delta).max(Decimal::ZERO);
            if new_qty == Decimal::ZERO {
                target.remove(&price);
            } else {
                target.insert(price, new_qty);
            }
        }

        book.seq = seq;

        if let Some(tick) = self.emit_tick(ticker) {
            let _ = tick_tx.send(tick);
        }
        Ok(())
    }

    fn parse_price_size_entry(entry: &serde_json::Value) -> Result<(Decimal, Decimal)> {
        if let Some(arr) = entry.as_array() {
            let p = arr.get(0)
                .and_then(|v| v.as_str())
                .and_then(|s| Decimal::from_str(s).ok())
                .or_else(|| arr.get(0).and_then(|v| v.as_f64()).and_then(|f| Decimal::try_from(f).ok()))
                .context("bad price in entry")?;
            let s = arr.get(1)
                .and_then(|v| v.as_str())
                .and_then(|s| Decimal::from_str(s).ok())
                .or_else(|| arr.get(1).and_then(|v| v.as_f64()).and_then(|f| Decimal::try_from(f).ok()))
                .context("bad size in entry")?;
            let p = if p > Decimal::ONE {
                p / Decimal::from(100)
            } else {
                p
            };
            return Ok((p, s));
        }
        anyhow::bail!("unexpected entry shape: {entry}")
    }

    fn parse_decimal_field(v: &serde_json::Value) -> Result<Decimal> {
        if let Some(s) = v.as_str() {
            return Decimal::from_str(s).context("decimal parse");
        }
        if let Some(n) = v.as_f64() {
            return Decimal::try_from(n).context("f64->decimal");
        }
        anyhow::bail!("cannot parse decimal from {v}")
    }

    fn emit_tick(&self, ticker: &str) -> Option<NormalizedTick> {
        let market_id = *self.subscriptions.get(ticker)?;
        let book = self.books.get(ticker)?;
        if !book.is_initialized { return None; }

        let bid = book.best_yes_bid().unwrap_or((Decimal::ZERO, Decimal::ZERO));
        let ask = book.best_yes_ask().unwrap_or((Decimal::ZERO, Decimal::ZERO));

        // Only emit if we have at least one valid side
        if bid.0 == Decimal::ZERO && ask.0 == Decimal::ZERO { return None; }

        let mid = match (bid.0 > Decimal::ZERO, ask.0 > Decimal::ZERO) {
            (true, true) => (bid.0 + ask.0) / Decimal::TWO,
            (true, false) => bid.0,
            (false, true) => ask.0,
            _ => return None,
        };

        // Validate prices are in [0,1]
        if bid.0 > Decimal::ONE || ask.0 > Decimal::ONE { return None; }

        let fee_bps = if ticker.contains("15M") || ticker.contains("BTC") || ticker.contains("ETH") {
            5u16
        } else {
            175u16
        };

        Some(NormalizedTick {
            platform: Platform::Kalshi,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: book.depth(),
            fee_rate_bps: fee_bps,
            sequence: book.seq,
        })
    }

    /// Fetch current orderbook via Kalshi REST API and bootstrap the local book.
    /// Called after subscribing to a ticker (both initial and dynamic) to ensure
    /// we have immediate book state rather than waiting for the first WS snapshot.
    ///
    /// Kalshi REST endpoint: GET /trade-api/v2/markets/{ticker}/orderbook
    /// Returns { orderbook: { yes: [[price_cents, qty], ...], no: [[price_cents, qty], ...] } }
    async fn bootstrap_book_via_rest(
        &mut self,
        ticker: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let url = format!("{}/markets/{}/orderbook", self.config.rest_url, ticker);

        let auth_header = if let Some(auth) = &self.auth {
            match auth.auth_header().await {
                Ok(h) => Some(h),
                Err(e) => {
                    warn!(ticker, error = %e, "Failed to get Kalshi auth header for REST bootstrap");
                    None
                }
            }
        } else {
            None
        };

        let mut req = self.http.get(&url);
        if let Some(header) = auth_header {
            req = req.header("Authorization", header);
        }

        let resp = match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            req.send(),
        ).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                warn!(ticker, error = %e, "Kalshi REST orderbook fetch failed");
                return;
            }
            Err(_) => {
                warn!(ticker, "Kalshi REST orderbook fetch timed out");
                return;
            }
        };

        if !resp.status().is_success() {
            warn!(ticker, status = %resp.status(), "Kalshi REST orderbook returned non-2xx");
            return;
        }

        let data: KalshiRestOrderbook = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                warn!(ticker, error = %e, "Failed to parse Kalshi REST orderbook response");
                return;
            }
        };

        let book = self.books.entry(ticker.to_string()).or_insert_with(KalshiOrderBook::new);
        book.yes_bids.clear();
        book.no_bids.clear();

        // REST API returns prices in CENTS (integers 1-99)
        // Convert to dollar format (divide by 100)
        for entry in &data.orderbook.yes {
            if entry.len() >= 2 {
                let price_cents = entry[0].as_i64().unwrap_or(0);
                let qty = entry[1].as_i64().unwrap_or(0);
                if price_cents > 0 && price_cents < 100 && qty > 0 {
                    let price = Decimal::from(price_cents) / Decimal::from(100);
                    book.yes_bids.insert(price, Decimal::from(qty));
                }
            }
        }

        for entry in &data.orderbook.no {
            if entry.len() >= 2 {
                let price_cents = entry[0].as_i64().unwrap_or(0);
                let qty = entry[1].as_i64().unwrap_or(0);
                if price_cents > 0 && price_cents < 100 && qty > 0 {
                    let price = Decimal::from(price_cents) / Decimal::from(100);
                    book.no_bids.insert(price, Decimal::from(qty));
                }
            }
        }

        book.is_initialized = true;
        book.seq = 0; // Will be updated on first WS delta

        let yes_count = book.yes_bids.len();
        let no_count = book.no_bids.len();

        if let Some(tick) = self.emit_tick(ticker) {
            info!(
                ticker,
                bid = %tick.bid_price,
                ask = %tick.ask_price,
                yes_levels = yes_count,
                no_levels = no_count,
                "Kalshi REST orderbook bootstrap successful — tick emitted"
            );
            let _ = tick_tx.send(tick);
        } else {
            tracing::debug!(ticker, yes_levels = yes_count, no_levels = no_count,
                "Kalshi REST orderbook fetched but no valid BBO (market may have no resting orders)");
        }
    }
}

#[async_trait]
impl FeedHandler for KalshiFeed {
    fn platform(&self) -> Platform { Platform::Kalshi }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.yes_bids.clear();
            book.no_bids.clear();
            book.is_initialized = false;
            book.seq = 0;
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let auth = match &self.auth {
            Some(a) => a.clone(),
            None => {
                error!("Kalshi WebSocket requires authentication. Set KALSHI_API_KEY_ID and KALSHI_RSA_PEM_PATH.");
                tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
                return Ok(());
            }
        };

        info!("Connecting to Kalshi WebSocket: {}", self.config.ws_url);

        let mut request = self.config.ws_url.as_str()
            .into_client_request()
            .context("Invalid Kalshi WebSocket URL")?;

        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::USER_AGENT,
            "MERCURY/0.1.0".try_into().unwrap(),
        );

        let headers = auth.generate_ws_headers()?;
        for (key, val) in headers {
            use std::str::FromStr;
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::header::HeaderName::from_str(&key)
                    .context("Invalid Kalshi header name")?,
                tokio_tungstenite::tungstenite::http::header::HeaderValue::from_str(&val)
                    .context("Invalid Kalshi header value")?,
            );
        }

        let tls = crate::crypto::tls::build_tls_connector()
            .context("Failed to build Kalshi TLS connector")?;

        let (ws_stream, _) = match connect_async_tls_with_config(
            request, None, false, Some(Connector::NativeTls(tls))
        ).await {
            Ok(v) => v,
            Err(e) => anyhow::bail!("Kalshi WebSocket connect failed: {e}"),
        };

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to orderbook_delta for all known tickers
        let tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
        if !tickers.is_empty() {
            for chunk in tickers.chunks(50) {
                let sub = KalshiSubscribe {
                    id: 1,
                    cmd: "subscribe".into(),
                    params: KalshiSubParams {
                        channels: vec!["orderbook_delta".into()],
                        market_tickers: chunk.to_vec(),
                    },
                };
                let msg_text = serde_json::to_string(&sub)?;
                write.send(Message::Text(msg_text.into())).await
                    .context("Kalshi subscribe send failed")?;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            info!(count = tickers.len(), "Subscribed to Kalshi markets");
        }

        // Initialize book state for all known tickers
        for ticker in self.subscriptions.keys() {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        // Bootstrap all initial tickers via REST immediately.
        // Kalshi DOES send automatic orderbook_snapshot on WS subscription,
        // but fetching via REST ensures we have data right away without waiting
        // for the WS snapshot to arrive (which can take several seconds).
        {
            let initial_tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
            for ticker in &initial_tickers {
                self.bootstrap_book_via_rest(ticker, &tick_tx).await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            if !initial_tickers.is_empty() {
                info!(count = initial_tickers.len(), "Bootstrapped initial Kalshi books via REST");
            }
        }

        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(15));
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                msg_opt = tokio::time::timeout(std::time::Duration::from_secs(90), read.next()) => {
                    let msg = match msg_opt {
                        Ok(Some(Ok(m))) => m,
                        Ok(Some(Err(e))) => return Err(e.into()),
                        Ok(None) => break,
                        Err(_) => return Err(anyhow::anyhow!("Kalshi read timeout")),
                    };
                    match msg {
                        Message::Text(text) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                let es = e.to_string();
                                if es.contains("sequence gap") || es.contains("snapshot first") {
                                    return Err(e);
                                }
                                tracing::warn!(error = %e, "Kalshi message handling non-fatal error");
                            }
                        }
                        Message::Ping(data) => { let _ = write.send(Message::Pong(data)).await; }
                        Message::Close(_) => { info!("Kalshi WebSocket closed"); break; }
                        _ => {}
                    }
                }

                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                        warn!(error = %e, "Kalshi ping failed");
                        return Err(e.into());
                    }
                }

                _ = sync_interval.tick() => {
                    // Dynamically subscribe to newly discovered markets
                    match self.db.get_active_markets().await {
                        Ok(markets) => {
                            let mut new_tickers = Vec::new();
                            for m in markets {
                                if let Some(info) = m.platforms.get(&Platform::Kalshi) {
                                    let ticker = info.platform_market_id.clone();
                                    if !self.subscriptions.contains_key(&ticker) {
                                        self.subscriptions.insert(ticker.clone(), m.unified_id);
                                        self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
                                        new_tickers.push(ticker);
                                    }
                                }
                            }
                            if !new_tickers.is_empty() {
                                info!(count = new_tickers.len(), tickers = ?new_tickers, "Dynamically subscribing to new Kalshi markets");
                                for chunk in new_tickers.chunks(50) {
                                    let sub = KalshiSubscribe {
                                        id: 2,
                                        cmd: "subscribe".into(),
                                        params: KalshiSubParams {
                                            channels: vec!["orderbook_delta".into()],
                                            market_tickers: chunk.to_vec(),
                                        },
                                    };
                                    if let Ok(msg_text) = serde_json::to_string(&sub) {
                                        let _ = write.send(Message::Text(msg_text.into())).await;
                                    }
                                }

                                // FIX: Bootstrap book state via REST for dynamically added tickers.
                                // The Kalshi WS sends an orderbook_snapshot after subscribing, but
                                // it can take several seconds. REST bootstrap ensures immediate data.
                                for ticker in &new_tickers {
                                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    self.bootstrap_book_via_rest(ticker, &tick_tx).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                }
                            }
                        }
                        Err(e) => warn!(error = %e, "Kalshi feed: DB refresh failed"),
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "kalshi_tests.rs"]
mod kalshi_tests;
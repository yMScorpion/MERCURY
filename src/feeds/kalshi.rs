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
    /// Key: ticker -> unified market UUID
    subscriptions: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, KalshiOrderBook>,
    http: reqwest::Client,
    latest_seq: u64,
    /// The subscription ID returned by Kalshi for the orderbook_delta channel.
    /// Required for update_subscription (add/remove markets) commands.
    orderbook_sid: Option<u64>,
    /// Next command id counter
    cmd_id: u64,
}

#[derive(Debug, Default)]
struct KalshiOrderBook {
    pub yes_bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub no_bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub is_initialized: bool,
    pub seq: u64,
}

impl KalshiOrderBook {
    fn new() -> Self { Self::default() }

    fn best_yes_bid(&self) -> Option<(Decimal, Decimal)> {
        self.yes_bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

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

/// Initial subscription: subscribe to a channel for a list of tickers.
#[derive(Debug, Serialize)]
struct KalshiSubscribe {
    id: u64,
    cmd: String,
    params: KalshiSubParams,
}

#[derive(Debug, Serialize)]
struct KalshiSubParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
}

/// Dynamic add/remove markets to an existing subscription using its sid.
/// This is the correct way to add markets mid-session per Kalshi docs.
#[derive(Debug, Serialize)]
struct KalshiUpdateSubscription {
    id: u64,
    cmd: String,
    params: KalshiUpdateSubParams,
}

#[derive(Debug, Serialize)]
struct KalshiUpdateSubParams {
    /// The subscription ID(s) to update (returned in the "subscribed" response as "sid")
    sids: Vec<u64>,
    market_tickers: Vec<String>,
    /// "add_markets" or "delete_markets"
    action: String,
}

#[derive(Debug, Deserialize)]
struct KalshiEnvelope {
    #[serde(rename = "type")]
    msg_type: String,
    seq: Option<u64>,
    sid: Option<u64>,
    msg: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct KalshiRestOrderbook {
    #[serde(default)]
    orderbook: KalshiRestBook,
}

#[derive(Debug, Deserialize, Default)]
struct KalshiRestBook {
    #[serde(default)]
    yes: Vec<Vec<serde_json::Value>>,
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
            orderbook_sid: None,
            cmd_id: 1,
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.cmd_id;
        self.cmd_id += 1;
        id
    }

    // ── Message dispatcher ──────────────────────────────────────────────────

    fn handle_message(&mut self, text: &str, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let env: KalshiEnvelope = serde_json::from_str(text)
            .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

        match env.msg_type.as_str() {
            "orderbook_snapshot" => self.handle_snapshot(&env, tick_tx),
            "orderbook_delta"    => self.handle_delta(&env, tick_tx),
            "subscribed" => {
                if let Some(sid) = env.sid {
                    // Store the sid so we can use update_subscription later
                    if self.orderbook_sid.is_none() {
                        self.orderbook_sid = Some(sid);
                        tracing::info!(sid, "Kalshi orderbook_delta subscription confirmed, sid stored");
                    }
                }
                if let Some(msg) = &env.msg {
                    let channel = msg.get("channel").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let sid = env.sid.or_else(|| msg.get("sid").and_then(|v| v.as_u64())).unwrap_or(0);
                    tracing::debug!(channel, sid, "Kalshi subscription confirmed");
                }
                Ok(())
            }
            "ok" => {
                // Response to update_subscription — log any market list changes
                if let Some(msg) = &env.msg {
                    if let Some(tickers) = msg.get("market_tickers").and_then(|v| v.as_array()) {
                        tracing::debug!(count = tickers.len(), "Kalshi subscription update acknowledged");
                    }
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

        if seq > 0 && self.latest_seq > 0 && seq < self.latest_seq {
            return Ok(());
        }

        if seq > 0 && self.latest_seq > 0 && seq > self.latest_seq + 1 {
            tracing::warn!(ticker, expected = self.latest_seq + 1, got = seq, "Kalshi sequence gap detected.");
        }
        self.latest_seq = seq.max(self.latest_seq);

        if let (Some(price_val), Some(delta_val), Some(side_val)) = (
            msg.get("price_dollars"),
            msg.get("delta_fp"),
            msg.get("side").and_then(|v| v.as_str()),
        ) {
            let price = Self::parse_decimal_field(price_val)?;
            let delta = Self::parse_decimal_field(delta_val)?;
            let price = if price > Decimal::ONE { price / Decimal::from(100) } else { price };
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
                .and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok())
                .or_else(|| arr.get(0).and_then(|v| v.as_f64()).and_then(|f| Decimal::try_from(f).ok()))
                .context("bad price in entry")?;
            let s = arr.get(1)
                .and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok())
                .or_else(|| arr.get(1).and_then(|v| v.as_f64()).and_then(|f| Decimal::try_from(f).ok()))
                .context("bad size in entry")?;
            let p = if p > Decimal::ONE { p / Decimal::from(100) } else { p };
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

        if bid.0 == Decimal::ZERO && ask.0 == Decimal::ZERO { return None; }

        let mid = match (bid.0 > Decimal::ZERO, ask.0 > Decimal::ZERO) {
            (true, true) => (bid.0 + ask.0) / Decimal::TWO,
            (true, false) => bid.0,
            (false, true) => ask.0,
            _ => return None,
        };

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
        book.seq = 0;

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
                "Kalshi REST orderbook fetched but no valid BBO");
        }
    }

    /// Subscribe new tickers using the correct Kalshi WS protocol:
    /// - If we have a sid from the initial subscribe, use update_subscription with add_markets
    /// - Otherwise fall back to a new subscribe command
    async fn subscribe_new_tickers(
        &mut self,
        new_tickers: &[String],
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            Message,
        >,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        if new_tickers.is_empty() {
            return;
        }

        info!(count = new_tickers.len(), tickers = ?new_tickers, "Dynamically subscribing to new Kalshi markets");

        // Ensure book state exists for each new ticker before subscribing
        for ticker in new_tickers {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        if let Some(sid) = self.orderbook_sid {
            // Use update_subscription with add_markets — the correct incremental approach
            for chunk in new_tickers.chunks(50) {
                let id = self.next_id();
                let update = KalshiUpdateSubscription {
                    id,
                    cmd: "update_subscription".to_string(),
                    params: KalshiUpdateSubParams {
                        sids: vec![sid],
                        market_tickers: chunk.to_vec(),
                        action: "add_markets".to_string(),
                    },
                };
                if let Ok(msg_text) = serde_json::to_string(&update) {
                    if let Err(e) = write.send(Message::Text(msg_text.into())).await {
                        warn!(error = %e, "Failed to send Kalshi update_subscription");
                    } else {
                        tracing::debug!(sid, count = chunk.len(), "Sent Kalshi update_subscription add_markets");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        } else {
            // No sid yet — use a fresh subscribe command
            for chunk in new_tickers.chunks(50) {
                let id = self.next_id();
                let sub = KalshiSubscribe {
                    id,
                    cmd: "subscribe".to_string(),
                    params: KalshiSubParams {
                        channels: vec!["orderbook_delta".into()],
                        market_tickers: chunk.to_vec(),
                    },
                };
                if let Ok(msg_text) = serde_json::to_string(&sub) {
                    if let Err(e) = write.send(Message::Text(msg_text.into())).await {
                        warn!(error = %e, "Failed to send Kalshi subscribe");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }

        // Bootstrap books via REST for the newly subscribed tickers
        for ticker in new_tickers {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            self.bootstrap_book_via_rest(ticker, tick_tx).await;
        }
    }

    /// Remove stale/resolved tickers from the WS subscription and clean up local state.
    async fn unsubscribe_tickers(
        &mut self,
        old_tickers: &[String],
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            Message,
        >,
    ) {
        if old_tickers.is_empty() {
            return;
        }

        info!(count = old_tickers.len(), tickers = ?old_tickers, "Unsubscribing from resolved Kalshi markets");

        if let Some(sid) = self.orderbook_sid {
            for chunk in old_tickers.chunks(50) {
                let id = self.next_id();
                let update = KalshiUpdateSubscription {
                    id,
                    cmd: "update_subscription".to_string(),
                    params: KalshiUpdateSubParams {
                        sids: vec![sid],
                        market_tickers: chunk.to_vec(),
                        action: "delete_markets".to_string(),
                    },
                };
                if let Ok(msg_text) = serde_json::to_string(&update) {
                    if let Err(e) = write.send(Message::Text(msg_text.into())).await {
                        warn!(error = %e, "Failed to send Kalshi update_subscription delete_markets");
                    }
                }
            }
        }

        // Clean up local state
        for ticker in old_tickers {
            self.books.remove(ticker);
            self.subscriptions.remove(ticker);
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
        // Reset the sid so we re-capture it on reconnect
        self.orderbook_sid = None;
        self.latest_seq = 0;
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

        // Reset sid on new connection — will be set when we receive the "subscribed" response
        self.orderbook_sid = None;
        self.latest_seq = 0;

        // Subscribe to orderbook_delta for all known tickers
        let tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
        if !tickers.is_empty() {
            for chunk in tickers.chunks(50) {
                let id = self.next_id();
                let sub = KalshiSubscribe {
                    id,
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

        // Bootstrap books via REST immediately for initial tickers
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
                    // Sync with DB: find new markets and resolved/expired ones
                    match self.db.get_active_markets().await {
                        Ok(active_markets) => {
                            // Determine which tickers are new (in DB but not subscribed)
                            let mut new_tickers = Vec::new();
                            let mut current_ticker_to_uuid: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();

                            for m in &active_markets {
                                if let Some(info) = m.platforms.get(&Platform::Kalshi) {
                                    let ticker = info.platform_market_id.clone();
                                    current_ticker_to_uuid.insert(ticker.clone(), m.unified_id);
                                    if !self.subscriptions.contains_key(&ticker) {
                                        new_tickers.push(ticker);
                                    }
                                }
                            }

                            // Determine which tickers are stale (subscribed but no longer active)
                            let stale_tickers: Vec<String> = self.subscriptions.keys()
                                .filter(|t| !current_ticker_to_uuid.contains_key(*t))
                                .cloned()
                                .collect();

                            // Add new subscriptions to our map before subscribing
                            for ticker in &new_tickers {
                                if let Some(uuid) = current_ticker_to_uuid.get(ticker) {
                                    self.subscriptions.insert(ticker.clone(), *uuid);
                                }
                            }

                            // Subscribe to new tickers
                            if !new_tickers.is_empty() {
                                self.subscribe_new_tickers(&new_tickers, &mut write, &tick_tx).await;
                            }

                            // Unsubscribe from stale tickers
                            if !stale_tickers.is_empty() {
                                self.unsubscribe_tickers(&stale_tickers, &mut write).await;
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
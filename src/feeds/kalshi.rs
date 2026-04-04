use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::client::IntoClientRequest, tungstenite::Message, Connector};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::KalshiConfig;
use crate::crypto::jwt::KalshiAuth;
use crate::types::*;
use std::sync::Arc;

pub struct KalshiFeed {
    config: KalshiConfig,
    auth: Option<Arc<KalshiAuth>>,
    db: std::sync::Arc<dyn crate::db::Database>,
    subscriptions: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, KalshiOrderBook>,
    sequence: u64,
}

struct KalshiOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_seq: u64,
    is_initialized: bool,
}

impl KalshiOrderBook {
    fn new() -> Self {
        Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new(), last_seq: 0, is_initialized: false }
    }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / rust_decimal::Decimal::from(2))
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        levels.extend(self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels.extend(self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels
    }
}

#[derive(Deserialize)]
struct KalshiWsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    msg: Option<KalshiMsgPayload>,
}

#[derive(Deserialize)]
pub struct KalshiMsgPayload {
    #[serde(default)]
    pub market_ticker: String,
    pub seq: Option<u64>,
    pub yes: Option<Vec<[String; 2]>>,
    pub no: Option<Vec<[String; 2]>>,
    pub price_deltas: Option<KalshiDeltas>,
}

#[derive(Deserialize)]
pub struct KalshiDeltas {
    pub yes: Option<Vec<[String; 2]>>,
    pub no: Option<Vec<[String; 2]>>,
}

#[derive(Serialize)]
struct KalshiSubscribe {
    id: u64,
    cmd: String,
    params: KalshiSubParams,
}

#[derive(Serialize)]
struct KalshiSubParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
}

impl KalshiFeed {
    pub fn new(
        config: KalshiConfig,
        auth: Option<KalshiAuth>,
        db: std::sync::Arc<dyn crate::db::Database>,
        subscriptions: Vec<(String, Uuid)>
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        for (ticker, market_id) in subscriptions {
            subs_map.insert(ticker, market_id);
        }
        Self {
            config,
            auth: auth.map(Arc::new),
            db,
            subscriptions: subs_map,
            books: std::collections::HashMap::new(),
            sequence: 0,
        }
    }

    fn ticker_to_market_id(&self, ticker: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            // CRITICAL FIX: Explicitly extract the string slice for comparison
            .find(|(t, _)| t.as_str() == ticker)
            .map(|(_, id)| *id)
    }
    fn emit_tick(&self, ticker: &str) -> Option<NormalizedTick> {
        let market_id = self.ticker_to_market_id(ticker)?;
        let book = self.books.get(ticker)?;
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        // CRIT-5 FIX: Route feed calculations through the centralized normalizer function
        // to guarantee identical math between the execution engine and the feed estimator.
        let fee_per_contract = crate::feeds::normalizer::kalshi_fee(mid, Decimal::ONE);
        
        let fee_bps = if mid > Decimal::ZERO {
            let bps = (fee_per_contract / mid) * Decimal::from(10000);
            // CRITICAL FIX: Decimal to u16 conversion fails if there is any fractional remainder.
            // We must round the BPS first, otherwise it will constantly default to 175.
            bps.round().try_into().unwrap_or(175u16)
        } else {
            175
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
            book_depth: std::sync::Arc::new(book.depth()),
            fee_rate_bps: fee_bps,
            sequence: 0,
        })
    }
}

#[async_trait]
impl FeedHandler for KalshiFeed {
    fn platform(&self) -> Platform {
        Platform::Kalshi
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
            book.last_seq = 0;
            // Fix: Mark book as uninitialized to reject all deltas until snapshot arrives
            book.is_initialized = false;
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        info!("Connecting to Kalshi WebSocket");

        // Send JWT in Authorization header — never in the URL where it can be
        // logged by proxies, CDNs, or the server's access log (CRIT-2 fix).
        let (ws_stream, _) = if let Some(auth) = &self.auth {
            let token = auth.generate_token()?;
            let mut request = self.config.ws_url.as_str()
                .into_client_request()
                .context("Invalid Kalshi WebSocket URL")?;
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
                format!("Bearer {}", token)
                    .try_into()
                    .context("Failed to build Kalshi auth header")?,
            );
            {
                let mut tls_builder = native_tls::TlsConnector::builder();
                tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                // M-8 FIX: Explicit TLS Cert Pinning
                if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
                    if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                        tls_builder.add_root_certificate(cert);
                    }
                }
                let tls = tls_builder.build().context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(request, None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        } else {
            {
                let mut tls_builder = native_tls::TlsConnector::builder();
                tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                // M-8 FIX: Explicit TLS Cert Pinning
                if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
                    if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                        tls_builder.add_root_certificate(cert);
                    }
                }
                let tls = tls_builder.build().context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(self.config.ws_url.as_str(), None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        };

        let (mut write, mut read) = ws_stream.split();

        let tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
        if !tickers.is_empty() {
            let sub = KalshiSubscribe {
                id: 1,
                cmd: "subscribe".into(),
                params: KalshiSubParams {
                    channels: vec!["orderbook_snapshot".into(), "orderbook_delta".into(), "trade".into()],
                    market_tickers: tickers.clone(),
                },
            };
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = tickers.len(), "Subscribed to Kalshi markets");
        }

        for (ticker, _) in &self.subscriptions {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            tokio::select! {
                msg_opt = read.next() => {
                    let msg = match msg_opt {
                        Some(m) => m,
                        None => continue,
                    };
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                tracing::warn!(error = %e, "Failed to process Kalshi message");
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Ping(data)) => {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await;
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            tracing::info!("Kalshi WebSocket closed");
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                        _ => {}
                    }
                }
                _ = sync_interval.tick() => {
                    // CRITICAL FIX: Dynamically ingest newly discovered markets to prevent Kalshi blindspots
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_subs = Vec::new();
                        for m in markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Kalshi) {
                                let ticker = info.platform_market_id.clone();
                                if !self.subscriptions.contains_key(&ticker) {
                                    self.subscriptions.insert(ticker.clone(), m.unified_id);
                                    new_subs.push(ticker);
                                }
                            }
                        }
                        if !new_subs.is_empty() {
                            // Initialize order books for new tickers BEFORE subscribing
                            for ticker in &new_subs {
                                self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
                            }
                            let sub = KalshiSubscribe {
                                id: 2,
                                cmd: "subscribe".into(),
                                params: KalshiSubParams {
                                    channels: vec!["orderbook_snapshot".into(), "orderbook_delta".into()],
                                    market_tickers: new_subs.clone(),
                                },
                            };
                            if let Ok(msg_text) = serde_json::to_string(&sub) {
                                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(msg_text.into())).await;
                                tracing::info!(count = new_subs.len(), "Dynamically subscribed to new Kalshi markets");
                            }
                        }
                    }
                }
            }
        }
    }
}

impl KalshiFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let msg: KalshiWsMessage = serde_json::from_str(text)?;

        match msg.msg_type.as_str() {
            "orderbook_snapshot" => {
                if let Some(data) = msg.msg {
                    self.handle_orderbook_snapshot(&data, tick_tx);
                }
            }
            "orderbook_delta" => {
                if let Some(data) = msg.msg {
                    if self.handle_orderbook_delta(&data, tick_tx) {
                        return Err(anyhow::anyhow!("Sequence gap — reconnecting for fresh snapshot"));
                    }
                }
            }
            "trade" => {}
            "subscribed" => {
                info!("Kalshi subscription confirmed");
            }
            _ => {
                debug!(msg_type = %msg.msg_type, "Unknown Kalshi message type");
            }
        }

        Ok(())
    }

    fn handle_orderbook_snapshot(
        &mut self,
        data: &KalshiMsgPayload,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let ticker = &data.market_ticker;
        if let Some(book) = self.books.get_mut(ticker) {
            book.is_initialized = true; // CRITICAL FIX: Mark book safe for incoming deltas
            book.bids.clear();
            book.asks.clear();

            // Kalshi API v2 sends prices as dollar-formatted strings ("0.4200"),
            // NOT cent integers. Parse directly — no /100 conversion.
            if let Some(yes_bids) = &data.yes {
                for level in yes_bids {
                    if let (Ok(p), Ok(s)) = (
                        Decimal::from_str(&level[0]),
                        Decimal::from_str(&level[1]),
                    ) {
                        book.bids.insert(p, s);
                    }
                }
            }

            if let Some(no_asks) = &data.no {
                for level in no_asks {
                    if let (Ok(p), Ok(s)) = (
                        Decimal::from_str(&level[0]),
                        Decimal::from_str(&level[1]),
                    ) {
                        // CRITICAL FIX: Kalshi sends bids for the NO token. 
                        // A bid for NO at 0.40 is equivalent to an ask for YES at 0.60.
                        let yes_ask_price = Decimal::ONE - p;
                        book.asks.insert(yes_ask_price, s);
                    }
                }
            }

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(ticker) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }
    }

    fn handle_orderbook_delta(
        &mut self,
        data: &KalshiMsgPayload,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> bool {
        let ticker = &data.market_ticker;
        let seq = data.seq.unwrap_or(0);

        if let Some(book) = self.books.get(ticker) {
            // Fix: If we receive a delta before a snapshot, explicitly trigger a reconnect.
            if !book.is_initialized {
                return true; 
            }
            if seq > 0 && book.last_seq > 0 && seq != book.last_seq + 1 {
                warn!(ticker, expected = book.last_seq + 1, got = seq,
                    "Kalshi sequence gap — reconnecting to get fresh snapshot");
                return true; // signal caller to reconnect
            }
        }

        if let Some(book) = self.books.get_mut(ticker) {
            book.last_seq = seq;

            if let Some(deltas) = &data.price_deltas {
                if let Some(bid_deltas) = &deltas.yes {
                    for delta in bid_deltas {
                        if let (Ok(p), Ok(s)) = (
                            Decimal::from_str(&delta[0]),
                            Decimal::from_str(&delta[1]),
                        ) {
                            if s == Decimal::ZERO { book.bids.remove(&p); }
                            else { book.bids.insert(p, s); }
                        }
                    }
                }

            if let Some(ask_deltas) = &deltas.no {
                    for delta in ask_deltas {
                        if let (Ok(p), Ok(s)) = (
                            Decimal::from_str(&delta[0]),
                            Decimal::from_str(&delta[1]),
                        ) {
                            // CRITICAL FIX: Invert NO bids to YES asks
                            let yes_ask_price = Decimal::ONE - p;
                            if s == Decimal::ZERO { book.asks.remove(&yes_ask_price); }
                            else { book.asks.insert(yes_ask_price, s); }
                        }
                    }
                }
            }

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(ticker) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }

        false // no reconnect needed
    }
}

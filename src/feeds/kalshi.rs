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
}

#[derive(Debug, Serialize, Deserialize)]
struct KalshiSubscribe {
    id: u64,
    #[serde(rename = "type")]
    cmd: String,
    params: KalshiSubParams,
}

#[derive(Debug, Serialize, Deserialize)]
struct KalshiSubParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct KalshiOrderBook {
    pub bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub asks: std::collections::BTreeMap<Decimal, Decimal>,
    pub is_initialized: bool,
}

impl KalshiOrderBook {
    fn new() -> Self {
        Self {
            bids: std::collections::BTreeMap::new(),
            asks: std::collections::BTreeMap::new(),
            is_initialized: false,
        }
    }

    fn depth(&self) -> ArrayVec<PriceLevel, 20> {
        let mut depth = ArrayVec::new();
        let mut bids_it = self.bids.iter().next_back();
        while let Some((&price, &size)) = bids_it {
            let _ = depth.try_push(PriceLevel { price, size });
            if depth.len() >= 10 { break; }
            bids_it = self.bids.range(..price).next_back();
        }
        let bid_count = depth.len();
        for (&price, &size) in self.asks.iter() {
            let _ = depth.try_push(PriceLevel { price, size });
            if depth.len() >= bid_count + 10 { break; }
        }
        depth
    }
}

impl KalshiFeed {
    pub async fn new(
        config: KalshiConfig,
        auth: Option<KalshiAuth>,
        db: Arc<dyn crate::db::Database>,
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        // C-7 FIX: Pre-populate from DB, but don't hang if DB is busy
        if let Ok(markets) = db.get_active_markets().await {
            for m in markets {
                if let Some(info) = m.platforms.get(&Platform::Kalshi) {
                    subs_map.insert(info.platform_market_id.clone(), m.unified_id);
                }
            }
        }
        Self {
            config,
            auth: auth.map(Arc::new),
            db,
            subscriptions: subs_map,
            books: std::collections::HashMap::new(),
        }
    }

    fn handle_message(&mut self, text: &str, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let msg: serde_json::Value = serde_json::from_str(text)?;
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "orderbook_snapshot" => self.handle_snapshot(&msg, tick_tx),
            "orderbook_delta" => self.handle_delta(&msg, tick_tx),
            "error" => {
                let err_msg = msg.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown error");
                anyhow::bail!("Kalshi WS error: {}", err_msg);
            }
            _ => Ok(()),
        }
    }

    fn handle_snapshot(&mut self, msg: &serde_json::Value, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let ticker = msg.get("market_ticker").and_then(|v| v.as_str()).context("missing ticker")?;
        let book = self.books.entry(ticker.to_string()).or_insert_with(KalshiOrderBook::new);
        
        book.bids.clear();
        book.asks.clear();

        if let Some(bids) = msg.get("bids").and_then(|v| v.as_array()) {
            for b in bids {
                let p = Decimal::from_str(b.get(0).and_then(|v| v.as_str()).unwrap_or("0"))?;
                let s = Decimal::from_str(b.get(1).and_then(|v| v.as_str()).unwrap_or("0"))?;
                if s > Decimal::ZERO { book.bids.insert(p, s); }
            }
        }

        if let Some(asks) = msg.get("asks").and_then(|v| v.as_array()) {
            for a in asks {
                let p = Decimal::from_str(a.get(0).and_then(|v| v.as_str()).unwrap_or("0"))?;
                let s = Decimal::from_str(a.get(1).and_then(|v| v.as_str()).unwrap_or("0"))?;
                if s > Decimal::ZERO { book.asks.insert(p, s); }
            }
        }

        book.is_initialized = true;
        if let Some(tick) = self.create_tick(ticker) {
            let _ = tick_tx.send(tick);
        }
        Ok(())
    }

    fn handle_delta(&mut self, msg: &serde_json::Value, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let ticker = msg.get("market_ticker").and_then(|v| v.as_str()).context("missing ticker")?;
        let book = self.books.entry(ticker.to_string()).or_insert_with(KalshiOrderBook::new);
        
        if !book.is_initialized { return Ok(()); }

        let side = msg.get("side").and_then(|v| v.as_str()).context("missing side")?;
        let p = Decimal::from_str(msg.get("price").and_then(|v| v.as_str()).unwrap_or("0"))?;
        let s = Decimal::from_str(msg.get("delta").and_then(|v| v.as_str()).unwrap_or("0"))?;

        let map = if side == "yes" { &mut book.bids } else { &mut book.asks };
        if s == Decimal::ZERO {
            map.remove(&p);
        } else {
            map.insert(p, s);
        }

        if let Some(tick) = self.create_tick(ticker) {
            let _ = tick_tx.send(tick);
        }
        Ok(())
    }

    fn create_tick(&self, ticker: &str) -> Option<NormalizedTick> {
        let market_id = *self.subscriptions.get(ticker)?;
        let book = self.books.get(ticker)?;
        
        let bid = book.bids.iter().next_back().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ZERO, Decimal::ZERO));
        let ask = book.asks.iter().next().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ZERO, Decimal::ZERO));
        
        if bid.0 == Decimal::ZERO || ask.0 == Decimal::ZERO { return None; }

        let mid = (bid.0 + ask.0) / Decimal::TWO;
        let fee_bps = if ticker.contains("PRES") { 50 } else { 10 };

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
            book.is_initialized = false;
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        if self.auth.is_none() {
            tracing::error!("CRITICAL: Kalshi WebSocket requires authentication. Disabling feed because no credentials were provided in .env.");
            // Sleep forever to keep the task alive without spamming reconnects
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
            return Ok(());
        }
        info!("Connecting to Kalshi WebSocket");

        let mut request = self.config.ws_url.as_str()
            .into_client_request()
            .context("Invalid Kalshi WebSocket URL")?;
        
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::USER_AGENT,
            "MERCURY/0.1.0".try_into().unwrap(),
        );

        if let Some(auth) = &self.auth {
            let headers = auth.generate_ws_headers()?;
            for (key, val) in headers {
                use std::str::FromStr;
                request.headers_mut().insert(
                    tokio_tungstenite::tungstenite::http::header::HeaderName::from_str(&key).context("Invalid Kalshi Header")?,
                    tokio_tungstenite::tungstenite::http::header::HeaderValue::from_str(&val).context("Invalid Kalshi Header Value")?,
                );
            }
        }

        let tls = crate::crypto::tls::build_tls_connector()
            .context("Failed to build Kalshi TLS connector")?;
        
        let (ws_stream, _) = match connect_async_tls_with_config(request, None, false, Some(Connector::NativeTls(tls))).await {
            Ok(v) => v,
            Err(e) => {
                error!(error = %e, "Detailed Kalshi WebSocket connection failure");
                anyhow::bail!("Failed to connect to Kalshi WebSocket: {}", e);
            }
        };

        let (mut write, mut read) = ws_stream.split();

        let tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
        if !tickers.is_empty() {
            let sub = KalshiSubscribe {
                id: 1,
                cmd: "subscribe".into(),
                params: KalshiSubParams {
                    channels: vec!["orderbook_delta".into(), "trade".into()],
                    market_tickers: tickers.clone(),
                },
            };
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = tickers.len(), "Subscribed to Kalshi markets");
        }

        for ticker in self.subscriptions.keys() {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        // Periodically refresh subscriptions from DB to pick up newly discovered
        // 15-minute crypto markets without requiring a full WS reconnect.
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                msg_opt = read.next() => {
                    let msg_res = match msg_opt {
                        Some(m) => m,
                        None => break,
                    };
                    match msg_res {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                warn!(error = %e, "Failed to process Kalshi message");
                            }
                        }
                        Ok(Message::Ping(data)) => {
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Ok(Message::Close(_)) => {
                            info!("Kalshi WebSocket closed");
                            break;
                        }
                        Err(e) => return Err(e.into()),
                        _ => {}
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                        warn!(error = %e, "Failed to send Kalshi ping");
                        return Err(e.into());
                    }
                }
                _ = sync_interval.tick() => {
                    // Dynamically subscribe to newly discovered markets (e.g. fresh 15-min candles)
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
                                let sub = KalshiSubscribe {
                                    id: 2,
                                    cmd: "subscribe".into(),
                                    params: KalshiSubParams {
                                        channels: vec!["orderbook_delta".into(), "trade".into()],
                                        market_tickers: new_tickers.clone(),
                                    },
                                };
                                if let Ok(msg_text) = serde_json::to_string(&sub) {
                                    let _ = write.send(Message::Text(msg_text.into())).await;
                                    info!(count = new_tickers.len(), "Dynamically subscribed to new Kalshi markets");
                                }
                            }
                        }
                        Err(e) => warn!(error = %e, "Kalshi feed: DB refresh failed, skipping subscription update"),
                    }
                }
            }
        }

        Ok(())
    }
}

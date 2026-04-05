use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};
use tokio_tungstenite::Connector;
use tracing::{debug, info};
use uuid::Uuid;

use super::base::FeedHandler;
use super::common::LocalBookOps;
use crate::config::PolymarketConfig;
use crate::types::*;

pub struct PolymarketFeed {
    config: PolymarketConfig,
    db: std::sync::Arc<dyn crate::db::Database>,
    subscriptions: std::collections::HashMap<String, Uuid>,
    /// Maps YES token ID → market UUID for O(1) tick lookup
    yes_token_to_market: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, LocalOrderBook>,
    fee_rates: std::collections::HashMap<String, u16>,
    sequence: u64,
}

struct LocalOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_trade_price: Decimal,
    sequence: u64,
}

impl LocalOrderBook {
    fn new() -> Self {
        Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new(), last_trade_price: Decimal::ZERO, sequence: 0 }
    }

    fn apply_update(&mut self, side: &str, price: Decimal, size: Decimal) {
        let book = if side == "BUY" || side == "bid" { &mut self.bids } else { &mut self.asks };
        if size == Decimal::ZERO {
            book.remove(&price);
        } else {
            book.insert(price, size);
        }
    }

    #[allow(dead_code)]
    fn apply_snapshot(&mut self, bids: &[(Decimal, Decimal)], asks: &[(Decimal, Decimal)]) {
        self.bids.clear();
        self.asks.clear();
        for (p, s) in bids { self.bids.insert(*p, *s); }
        for (p, s) in asks { self.asks.insert(*p, *s); }
    }
}

impl super::common::LocalBookOps for LocalOrderBook {
    fn bids(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.bids }
    fn asks(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.asks }
}

#[derive(Deserialize)]
struct WsMessage {
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    market: String,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    bids: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    asks: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    changes: Option<Vec<BookChange>>,
    #[serde(default)]
    sequence: Option<u64>,
}

#[derive(Deserialize)]
struct PriceSizeEntry {
    price: String,
    size: String,
}

#[derive(Deserialize)]
struct BookChange {
    side: String,
    price: String,
    size: String,
}

#[derive(Serialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    assets_ids: Vec<String>,
}

impl PolymarketFeed {
    // CRITICAL FIX: Added `db` parameter and converted Vec to HashMap to match struct definition
    pub fn new(
        config: PolymarketConfig,
        db: std::sync::Arc<dyn crate::db::Database>,
        subscriptions: Vec<(String, Uuid, u16)>,
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        let mut fee_rates = std::collections::HashMap::new();
        for (asset_id, market_id, fee_bps) in subscriptions {
            subs_map.insert(asset_id.clone(), market_id);
            // Also key fee_rates by YES token so emit_tick can find them
            let ws_key = asset_id.split(',').next().unwrap_or(&asset_id).to_string();
            fee_rates.insert(ws_key, fee_bps);
        }
        // Build O(1) yes-token lookup map
        let mut yes_token_to_market = std::collections::HashMap::new();
        for (asset_id, &market_id) in &subs_map {
            let yes_token = asset_id.split(',').next().unwrap_or(asset_id).to_string();
            yes_token_to_market.insert(yes_token, market_id);
        }

        Self {
            config,
            db,
            subscriptions: subs_map,
            yes_token_to_market,
            books: std::collections::HashMap::new(),
            fee_rates,
            sequence: 0,
        }
    }

    fn asset_to_market_id(&self, asset_id: &str) -> Option<Uuid> {
        self.yes_token_to_market.get(asset_id).copied()
    }

    fn emit_tick(&self, asset_id: &str) -> Option<NormalizedTick> {
        let market_id = self.asset_to_market_id(asset_id)?;
        let book = self.books.get(asset_id)?;
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;
        let fee_bps = self.fee_rates.get(asset_id).copied().unwrap_or(200);

        Some(NormalizedTick {
            platform: Platform::Polymarket,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: book.last_trade_price,
            last_trade_size: Decimal::ZERO,
            book_depth: book.depth(),
            fee_rate_bps: fee_bps,
            sequence: book.sequence, // CRITICAL FIX: Pass the actual sequence counter
        })
    }
}

#[async_trait]
impl FeedHandler for PolymarketFeed {
    fn platform(&self) -> Platform {
        Platform::Polymarket
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
    }

async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to Polymarket WebSocket");

        let tls_connector = crate::crypto::tls::build_tls_connector()
            .context("Failed to build Polymarket TLS connector")?;
        let connector = Connector::NativeTls(tls_connector);
        let (ws_stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            connect_async_tls_with_config(
                url,
                None,
                false,
                Some(connector),
            )
        )
            .await
            .map_err(|_| anyhow::anyhow!("Polymarket WebSocket connect timed out after 15s"))?
            .context("Failed to connect to Polymarket WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // We only subscribe to the YES token (the first token in the comma-separated pair) 
        // because the spread engine automatically derives the NO price from the YES orderbook.
        let asset_ids: Vec<String> = self.subscriptions.iter()
            .map(|(a, _)| a.split(',').next().unwrap_or(a).to_string())
            .collect();
            
        if !asset_ids.is_empty() {
            let sub_msg = SubscribeMessage {
                msg_type: "subscribe".into(),
                assets_ids: asset_ids.clone(),
            };
            let msg_text = serde_json::to_string(&sub_msg)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = asset_ids.len(), "Subscribed to Polymarket markets");
        }

        for (asset_id, _) in &self.subscriptions {
            // WS events arrive keyed by the YES token (first element of comma pair).
            // Books must be keyed the same way or lookups will always miss.
            let ws_key = asset_id.split(',').next().unwrap_or(asset_id).to_string();
            self.books.entry(ws_key).or_insert_with(LocalOrderBook::new);
        }

        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
             tokio::select! {
                msg_timeout = tokio::time::timeout(std::time::Duration::from_secs(30), read.next()) => {
                    let msg_opt = match msg_timeout {
                        Ok(m) => m,
                        Err(_) => return Err(anyhow::anyhow!("No message for 30s — Polymarket heartbeat timeout")),
                    };
                    let msg = match msg_opt {
                        Some(m) => m,
                        None => continue,
                    };
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                let err_str = e.to_string();
                                if err_str.contains("sequence gap") {
                                    tracing::warn!(error = %e, "Polymarket sequence gap — reconnecting");
                                    return Err(e);
                                }
                                tracing::warn!(error = %e, "Failed to process Polymarket message");
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Ping(data)) => {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await;
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            info!("Polymarket WebSocket closed by server");
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                        _ => {}
                    }
                }
                _ = sync_interval.tick() => {
                    // CRITICAL FIX: Dynamically ingest newly discovered 15-min candles without rebooting
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_subs = Vec::new();
                        for m in markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Polymarket) {
                                let asset_id = info.platform_market_id.clone();
                                if !self.subscriptions.contains_key(&asset_id) {
                                    self.subscriptions.insert(asset_id.clone(), m.unified_id);
                                    
                                    // Extract the YES token so emit_tick can correctly look up the fee rate
                                    let ws_key = asset_id.split(',').next().unwrap_or(&asset_id).to_string();
                                    self.fee_rates.insert(ws_key.clone(), info.fee_rate_bps);
                                    self.yes_token_to_market.insert(ws_key.clone(), m.unified_id);
                                    new_subs.push(ws_key);
                                }
                            }
                        }
                        if !new_subs.is_empty() {
                            // Initialize local order books for new assets before subscribing
                            for ws_key in &new_subs {
                                self.books.entry(ws_key.clone()).or_insert_with(LocalOrderBook::new);
                                // Keep O(1) lookup map in sync with any newly added subscriptions
                                if let Some(&market_id) = self.subscriptions.values()
                                    .zip(self.subscriptions.keys())
                                    .find(|(_, k)| k.split(',').next().map(|t| t == ws_key).unwrap_or(false))
                                    .map(|(v, _)| v)
                                {
                                    self.yes_token_to_market.insert(ws_key.clone(), market_id);
                                }
                            }
                            let sub_msg = SubscribeMessage { msg_type: "subscribe".into(), assets_ids: new_subs.clone() };
                            if let Ok(msg_text) = serde_json::to_string(&sub_msg) {
                                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(msg_text.into())).await;
                                tracing::info!(count = new_subs.len(), "Dynamically subscribed to new Polymarket markets");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PolymarketPayload {
    Array(Vec<WsMessage>),
    Single(WsMessage),
}

impl PolymarketFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        match serde_json::from_str::<PolymarketPayload>(text) {
            Ok(PolymarketPayload::Array(messages)) => {
                for msg in messages {
                    self.process_event(&msg, tick_tx)?;
                }
            }
            Ok(PolymarketPayload::Single(msg)) => {
                self.process_event(&msg, tick_tx)?;
            }
            Err(e) => {
                tracing::debug!(error = %e, "Failed to parse Polymarket WS message");
            }
        }
        Ok(())
    }

    fn process_event(&mut self, msg: &WsMessage, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let asset_id = if !msg.asset_id.is_empty() {
            &msg.asset_id
        } else if !msg.market.is_empty() {
            &msg.market
        } else {
            return Ok(());
        };

        match msg.event_type.as_str() {
            "book" => {
                if let (Some(bids), Some(asks)) = (&msg.bids, &msg.asks) {
                    let seq_to_emit;
                    {
                        let book = self.books.entry(asset_id.to_string()).or_insert_with(LocalOrderBook::new);
                        book.bids.clear();
                        book.asks.clear();

                        for e in bids {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                book.bids.insert(p, s);
                            }
                        }
                        for e in asks {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                book.asks.insert(p, s);
                            }
                        }

                        // LOW-7 FIX: Properly handle book_snapshot sequence tracking
                        // Snapshots from Polymarket establish the base sequence. We accept it unconditionally
                        // because a snapshot means we reconnected and need to hard-reset our local tracker.
                        if let Some(msg_seq) = msg.sequence {
                            book.sequence = msg_seq;
                        } else {
                            book.sequence += 1;
                        }
                        seq_to_emit = book.sequence;
                    }  // mutable borrow ends

                    if let Some(mut tick) = self.emit_tick(asset_id) {
                        tick.sequence = seq_to_emit;
                        let _ = tick_tx.send(tick);
                    }
                }
            }
            "price_change" | "book_update" => {
                if let Some(changes) = &msg.changes {
                    let mut seq_to_emit = None;
                    {
                        let book = self.books.entry(asset_id.to_string()).or_insert_with(LocalOrderBook::new);
                        if let Some(msg_seq) = msg.sequence {
                            if msg_seq <= book.sequence && book.sequence > 0 { return Ok(()); }
                            if book.sequence > 0 && msg_seq > book.sequence + 1 {
                                return Err(anyhow::anyhow!("Polymarket sequence gap detected: expected {}, got {}", book.sequence + 1, msg_seq));
                            }
                            book.sequence = msg_seq;
                        } else {
                            book.sequence += 1;
                        }

                        for change in changes {
                            if let (Ok(p), Ok(s)) = (
                                Decimal::from_str(&change.price),
                                Decimal::from_str(&change.size),
                            ) {
                                book.apply_update(&change.side, p, s);
                            }
                        }
                        seq_to_emit = Some(book.sequence);
                    }  // mutable borrow ends

                    if let Some(seq) = seq_to_emit {
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = seq;
                            let _ = tick_tx.send(tick);
                        }
                    }
                }
            }
            "last_trade_price" => {
                if let Some(price_str) = &msg.price {
                    if let Ok(price) = Decimal::from_str(price_str) {
                        if let Some(book) = self.books.get_mut(asset_id) {
                            book.last_trade_price = price;
                        }
                    }
                }
            }
            _ => {
                debug!(event_type = %msg.event_type, "Unknown Polymarket event type");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "polymarket_tests.rs"]
mod polymarket_tests;

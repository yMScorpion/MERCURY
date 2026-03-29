use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};
use tokio_tungstenite::Connector;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::PolymarketConfig;
use crate::types::*;

pub struct PolymarketFeed {
    config: PolymarketConfig,
    subscriptions: Vec<(String, Uuid)>,
    books: std::collections::HashMap<String, LocalOrderBook>,
    fee_rates: std::collections::HashMap<String, u16>,
    sequence: u64,
}

struct LocalOrderBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
    last_trade_price: Decimal,
}

impl LocalOrderBook {
    fn new() -> Self {
        Self { bids: BTreeMap::new(), asks: BTreeMap::new(), last_trade_price: Decimal::ZERO }
    }

    /// Returns best bid only if non-empty — never returns phantom (0, 0) fallback.
    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s))
    }

    /// Returns best ask only if non-empty — never returns phantom (1.0, 0) fallback.
    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(p, s)| (*p, *s))
    }

    /// Returns mid-price only when both sides have real liquidity.
    fn mid_price(&self) -> Option<Decimal> {
        let (bid, _) = self.best_bid()?;
        let (ask, _) = self.best_ask()?;
        Some((bid + ask) / Decimal::from(2))
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::with_capacity(20);
        for (price, size) in self.bids.iter().rev().take(10) {
            levels.push(PriceLevel { price: *price, size: *size });
        }
        for (price, size) in self.asks.iter().take(10) {
            levels.push(PriceLevel { price: *price, size: *size });
        }
        levels
    }

    fn apply_update(&mut self, side: &str, price: Decimal, size: Decimal) {
        let book = if side == "BUY" || side == "bid" { &mut self.bids } else { &mut self.asks };
        if size == Decimal::ZERO {
            book.remove(&price);
        } else {
            book.insert(price, size);
        }
    }

    fn apply_snapshot(&mut self, bids: &[(Decimal, Decimal)], asks: &[(Decimal, Decimal)]) {
        self.bids.clear();
        self.asks.clear();
        for (p, s) in bids { self.bids.insert(*p, *s); }
        for (p, s) in asks { self.asks.insert(*p, *s); }
    }
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
    pub fn new(config: PolymarketConfig, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            subscriptions,
            books: std::collections::HashMap::new(),
            fee_rates: std::collections::HashMap::new(),
            sequence: 0,
        }
    }

    fn asset_to_market_id(&self, asset_id: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(a, _)| a == asset_id)
            .map(|(_, id)| *id)
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
            sequence: 0,
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

        let tls_connector = native_tls::TlsConnector::builder()
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .context("Failed to build TLS connector")?;
        let connector = Connector::NativeTls(tls_connector);
        let (ws_stream, _) = connect_async_tls_with_config(
            url,
            None, // WebSocket config
            false, // disable_nagle
            Some(connector),
        )
            .await
            .context("Failed to connect to Polymarket WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        let asset_ids: Vec<String> = self.subscriptions.iter().map(|(a, _)| a.clone()).collect();
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
            self.books.entry(asset_id.clone()).or_insert_with(LocalOrderBook::new);
        }

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_message(&text, &tick_tx) {
                        warn!(error = %e, "Failed to process Polymarket message");
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    info!("Polymarket WebSocket closed by server");
                    return Ok(());
                }
                Err(e) => {
                    return Err(e).context("Polymarket WebSocket error");
                }
                _ => {}
            }
        }

        Ok(())
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
                    self.process_event(&msg, tick_tx);
                }
            }
            Ok(PolymarketPayload::Single(msg)) => {
                self.process_event(&msg, tick_tx);
            }
            Err(e) => {
                tracing::debug!(error = %e, "Failed to parse Polymarket WS message");
            }
        }
        Ok(())
    }

    fn process_event(&mut self, msg: &WsMessage, tick_tx: &broadcast::Sender<NormalizedTick>) {
        let asset_id = if !msg.asset_id.is_empty() {
            &msg.asset_id
        } else if !msg.market.is_empty() {
            &msg.market
        } else {
            return;
        };

        match msg.event_type.as_str() {
            "book" => {
                if let (Some(bids), Some(asks)) = (&msg.bids, &msg.asks) {
                    if let Some(book) = self.books.get_mut(asset_id) {
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
                        
                        self.sequence += 1;
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = self.sequence;
                            let _ = tick_tx.send(tick);
                        }
                    }
                }
            }
            "price_change" | "book_update" => {
                if let Some(changes) = &msg.changes {
                    if let Some(book) = self.books.get_mut(asset_id) {
                        for change in changes {
                            if let (Ok(p), Ok(s)) = (
                                Decimal::from_str(&change.price),
                                Decimal::from_str(&change.size),
                            ) {
                                book.apply_update(&change.side, p, s);
                            }
                        }
                        self.sequence += 1;
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = self.sequence;
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
    }
}

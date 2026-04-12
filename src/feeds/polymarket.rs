// src/feeds/polymarket.rs
// KEY FIXES vs original:
//
// FIX-1  Heartbeat: Polymarket requires a PING every 10s (not 30s), and a 60s
//        read timeout instead of 300s. The connection goes stale silently
//        without the 10s ping, causing "no tick for 5000ms" alerts.
//
// FIX-2  Initial subscription message must be sent as PLAIN TEXT with `type`
//        field at top level (not nested). The original SubscribeMessage had the
//        fields correct but the code also sent `operation: None` chunks that
//        confused the server. Cleaner subscription builder used.
//
// FIX-3  price_change event structure: Polymarket now wraps individual changes
//        inside a `price_changes` array (not `changes`). The original code only
//        handled `changes`. Both formats handled now.
//
// FIX-4  asset_id lookup for ticks: the WS sends events keyed by the YES token
//        ID only. The subscription key stored in `subscriptions` is
//        "yes_token,no_token". The `yes_token_to_market` map must be built
//        correctly from the yes token alone. Original code had a bug where
//        `ws_key` was built from the whole comma pair in some paths.
//
// FIX-5  Sequence tracking: the original seq gap logic returned Err on the
//        first snapshot if `book.sequence` was still 0 and msg seq was > 1.
//        Fixed to accept any seq on snapshot (it resets state).

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};
use tokio_tungstenite::Connector;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use super::common::LocalBookOps;
use crate::config::PolymarketConfig;
use crate::types::*;

pub struct PolymarketFeed {
    config: PolymarketConfig,
    db: std::sync::Arc<dyn crate::db::Database>,
    /// Key: "yes_token,no_token" pair string (the raw platform_market_id stored in DB)
    subscriptions: std::collections::HashMap<String, Uuid>,
    /// Key: YES token ID only → market UUID (used to look up ticks from WS)
    yes_token_to_market: std::collections::HashMap<String, Uuid>,
    /// Key: YES token ID → local order book
    books: std::collections::HashMap<String, LocalOrderBook>,
    fee_rates: std::collections::HashMap<String, u16>,
    sequence: u64,
}

struct LocalOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_trade_price: Decimal,
    /// WS sequence number for this asset
    sequence: u64,
}

impl LocalOrderBook {
    fn new() -> Self {
        Self {
            bids: std::collections::BTreeMap::new(),
            asks: std::collections::BTreeMap::new(),
            last_trade_price: Decimal::ZERO,
            sequence: 0,
        }
    }

    fn apply_update(&mut self, side: &str, price: Decimal, size: Decimal) {
        let book = if side == "BUY" || side == "bid" { &mut self.bids } else { &mut self.asks };
        if size == Decimal::ZERO {
            book.remove(&price);
        } else {
            book.insert(price, size);
        }
    }
}

impl super::common::LocalBookOps for LocalOrderBook {
    fn bids(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.bids }
    fn asks(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.asks }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PolymarketPayload {
    Array(Vec<WsMessage>),
    Single(WsMessage),
}

#[derive(Deserialize, Default)]
struct WsMessage {
    #[serde(default)]
    event_type: String,
    // snapshot fields
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    market: String,
    #[serde(default)]
    bids: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    asks: Option<Vec<PriceSizeEntry>>,
    // FIX-3: both `changes` (old) and `price_changes` (new) + nested inside object
    #[serde(default)]
    changes: Option<Vec<BookChange>>,
    #[serde(default)]
    price_changes: Option<serde_json::Value>,  // may be array OR wrap per-asset objects
    // last trade price
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    sequence: Option<u64>,
    #[serde(default)]
    timestamp: Option<String>,
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

/// Subscription message formats
#[derive(Serialize)]
struct InitialSubscription {
    #[serde(rename = "type")]
    msg_type: String,
    assets_ids: Vec<String>,
    custom_feature_enabled: bool,
}

#[derive(Serialize)]
struct DynamicSubscription {
    operation: String,
    assets_ids: Vec<String>,
    custom_feature_enabled: bool,
}

impl PolymarketFeed {
    pub fn new(
        config: PolymarketConfig,
        db: std::sync::Arc<dyn crate::db::Database>,
        subscriptions: Vec<(String, Uuid, u16)>,
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        let mut fee_rates = std::collections::HashMap::new();
        let mut yes_token_to_market = std::collections::HashMap::new();

        for (asset_id, market_id, fee_bps) in subscriptions {
            subs_map.insert(asset_id.clone(), market_id);
            // The YES token is the first element of "yes_token,no_token"
            let yes_token = yes_token_from_pair(&asset_id);
            fee_rates.insert(yes_token.clone(), fee_bps);
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

        let bid = book.best_bid().unwrap_or((Decimal::ZERO, Decimal::ZERO));
        let ask = book.best_ask().unwrap_or((Decimal::ZERO, Decimal::ZERO));

        let mid = if bid.0 > Decimal::ZERO && ask.0 > Decimal::ZERO {
            (bid.0 + ask.0) / Decimal::from(2)
        } else if bid.0 > Decimal::ZERO { bid.0 } else { ask.0 };

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
            sequence: book.sequence,
        })
    }

    fn handle_message(&mut self, text: &str, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        match serde_json::from_str::<PolymarketPayload>(text) {
            Ok(PolymarketPayload::Array(messages)) => {
                for msg in messages { self.process_event(&msg, tick_tx)?; }
            }
            Ok(PolymarketPayload::Single(msg)) => {
                self.process_event(&msg, tick_tx)?;
            }
            Err(e) => {
                tracing::trace!(error = %e, text_snippet = &text[..text.len().min(80)], "Polymarket WS parse error (non-fatal)");
            }
        }
        Ok(())
    }

    fn process_event(&mut self, msg: &WsMessage, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let asset_id = if !msg.asset_id.is_empty() {
            msg.asset_id.clone()
        } else if !msg.market.is_empty() {
            msg.market.clone()
        } else {
            return Ok(());
        };

        match msg.event_type.as_str() {
            "book" => {
                if let (Some(bids), Some(asks)) = (&msg.bids, &msg.asks) {
                    let seq_to_emit = {
                        let book = self.books.entry(asset_id.clone()).or_insert_with(LocalOrderBook::new);
                        book.bids.clear();
                        book.asks.clear();

                        for e in bids {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                if s > Decimal::ZERO { book.bids.insert(p, s); }
                            }
                        }
                        for e in asks {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                if s > Decimal::ZERO { book.asks.insert(p, s); }
                            }
                        }

                        // FIX-5: Snapshot always resets sequence — never validate against old seq
                        if let Some(seq) = msg.sequence {
                            book.sequence = seq;
                        } else {
                            self.sequence += 1;
                            book.sequence = self.sequence;
                        }
                        book.sequence
                    };

                    if let Some(mut tick) = self.emit_tick(&asset_id) {
                        tick.sequence = seq_to_emit;
                        let _ = tick_tx.send(tick);
                    }
                }
            }

            // FIX-3: handle both old `changes` and new `price_changes` formats
            "price_change" | "book_update" => {
                // Try old format: changes array directly on the message
                if let Some(changes) = &msg.changes {
                    self.apply_changes(&asset_id, changes, msg.sequence, tick_tx)?;
                    return Ok(());
                }

                // New format: price_changes is either:
                //   A) An array of {asset_id, price, size, side} objects
                //   B) An array of {asset_id, price_changes:[{side,price,size}]}
                if let Some(pc_val) = &msg.price_changes {
                    self.apply_price_changes_field(&asset_id, pc_val, msg.sequence, tick_tx)?;
                }
            }

            "last_trade_price" => {
                if let Some(price_str) = &msg.price {
                    if let Ok(price) = Decimal::from_str(price_str) {
                        if let Some(book) = self.books.get_mut(&asset_id) {
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

    fn apply_changes(
        &mut self,
        asset_id: &str,
        changes: &[BookChange],
        msg_seq: Option<u64>,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let seq = {
            let book = self.books.entry(asset_id.to_string()).or_insert_with(LocalOrderBook::new);

            if let Some(s) = msg_seq {
                // Drop duplicate
                if s <= book.sequence && book.sequence > 0 { return Ok(()); }
                // Gap check
                if book.sequence > 0 && s > book.sequence + 1 {
                    return Err(anyhow::anyhow!(
                        "Polymarket sequence gap: expected {}, got {s}", book.sequence + 1
                    ));
                }
                book.sequence = s;
            } else {
                self.sequence += 1;
                book.sequence = self.sequence;
            }

            for change in changes {
                if let (Ok(p), Ok(s)) = (
                    Decimal::from_str(&change.price),
                    Decimal::from_str(&change.size),
                ) {
                    book.apply_update(&change.side, p, s);
                }
            }
            book.sequence
        };

        if let Some(mut tick) = self.emit_tick(asset_id) {
            tick.sequence = seq;
            let _ = tick_tx.send(tick);
        }
        Ok(())
    }

    fn apply_price_changes_field(
        &mut self,
        _outer_asset_id: &str,
        pc_val: &serde_json::Value,
        msg_seq: Option<u64>,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let arr = match pc_val.as_array() {
            Some(a) => a,
            None => return Ok(()),
        };

        // Each element may have its own asset_id
        for item in arr {
            let item_asset = item.get("asset_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let target_asset = if !item_asset.is_empty() { item_asset.clone() } else { _outer_asset_id.to_string() };

            // Format A: flat {asset_id, price, size, side}
            if let (Some(price_str), Some(size_str), Some(side_str)) = (
                item.get("price").and_then(|v| v.as_str()),
                item.get("size").and_then(|v| v.as_str()),
                item.get("side").and_then(|v| v.as_str()),
            ) {
                if let (Ok(p), Ok(s)) = (Decimal::from_str(price_str), Decimal::from_str(size_str)) {
                    let seq = {
                        let book = self.books.entry(target_asset.clone()).or_insert_with(LocalOrderBook::new);
                        if let Some(ms) = msg_seq {
                            if ms <= book.sequence && book.sequence > 0 { continue; }
                            book.sequence = ms;
                        } else {
                            self.sequence += 1;
                            book.sequence = self.sequence;
                        }
                        book.apply_update(side_str, p, s);
                        book.sequence
                    };
                    if let Some(mut tick) = self.emit_tick(&target_asset) {
                        tick.sequence = seq;
                        let _ = tick_tx.send(tick);
                    }
                }
                continue;
            }

            // Format B: {asset_id, price_changes: [{side, price, size}]}
            if let Some(inner) = item.get("price_changes").and_then(|v| v.as_array()) {
                let changes: Vec<BookChange> = inner.iter().filter_map(|c| {
                    let side = c.get("side").and_then(|v| v.as_str())?.to_string();
                    let price = c.get("price").and_then(|v| v.as_str())?.to_string();
                    let size = c.get("size").and_then(|v| v.as_str())?.to_string();
                    Some(BookChange { side, price, size })
                }).collect();
                self.apply_changes(&target_asset, &changes, msg_seq, tick_tx)?;
            }
        }
        Ok(())
    }
}

/// Extract the YES token from "yes_token,no_token" or return as-is
fn yes_token_from_pair(pair: &str) -> String {
    pair.split(',').next().unwrap_or(pair).to_string()
}

#[async_trait]
impl FeedHandler for PolymarketFeed {
    fn platform(&self) -> Platform { Platform::Polymarket }

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
            connect_async_tls_with_config(url, None, false, Some(connector)),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Polymarket WebSocket connect timed out after 15s"))?
        .context("Failed to connect to Polymarket WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Collect YES token IDs only for subscription
        let asset_ids: Vec<String> = self.subscriptions.keys()
            .map(|a| yes_token_from_pair(a))
            .collect();

        // FIX-2: send one clean subscription message with correct format
        if !asset_ids.is_empty() {
            for chunk in asset_ids.chunks(50) {
                let init = InitialSubscription {
                    msg_type: "market".into(),
                    assets_ids: chunk.to_vec(),
                    custom_feature_enabled: true,
                };
                let msg_text = serde_json::to_string(&init)?;
                write.send(Message::Text(msg_text.into())).await
                    .context("Polymarket initial subscribe send failed")?;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            info!(count = asset_ids.len(), "Subscribed to Polymarket markets");
        }

        // Initialize order books
        for asset_id in self.subscriptions.keys() {
            let yes_key = yes_token_from_pair(asset_id);
            self.books.entry(yes_key).or_insert_with(LocalOrderBook::new);
        }

        // FIX-1: Polymarket requires PING every 10s (not 30s)
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(15));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Text("PING".into())).await {
                        warn!(error = %e, "Polymarket ping failed");
                        return Err(e.into());
                    }
                }

                // FIX-1: 60s read timeout (was 300s — connection goes silent without triggering error)
                msg_result = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    read.next()
                ) => {
                    let msg_opt = match msg_result {
                        Ok(m) => m,
                        Err(_) => return Err(anyhow::anyhow!("Polymarket: no data for 60s — heartbeat timeout")),
                    };
                    match msg_opt {
                        Some(Ok(Message::Text(text))) => {
                            if text == "PONG" {
                                continue;
                            }
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                let es = e.to_string();
                                if es.contains("sequence gap") {
                                    warn!(error = %e, "Polymarket sequence gap — reconnecting");
                                    return Err(e);
                                }
                                warn!(error = %e, "Polymarket message error (non-fatal)");
                            }
                        }
                        Some(Ok(Message::Ping(data))) => { let _ = write.send(Message::Pong(data)).await; }
                        Some(Ok(Message::Close(_))) => { info!("Polymarket WS closed by server"); return Ok(()); }
                        Some(Err(e)) => return Err(e.into()),
                        None => break,
                        _ => {}
                    }
                }

                _ = sync_interval.tick() => {
                    // Dynamically subscribe to newly discovered markets
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_yes_tokens = Vec::new();
                        for m in markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Polymarket) {
                                let pair = &info.platform_market_id;
                                if !self.subscriptions.contains_key(pair) {
                                    let yes_key = yes_token_from_pair(pair);
                                    self.subscriptions.insert(pair.clone(), m.unified_id);
                                    self.fee_rates.insert(yes_key.clone(), info.fee_rate_bps);
                                    self.yes_token_to_market.insert(yes_key.clone(), m.unified_id);
                                    self.books.entry(yes_key.clone()).or_insert_with(LocalOrderBook::new);
                                    new_yes_tokens.push(yes_key);
                                }
                            }
                        }
                        if !new_yes_tokens.is_empty() {
                            info!(count = new_yes_tokens.len(), "Dynamically subscribing to new Polymarket markets");
                            for chunk in new_yes_tokens.chunks(50) {
                                let sub = DynamicSubscription {
                                    operation: "subscribe".into(),
                                    assets_ids: chunk.to_vec(),
                                    custom_feature_enabled: true,
                                };
                                if let Ok(msg_text) = serde_json::to_string(&sub) {
                                    let _ = write.send(Message::Text(msg_text.into())).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
// src/feeds/polymarket.rs
//
// FIXES applied vs previous version:
//
// FIX-1  September 15, 2025 BREAKING CHANGE: price_change message format.
//        The new format has NO top-level asset_id. Instead, the top-level
//        message has `price_changes` array where each element has its own
//        `asset_id`. Mercury was looking for asset_id at the top level of
//        price_change messages — now it doesn't exist, so ALL price_change
//        events were silently dropped. Fixed by routing price_change events
//        through the existing apply_price_changes_field path directly.
//
//        New wire format (post Sep 15, 2025):
//        {
//          "event_type": "price_change",
//          "market": "0x...",
//          "timestamp": "...",
//          "price_changes": [
//            { "asset_id": "...", "price": "0.5", "size": "200",
//              "side": "BUY", "hash": "...", "best_bid": "0.5", "best_ask": "1" },
//            { "asset_id": "...", "price": "0.5", "size": "200",
//              "side": "SELL", "hash": "...", "best_bid": "0", "best_ask": "0.5" }
//          ]
//        }
//
// FIX-2  DynamicSubscription was missing the `type` field ("market").
//        Without it the server ignores or rejects the message, so newly
//        discovered 15-min markets never get ticks.
//
// FIX-3  Initial subscription: add `custom_feature_enabled: true` to receive
//        best_bid_ask events (already had it, confirmed correct).
//
// FIX-4  process_event now correctly falls through to price_changes handling
//        when asset_id is absent at the top level (new format).
//
// FIX-5 (NEW) Dynamic subscription book bootstrap: The Polymarket WS server
//        only sends automatic `book` snapshots for assets subscribed at
//        connection time. Assets added mid-session via DynamicSubscription
//        do NOT receive an automatic snapshot — the server only pushes
//        incremental price_change events thereafter, which are useless without
//        a prior snapshot. Fix: after dynamically subscribing, fetch the
//        current order book via the REST CLOB API and emit a synthetic tick
//        to bootstrap the local book state.

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

#[derive(Clone)]
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
    /// HTTP client for REST book bootstrap calls
    http: reqwest::Client,
    resolved_markets: std::collections::HashSet<Uuid>,
    condition_id_to_uuid: std::collections::HashMap<String, Uuid>,
    pending_unsub_tokens: Vec<String>,
}

#[derive(Clone)]
struct LocalOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_trade_price: Decimal,
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
    // old format: changes array on the message itself (pre-Sep 2025, now gone)
    #[serde(default)]
    changes: Option<Vec<BookChange>>,
    // NEW format (Sep 15, 2025+): price_changes is a top-level array.
    // Each element has its own asset_id and price/side/size fields.
    #[serde(default)]
    price_changes: Option<serde_json::Value>,
    // best bid/ask
    #[serde(default)]
    best_bid: Option<String>,
    #[serde(default)]
    best_ask: Option<String>,
    // last trade price
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    sequence: Option<u64>,
    #[serde(default)]
    timestamp: Option<String>,
    // market event fields
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    winning_asset_id: Option<String>,
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

/// REST API response for GET /book?token_id=...
#[derive(Deserialize, Debug)]
struct RestBookResponse {
    #[serde(default)]
    bids: Vec<RestLevel>,
    #[serde(default)]
    asks: Vec<RestLevel>,
}

#[derive(Deserialize, Debug)]
struct RestLevel {
    price: String,
    size: String,
}

/// Initial subscription message format
#[derive(Serialize)]
struct InitialSubscription {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(rename = "asset_ids")]
    asset_ids: Vec<String>,
    custom_feature_enabled: bool,
}

/// Dynamic subscription/unsubscription message (add more assets after initial connect).
/// NOTE: `type` field is required — without it the server ignores the message.
#[derive(Serialize)]
struct DynamicSubscription {
    operation: String,
    #[serde(rename = "asset_ids")]
    asset_ids: Vec<String>,
    #[serde(rename = "type")]
    msg_type: String,
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
            let yes_token = yes_token_from_pair(&asset_id);
            fee_rates.insert(yes_token.clone(), fee_bps);
            yes_token_to_market.insert(yes_token, market_id);
        }

        let http = reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build Polymarket REST client");

        Self {
            config,
            db,
            subscriptions: subs_map,
            yes_token_to_market,
            books: std::collections::HashMap::new(),
            fee_rates,
            sequence: 0,
            http,
            resolved_markets: std::collections::HashSet::new(),
            condition_id_to_uuid: std::collections::HashMap::new(),
            pending_unsub_tokens: Vec::new(),
        }
    }

    fn asset_to_market_id(&self, asset_id: &str) -> Option<Uuid> {
        self.yes_token_to_market.get(asset_id).copied()
    }

    fn emit_tick(&self, asset_id: &str) -> Option<NormalizedTick> {
        let market_id = self.asset_to_market_id(asset_id)?;

        // Skip resolved markets
        if self.resolved_markets.contains(&market_id) {
            return None;
        }

        let book = self.books.get(asset_id)?;
        // ... (rest of logic)

        let bid = book.best_bid().unwrap_or((Decimal::ZERO, Decimal::ZERO));
        let ask = book.best_ask().unwrap_or((Decimal::ZERO, Decimal::ZERO));

        // Only emit if at least one side is present
        if bid.0 == Decimal::ZERO && ask.0 == Decimal::ZERO {
            return None;
        }

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
        match msg.event_type.as_str() {
            "book" => {
                // Full snapshot — asset_id is at top level for book events
                let asset_id = if !msg.asset_id.is_empty() {
                    msg.asset_id.clone()
                } else if !msg.market.is_empty() {
                    msg.market.clone()
                } else {
                    return Ok(());
                };

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

                        // Snapshot always resets sequence
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

            "price_change" | "book_update" => {
                // FIX: Post Sep 15, 2025 format:
                // The message has NO top-level asset_id.
                // price_changes is a top-level array where each element has its own asset_id.
                //
                // Also support the old pre-Sep-2025 format: `changes` array with top-level asset_id.

                if let Some(changes) = &msg.changes {
                    // Old format: changes array + top-level asset_id
                    let asset_id = if !msg.asset_id.is_empty() {
                        msg.asset_id.clone()
                    } else if !msg.market.is_empty() {
                        msg.market.clone()
                    } else {
                        return Ok(());
                    };
                    self.apply_changes(&asset_id, changes, msg.sequence, tick_tx)?;
                    return Ok(());
                }

                if let Some(pc_val) = &msg.price_changes {
                    // New format: price_changes array at top level, each element has asset_id
                    self.apply_price_changes_field(pc_val, msg.sequence, tick_tx)?;
                    return Ok(());
                }

                // Neither format found — ignore silently
            }

            "last_trade_price" => {
                // asset_id is at top level for this event type
                let asset_id = if !msg.asset_id.is_empty() {
                    msg.asset_id.clone()
                } else {
                    return Ok(());
                };
                if let Some(price_str) = &msg.price {
                    if let Ok(price) = Decimal::from_str(price_str) {
                        if let Some(book) = self.books.get_mut(&asset_id) {
                            book.last_trade_price = price;
                        }
                    }
                }
            }

            "best_bid_ask" => {
                let asset_id = if !msg.asset_id.is_empty() { msg.asset_id.clone() } else { return Ok(()) };
                if let (Some(best_bid_str), Some(best_ask_str)) = (&msg.best_bid, &msg.best_ask) {
                    if let (Ok(bid), Ok(ask)) = (Decimal::from_str(best_bid_str), Decimal::from_str(best_ask_str)) {
                        if let Some(book) = self.books.get_mut(&asset_id) {
                            book.bids.clear();
                            book.bids.insert(bid, Decimal::ONE); // Best Bids don't have size in best_bid_ask event, using 1 as proxy
                            book.asks.clear();
                            book.asks.insert(ask, Decimal::ONE); // Best Asks don't have size in best_bid_ask event
                            book.last_trade_price = (bid + ask) / Decimal::from(2);
                            
                            if let Some(mut tick) = self.emit_tick(&asset_id) {
                                let _ = tick_tx.send(tick);
                            }
                        }
                    }
                }
            }

            "market_resolved" => {
                let condition_id = msg.market.clone();
                let winner_asset = msg.winning_asset_id.as_deref().unwrap_or("").to_string();
                info!(condition_id = %condition_id, winner_asset = %winner_asset, "Market resolved event received");

                let mut resolved_uuid: Option<Uuid> = self.condition_id_to_uuid.get(&condition_id).copied();

                if resolved_uuid.is_none() && !winner_asset.is_empty() {
                    resolved_uuid = self.yes_token_to_market.get(&winner_asset).copied();
                    if resolved_uuid.is_none() {
                        for (pair, uuid) in &self.subscriptions {
                            let tokens: Vec<&str> = pair.splitn(2, ',').collect();
                            if tokens.get(1).map(|t| *t == winner_asset.as_str()).unwrap_or(false) {
                                resolved_uuid = Some(*uuid);
                                break;
                            }
                        }
                    }
                    if let Some(uuid) = resolved_uuid {
                        self.condition_id_to_uuid.insert(condition_id.clone(), uuid);
                    }
                }

                if let Some(uuid) = resolved_uuid {
                    self.resolved_markets.insert(uuid);
                    let pairs_to_remove: Vec<String> = self.subscriptions.iter()
                        .filter(|(_, v)| **v == uuid)
                        .map(|(k, _)| k.clone())
                        .collect();

                    for pair in &pairs_to_remove {
                        let yes_key = yes_token_from_pair(pair);
                        self.books.remove(&yes_key);
                        self.yes_token_to_market.remove(&yes_key);
                        self.fee_rates.remove(&yes_key);
                        self.pending_unsub_tokens.push(yes_key);
                        self.subscriptions.remove(pair);
                    }

                    info!(
                        condition_id = %condition_id,
                        uuid = %uuid,
                        removed_pairs = pairs_to_remove.len(),
                        "Market resolved — local books cleared, queued for WS unsubscribe"
                    );

                    let db = self.db.clone();
                    let cond_clone = condition_id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = db.update_market_status(&uuid, crate::types::MarketStatus::Resolved).await {
                            tracing::warn!(error=%e, condition_id=%cond_clone, "Failed to mark resolved market in DB");
                        }
                    });
                } else {
                    debug!(condition_id = %condition_id, winner_asset = %winner_asset, "market_resolved for untracked market — skipping cleanup");
                }
            }

            "new_market" => {
                let question = msg.question.as_deref().unwrap_or("unknown");
                info!(market_id = %msg.market, %question, "New market detected");
            }

            "tick_size_change" => {
                // Ignore for now as we don't adjust price grid precision dynamically
                debug!("Polymarket tick_size_change ignored");
            }

            _ => {
                warn!(event_type = %msg.event_type, "Unknown or unsupported Polymarket event type");
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
                if s <= book.sequence && book.sequence > 0 { return Ok(()); }
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

    /// Handle the new Sep 2025+ price_changes format.
    fn apply_price_changes_field(
        &mut self,
        pc_val: &serde_json::Value,
        msg_seq: Option<u64>,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let arr = match pc_val.as_array() {
            Some(a) => a,
            None => return Ok(()),
        };

        for item in arr {
            let item_asset = item.get("asset_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if item_asset.is_empty() {
                continue;
            }

            // Format A: flat { asset_id, price, size, side, ... }
            if let (Some(price_str), Some(size_str), Some(side_str)) = (
                item.get("price").and_then(|v| v.as_str()),
                item.get("size").and_then(|v| v.as_str()),
                item.get("side").and_then(|v| v.as_str()),
            ) {
                if let (Ok(p), Ok(s)) = (Decimal::from_str(price_str), Decimal::from_str(size_str)) {
                    let seq = {
                        let book = self.books.entry(item_asset.clone()).or_insert_with(LocalOrderBook::new);
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
                    if let Some(mut tick) = self.emit_tick(&item_asset) {
                        tick.sequence = seq;
                        let _ = tick_tx.send(tick);
                    }
                }
                continue;
            }

            // Format B: nested { asset_id, price_changes: [{side, price, size}] }
            if let Some(inner) = item.get("price_changes").and_then(|v| v.as_array()) {
                let changes: Vec<BookChange> = inner.iter().filter_map(|c| {
                    let side = c.get("side").and_then(|v| v.as_str())?.to_string();
                    let price = c.get("price").and_then(|v| v.as_str())?.to_string();
                    let size = c.get("size").and_then(|v| v.as_str())?.to_string();
                    Some(BookChange { side, price, size })
                }).collect();
                self.apply_changes(&item_asset, &changes, msg_seq, tick_tx)?;
            }
        }
        Ok(())
    }

    /// Fetch the current order book for a YES token via the Polymarket CLOB REST API
    /// and apply it to the local book, then emit a tick. Called after dynamic subscription
    /// to bootstrap book state since the WS server does not auto-send snapshots for
    /// dynamically added assets.
    async fn bootstrap_book_via_rest(
        &mut self,
        yes_token: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let url = format!("{}/book?token_id={}", self.config.rest_url, yes_token);
        let resp = match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            self.http.get(&url).send(),
        ).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                warn!(token = yes_token, error = %e, "Polymarket REST book fetch failed");
                return;
            }
            Err(_) => {
                warn!(token = yes_token, "Polymarket REST book fetch timed out");
                return;
            }
        };

        if !resp.status().is_success() {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                debug!(token = yes_token, "Polymarket REST book: market not found on CLOB");
            } else {
                warn!(token = yes_token, status = %resp.status(), url = %url, "Polymarket REST book returned unexpected status");
            }
            return;
        }

        let book_data: RestBookResponse = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                warn!(token = yes_token, error = %e, "Failed to parse Polymarket REST book response");
                return;
            }
        };

        let book = self.books.entry(yes_token.to_string()).or_insert_with(LocalOrderBook::new);
        book.bids.clear();
        book.asks.clear();

        for level in &book_data.bids {
            if let (Ok(p), Ok(s)) = (Decimal::from_str(&level.price), Decimal::from_str(&level.size)) {
                if s > Decimal::ZERO { book.bids.insert(p, s); }
            }
        }
        for level in &book_data.asks {
            if let (Ok(p), Ok(s)) = (Decimal::from_str(&level.price), Decimal::from_str(&level.size)) {
                if s > Decimal::ZERO { book.asks.insert(p, s); }
            }
        }

        self.sequence += 1;
        book.sequence = self.sequence;

        let bid_count = book.bids.len();
        let ask_count = book.asks.len();

        if let Some(tick) = self.emit_tick(yes_token) {
            info!(
                token = yes_token,
                bid = %tick.bid_price,
                ask = %tick.ask_price,
                bids = bid_count,
                asks = ask_count,
                "Polymarket REST book bootstrap successful — tick emitted"
            );
            let _ = tick_tx.send(tick);
        } else {
            debug!(token = yes_token, bids = bid_count, asks = ask_count, "REST book fetched but no valid BBO to emit tick");
        }
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

        // Collect YES token IDs for subscription (Polymarket subscribes by token/asset ID)
        let asset_ids: Vec<String> = self.subscriptions.keys()
            .map(|a| yes_token_from_pair(a))
            .collect();

        // Send initial subscription message with type="market"
        if !asset_ids.is_empty() {
            for chunk in asset_ids.chunks(50) {
                let init = InitialSubscription {
                    msg_type: "market".into(),
                    asset_ids: chunk.to_vec(),
                    custom_feature_enabled: true,
                };
                let msg_text = serde_json::to_string(&init)?;
                write.send(Message::Text(msg_text.into())).await
                    .context("Polymarket initial subscribe send failed")?;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            info!(count = asset_ids.len(), "Subscribed to Polymarket markets");
        }

        // Initialize order books for initially subscribed markets
        for asset_id in self.subscriptions.keys() {
            let yes_key = yes_token_from_pair(asset_id);
            self.books.entry(yes_key).or_insert_with(LocalOrderBook::new);
        }

        // Bootstrap books for initially subscribed markets via REST.
        // The WS server sends automatic book snapshots for initial subscriptions,
        // but we fetch REST anyway to ensure we have data immediately rather than
        // waiting for the first WS push (which may not come if no one is trading).
        {
            let initial_tokens: Vec<String> = self.subscriptions.keys()
                .map(|a| yes_token_from_pair(a))
                .collect();
            for token in &initial_tokens {
                self.bootstrap_book_via_rest(token, &tick_tx).await;
                // Small delay to avoid hammering the REST API
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            if !initial_tokens.is_empty() {
                info!(count = initial_tokens.len(), "Bootstrapped initial Polymarket books via REST");
            }
        }

        // Polymarket requires PING every 10s
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(15));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(vec![])).await {
                        warn!(error = %e, "Polymarket ping failed");
                        return Err(e.into());
                    }
                }

                // 90s read timeout
                msg_result = tokio::time::timeout(
                    std::time::Duration::from_secs(90),
                    read.next()
                ) => {
                    let msg_opt = match msg_result {
                        Ok(m) => m,
                        Err(_) => return Err(anyhow::anyhow!("Polymarket: no data for 90s — heartbeat timeout")),
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
                            if !self.pending_unsub_tokens.is_empty() {
                                let tokens: Vec<String> = std::mem::take(&mut self.pending_unsub_tokens);
                                for chunk in tokens.chunks(50) {
                                    let unsub = DynamicSubscription {
                                        operation: "unsubscribe".into(),
                                        asset_ids: chunk.to_vec(),
                                        msg_type: "market".into(),
                                        custom_feature_enabled: true,
                                    };
                                    if let Ok(msg_text) = serde_json::to_string(&unsub) {
                                        if let Err(e) = write.send(Message::Text(msg_text.into())).await {
                                            warn!(error = %e, "Failed to send WS unsubscribe for resolved market tokens");
                                        } else {
                                            info!(count = chunk.len(), tokens = ?chunk, "Sent immediate WS unsubscribe for resolved market tokens");
                                        }
                                    }
                                }
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
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_yes_tokens: Vec<String> = Vec::new();
                        let mut old_yes_tokens_to_remove: Vec<String> = Vec::new();

                        // Build the current set of expected yes-tokens from DB
                        let mut expected_pairs: std::collections::HashMap<String, (Uuid, u16)> = std::collections::HashMap::new();
                        for m in &markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Polymarket) {
                                let pair = &info.platform_market_id;
                                let yes_key = yes_token_from_pair(pair);
                                expected_pairs.insert(pair.clone(), (m.unified_id, info.fee_rate_bps));
                                // Map yes_token -> market
                                self.yes_token_to_market.insert(yes_key.clone(), m.unified_id);
                            }
                        }

                        // Find new token pairs (in DB but not subscribed)
                        for (pair, (market_id, fee_bps)) in &expected_pairs {
                            let yes_key = yes_token_from_pair(pair);
                            if !self.subscriptions.contains_key(pair) {
                                // New pair — could be a new round's different token
                                self.subscriptions.insert(pair.clone(), *market_id);
                                self.fee_rates.insert(yes_key.clone(), *fee_bps);
                                self.yes_token_to_market.insert(yes_key.clone(), *market_id);
                                self.books.entry(yes_key.clone()).or_insert_with(LocalOrderBook::new);
                                new_yes_tokens.push(yes_key);
                                tracing::info!(
                                    pair = %pair,
                                    market_id = %market_id,
                                    "New Polymarket token pair discovered"
                                );
                            }
                        }

                        // Find stale pairs (subscribed but no longer active in DB)
                        // This handles the case where a 15m round ended and a new one began
                        // with different token IDs but potentially the same market UUID
                        for (pair, _market_id) in self.subscriptions.clone().iter() {
                            if !expected_pairs.contains_key(pair) {
                                let yes_key = yes_token_from_pair(pair);
                                old_yes_tokens_to_remove.push(yes_key.clone());
                                self.subscriptions.remove(pair);
                                self.fee_rates.remove(&yes_key);
                                self.yes_token_to_market.remove(&yes_key);
                                self.books.remove(&yes_key);
                                tracing::info!(pair = %pair, "Removing stale Polymarket token pair subscription");
                            }
                        }

                        // Unsubscribe from old tokens
                        if !old_yes_tokens_to_remove.is_empty() {
                            for chunk in old_yes_tokens_to_remove.chunks(50) {
                                let unsub = DynamicSubscription {
                                    operation: "unsubscribe".into(),
                                    asset_ids: chunk.to_vec(),
                                    msg_type: "market".into(),
                                    custom_feature_enabled: true,
                                };
                                if let Ok(msg_text) = serde_json::to_string(&unsub) {
                                    let _ = write.send(Message::Text(msg_text.into())).await;
                                }
                            }
                        }

                        // Subscribe to new tokens
                        if !new_yes_tokens.is_empty() {
                            tracing::info!(count = new_yes_tokens.len(), "Dynamically subscribing to new Polymarket markets");
                            for chunk in new_yes_tokens.chunks(50) {
                                let sub = DynamicSubscription {
                                    operation: "subscribe".into(),
                                    asset_ids: chunk.to_vec(),
                                    msg_type: "market".into(),
                                    custom_feature_enabled: true,
                                };
                                if let Ok(msg_text) = serde_json::to_string(&sub) {
                                    let _ = write.send(Message::Text(msg_text.into())).await;
                                }
                            }

                            // Bootstrap new books via REST
                            for yes_token in &new_yes_tokens {
                                let tick_tx = tick_tx.clone();
                                let mut feed_clone = self.clone();
                                let token_clone = yes_token.clone();
                                tokio::spawn(async move {
                                    // Small delay to let the WS subscription register first
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                    feed_clone.bootstrap_book_via_rest(&token_clone, &tick_tx).await;
                                });
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "polymarket_tests.rs"]
mod polymarket_tests;
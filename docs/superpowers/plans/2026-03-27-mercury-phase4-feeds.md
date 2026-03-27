# Phase 4: Feed Handler Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build WebSocket feed handlers for all four platforms (Polymarket, Kalshi, CDNA, ForecastEx) with normalization into unified `NormalizedTick` events published via broadcast channel.

**Architecture:** Each feed handler runs as an independent tokio task, connecting via WebSocket (or TCP for FIX), processing platform-specific messages, and emitting `NormalizedTick` events to a shared `broadcast::Sender`. Automatic reconnection with exponential backoff on disconnect.

**Tech Stack:** tokio-tungstenite, reqwest, serde_json, tokio broadcast channels

**Depends on:** Phase 1 (types), Phase 3 (crypto for auth)

---

### Task 1: Feed Handler Base Trait

**Files:**
- Create: `src/feeds/mod.rs`
- Create: `src/feeds/base.rs`

- [ ] **Step 1: Write the FeedHandler trait and common reconnection logic**

`src/feeds/mod.rs`:
```rust
pub mod base;
pub mod polymarket;
pub mod kalshi;
pub mod cdna;
pub mod forecastex;
pub mod normalizer;
```

`src/feeds/base.rs`:
```rust
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::types::{NormalizedTick, Platform, PlatformHealth};

#[async_trait]
pub trait FeedHandler: Send + Sync + 'static {
    fn platform(&self) -> Platform;

    /// Connect and start processing. Should run until disconnected.
    /// Returns Err on fatal error, Ok(()) on clean disconnect.
    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()>;
}

/// Run a feed handler with automatic reconnection
pub async fn run_with_reconnect(
    mut handler: Box<dyn FeedHandler>,
    tick_tx: broadcast::Sender<NormalizedTick>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
) {
    let platform = handler.platform();
    let mut backoff_secs = 1u64;
    let max_backoff = 60u64;

    loop {
        info!(%platform, "Connecting feed handler");
        match handler.connect_and_run(tick_tx.clone()).await {
            Ok(()) => {
                info!(%platform, "Feed handler disconnected cleanly");
                backoff_secs = 1; // Reset on clean disconnect
            }
            Err(e) => {
                error!(%platform, error = %e, "Feed handler error");
                let _ = alert_tx.send(crate::types::AlertMessage::SystemAlert {
                    severity: "warning".into(),
                    message: format!("{} feed disconnected: {}", platform, e),
                }).await;
            }
        }

        warn!(%platform, backoff_secs, "Reconnecting after backoff");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/feeds/
git commit -m "feat: add FeedHandler trait with automatic reconnection logic"
```

---

### Task 2: Polymarket Feed Handler

**Files:**
- Create: `src/feeds/polymarket.rs`

- [ ] **Step 1: Write the Polymarket WebSocket feed handler**

`src/feeds/polymarket.rs`:
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::PolymarketConfig;
use crate::types::*;

pub struct PolymarketFeed {
    config: PolymarketConfig,
    /// Map of asset_id -> unified_market_id
    subscriptions: Vec<(String, Uuid)>,
    /// Local order book replicas: asset_id -> book
    books: std::collections::HashMap<String, LocalOrderBook>,
    /// Fee rate BPS per market (polled from REST API)
    fee_rates: std::collections::HashMap<String, u16>,
    /// API credentials (from env vars)
    api_key: Option<String>,
    api_secret: Option<String>,
    api_passphrase: Option<String>,
    sequence: u64,
}

struct LocalOrderBook {
    bids: BTreeMap<Decimal, Decimal>, // price -> size, descending
    asks: BTreeMap<Decimal, Decimal>, // price -> size, ascending
}

impl LocalOrderBook {
    fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    fn best_bid(&self) -> (Decimal, Decimal) {
        self.bids.iter().next_back()
            .map(|(p, s)| (*p, *s))
            .unwrap_or((Decimal::ZERO, Decimal::ZERO))
    }

    fn best_ask(&self) -> (Decimal, Decimal) {
        self.asks.iter().next()
            .map(|(p, s)| (*p, *s))
            .unwrap_or((Decimal::ONE, Decimal::ZERO))
    }

    fn mid_price(&self) -> Decimal {
        let (bid, _) = self.best_bid();
        let (ask, _) = self.best_ask();
        (bid + ask) / Decimal::from(2)
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
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
        for (p, s) in bids {
            self.bids.insert(*p, *s);
        }
        for (p, s) in asks {
            self.asks.insert(*p, *s);
        }
    }
}

// Polymarket WebSocket message types
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
    side: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    bids: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    asks: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    changes: Option<Vec<BookChange>>,
    #[serde(default)]
    last_trade_price: Option<String>,
    #[serde(default)]
    hash: Option<String>,
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
            api_key: std::env::var("POLYMARKET_API_KEY").ok(),
            api_secret: std::env::var("POLYMARKET_API_SECRET").ok(),
            api_passphrase: std::env::var("POLYMARKET_API_PASSPHRASE").ok(),
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
        let (bid_price, bid_size) = book.best_bid();
        let (ask_price, ask_size) = book.best_ask();
        let fee_bps = self.fee_rates.get(asset_id).copied().unwrap_or(200); // default 2%

        Some(NormalizedTick {
            platform: Platform::Polymarket,
            market_id,
            timestamp_ns: now_ns(),
            bid_price,
            bid_size,
            ask_price,
            ask_size,
            mid_price: book.mid_price(),
            last_trade_price: Decimal::ZERO, // Updated separately
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

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to Polymarket WebSocket");

        let (ws_stream, _) = connect_async(url)
            .await
            .context("Failed to connect to Polymarket WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to markets
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

        // Initialize local order books
        for (asset_id, _) in &self.subscriptions {
            self.books.entry(asset_id.clone()).or_insert_with(LocalOrderBook::new);
        }

        // Process messages
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

impl PolymarketFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        // Try parsing as array (Polymarket sends arrays of events)
        if let Ok(messages) = serde_json::from_str::<Vec<WsMessage>>(text) {
            for msg in messages {
                self.process_event(&msg, tick_tx);
            }
            return Ok(());
        }

        // Try single message
        if let Ok(msg) = serde_json::from_str::<WsMessage>(text) {
            self.process_event(&msg, tick_tx);
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
                // Full order book snapshot
                if let (Some(bids), Some(asks)) = (&msg.bids, &msg.asks) {
                    let bid_levels: Vec<(Decimal, Decimal)> = bids.iter()
                        .filter_map(|e| {
                            let p = Decimal::from_str(&e.price).ok()?;
                            let s = Decimal::from_str(&e.size).ok()?;
                            Some((p, s))
                        })
                        .collect();
                    let ask_levels: Vec<(Decimal, Decimal)> = asks.iter()
                        .filter_map(|e| {
                            let p = Decimal::from_str(&e.price).ok()?;
                            let s = Decimal::from_str(&e.size).ok()?;
                            Some((p, s))
                        })
                        .collect();

                    if let Some(book) = self.books.get_mut(asset_id) {
                        book.apply_snapshot(&bid_levels, &ask_levels);
                        self.sequence += 1;
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = self.sequence;
                            let _ = tick_tx.send(tick);
                        }
                    }
                }
            }
            "price_change" | "book_update" => {
                // Incremental update
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
                // Update last trade price
                if let Some(price_str) = &msg.price {
                    if let Ok(_price) = Decimal::from_str(price_str) {
                        // Stored in tick on next emission
                    }
                }
            }
            _ => {
                debug!(event_type = %msg.event_type, "Unknown Polymarket event type");
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/feeds/polymarket.rs
git commit -m "feat: add Polymarket CLOB WebSocket feed handler with order book"
```

---

### Task 3: Kalshi Feed Handler

**Files:**
- Create: `src/feeds/kalshi.rs`

- [ ] **Step 1: Write the Kalshi WebSocket feed handler**

`src/feeds/kalshi.rs`:
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::KalshiConfig;
use crate::crypto::jwt::KalshiAuth;
use crate::types::*;

pub struct KalshiFeed {
    config: KalshiConfig,
    auth: Option<KalshiAuth>,
    /// Map of kalshi_ticker -> unified_market_id
    subscriptions: Vec<(String, Uuid)>,
    /// Local order book replicas
    books: std::collections::HashMap<String, KalshiOrderBook>,
    sequence: u64,
}

struct KalshiOrderBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
    last_seq: u64,
}

impl KalshiOrderBook {
    fn new() -> Self {
        Self { bids: BTreeMap::new(), asks: BTreeMap::new(), last_seq: 0 }
    }

    fn best_bid(&self) -> (Decimal, Decimal) {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ZERO, Decimal::ZERO))
    }

    fn best_ask(&self) -> (Decimal, Decimal) {
        self.asks.iter().next().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ONE, Decimal::ZERO))
    }

    fn mid_price(&self) -> Decimal {
        let (b, _) = self.best_bid();
        let (a, _) = self.best_ask();
        (b + a) / Decimal::from(2)
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        for (p, s) in self.bids.iter().rev().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        for (p, s) in self.asks.iter().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        levels
    }
}

// Kalshi WebSocket message types
#[derive(Deserialize)]
struct KalshiWsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    msg: Option<serde_json::Value>,
    #[serde(default)]
    sid: Option<u64>,
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
    pub fn new(config: KalshiConfig, auth: Option<KalshiAuth>, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            auth,
            subscriptions,
            books: std::collections::HashMap::new(),
            sequence: 0,
        }
    }

    fn ticker_to_market_id(&self, ticker: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(t, _)| t == ticker)
            .map(|(_, id)| *id)
    }

    fn emit_tick(&self, ticker: &str) -> Option<NormalizedTick> {
        let market_id = self.ticker_to_market_id(ticker)?;
        let book = self.books.get(ticker)?;
        let (bid_price, bid_size) = book.best_bid();
        let (ask_price, ask_size) = book.best_ask();

        // Kalshi fee: 7c * C * (1-C) per contract
        // We report the fee_rate_bps based on current mid price
        let mid = book.mid_price();
        let fee_per_contract = Decimal::from_str("0.07").unwrap() * mid * (Decimal::ONE - mid);
        let fee_bps = if mid > Decimal::ZERO {
            ((fee_per_contract / mid) * Decimal::from(10000)).to_u64().unwrap_or(175) as u16
        } else {
            175 // max fee at 50c
        };

        Some(NormalizedTick {
            platform: Platform::Kalshi,
            market_id,
            timestamp_ns: now_ns(),
            bid_price,
            bid_size,
            ask_price,
            ask_size,
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

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let mut url = self.config.ws_url.clone();

        // Add auth token as query param if available
        if let Some(auth) = &self.auth {
            let token = auth.generate_token()?;
            url = format!("{}?token={}", url, token);
        }

        info!("Connecting to Kalshi WebSocket");
        let (ws_stream, _) = connect_async(&url)
            .await
            .context("Failed to connect to Kalshi WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to orderbook channels
        let tickers: Vec<String> = self.subscriptions.iter().map(|(t, _)| t.clone()).collect();
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

        // Initialize books
        for (ticker, _) in &self.subscriptions {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        while let Some(msg) = read.next().await {
            match msg {
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
                    return Ok(());
                }
                Err(e) => {
                    return Err(e).context("Kalshi WebSocket error");
                }
                _ => {}
            }
        }

        Ok(())
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
                    self.handle_orderbook_delta(&data, tick_tx);
                }
            }
            "trade" => {
                // Could update last_trade_price
            }
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
        data: &serde_json::Value,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let ticker = data.get("market_ticker").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(book) = self.books.get_mut(ticker) {
            book.bids.clear();
            book.asks.clear();

            if let Some(yes_bids) = data.get("yes").and_then(|v| v.as_array()) {
                for level in yes_bids {
                    if let (Some(p), Some(s)) = (
                        level.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        level.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                    ) {
                        // Kalshi prices are in cents (1-99), normalize to 0.01-0.99
                        let price = p / Decimal::from(100);
                        book.bids.insert(price, s);
                    }
                }
            }

            if let Some(no_asks) = data.get("no").and_then(|v| v.as_array()) {
                for level in no_asks {
                    if let (Some(p), Some(s)) = (
                        level.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        level.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                    ) {
                        let price = p / Decimal::from(100);
                        book.asks.insert(price, s);
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
        data: &serde_json::Value,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let ticker = data.get("market_ticker").and_then(|v| v.as_str()).unwrap_or("");

        // Check sequence for gaps
        let seq = data.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(book) = self.books.get(ticker) {
            if seq > 0 && book.last_seq > 0 && seq != book.last_seq + 1 {
                warn!(ticker, expected = book.last_seq + 1, got = seq, "Kalshi sequence gap, need resync");
                // TODO: trigger full book resync via REST API
            }
        }

        if let Some(book) = self.books.get_mut(ticker) {
            book.last_seq = seq;

            // Apply bid deltas
            if let Some(bid_deltas) = data.get("price_deltas").and_then(|v| v.get("yes")).and_then(|v| v.as_array()) {
                for delta in bid_deltas {
                    if let (Some(p), Some(s)) = (
                        delta.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        delta.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                    ) {
                        let price = p / Decimal::from(100);
                        if s == Decimal::ZERO {
                            book.bids.remove(&price);
                        } else {
                            book.bids.insert(price, s);
                        }
                    }
                }
            }

            // Apply ask deltas
            if let Some(ask_deltas) = data.get("price_deltas").and_then(|v| v.get("no")).and_then(|v| v.as_array()) {
                for delta in ask_deltas {
                    if let (Some(p), Some(s)) = (
                        delta.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        delta.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                    ) {
                        let price = p / Decimal::from(100);
                        if s == Decimal::ZERO {
                            book.asks.remove(&price);
                        } else {
                            book.asks.insert(price, s);
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
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/feeds/kalshi.rs
git commit -m "feat: add Kalshi WebSocket feed handler with order book and sequence tracking"
```

---

### Task 4: CDNA Feed Handler

**Files:**
- Create: `src/feeds/cdna.rs`

- [ ] **Step 1: Write the Crypto.com CDNA feed handler**

`src/feeds/cdna.rs`:
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::CdnaConfig;
use crate::types::*;

pub struct CdnaFeed {
    config: CdnaConfig,
    subscriptions: Vec<(String, Uuid)>, // instrument_name -> unified_id
    books: std::collections::HashMap<String, CdnaOrderBook>,
    sequence: u64,
    request_id: u64,
}

struct CdnaOrderBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
}

impl CdnaOrderBook {
    fn new() -> Self { Self { bids: BTreeMap::new(), asks: BTreeMap::new() } }
    fn best_bid(&self) -> (Decimal, Decimal) {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ZERO, Decimal::ZERO))
    }
    fn best_ask(&self) -> (Decimal, Decimal) {
        self.asks.iter().next().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ONE, Decimal::ZERO))
    }
    fn mid_price(&self) -> Decimal {
        let (b, _) = self.best_bid();
        let (a, _) = self.best_ask();
        if b + a > Decimal::ZERO { (b + a) / Decimal::from(2) } else { Decimal::ZERO }
    }
    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        for (p, s) in self.bids.iter().rev().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        for (p, s) in self.asks.iter().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        levels
    }
}

#[derive(Serialize)]
struct CdnaSubscribe {
    id: u64,
    method: String,
    params: CdnaSubParams,
}

#[derive(Serialize)]
struct CdnaSubParams {
    channels: Vec<String>,
}

impl CdnaFeed {
    pub fn new(config: CdnaConfig, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            subscriptions,
            books: std::collections::HashMap::new(),
            sequence: 0,
            request_id: 1,
        }
    }

    fn instrument_to_market_id(&self, instrument: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(i, _)| i == instrument)
            .map(|(_, id)| *id)
    }

    fn emit_tick(&self, instrument: &str) -> Option<NormalizedTick> {
        let market_id = self.instrument_to_market_id(instrument)?;
        let book = self.books.get(instrument)?;
        let (bid_price, bid_size) = book.best_bid();
        let (ask_price, ask_size) = book.best_ask();

        Some(NormalizedTick {
            platform: Platform::Cdna,
            market_id,
            timestamp_ns: now_ns(),
            bid_price,
            bid_size,
            ask_price,
            ask_size,
            mid_price: book.mid_price(),
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: book.depth(),
            fee_rate_bps: 150, // ~1.5% default for CDNA
            sequence: 0,
        })
    }
}

#[async_trait]
impl FeedHandler for CdnaFeed {
    fn platform(&self) -> Platform {
        Platform::Cdna
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to CDNA WebSocket");

        let (ws_stream, _) = connect_async(url)
            .await
            .context("Failed to connect to CDNA WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to order book channels
        let channels: Vec<String> = self.subscriptions.iter()
            .map(|(instrument, _)| format!("book.{}", instrument))
            .collect();

        if !channels.is_empty() {
            let sub = CdnaSubscribe {
                id: self.request_id,
                method: "subscribe".into(),
                params: CdnaSubParams { channels: channels.clone() },
            };
            self.request_id += 1;
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = channels.len(), "Subscribed to CDNA channels");
        }

        for (instrument, _) in &self.subscriptions {
            self.books.entry(instrument.clone()).or_insert_with(CdnaOrderBook::new);
        }

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_message(&text, &tick_tx) {
                        warn!(error = %e, "Failed to process CDNA message");
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    info!("CDNA WebSocket closed");
                    return Ok(());
                }
                Err(e) => return Err(e).context("CDNA WebSocket error"),
                _ => {}
            }
        }

        Ok(())
    }
}

impl CdnaFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let v: serde_json::Value = serde_json::from_str(text)?;

        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if method != "subscribe" {
            return Ok(());
        }

        let channel = v.get("result")
            .and_then(|r| r.get("channel"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if !channel.starts_with("book.") {
            return Ok(());
        }

        let instrument = &channel[5..]; // strip "book."

        if let Some(data) = v.get("result").and_then(|r| r.get("data")) {
            if let Some(book) = self.books.get_mut(instrument) {
                // Process bids
                if let Some(bids) = data.get("bids").and_then(|b| b.as_array()) {
                    for entry in bids {
                        if let (Some(p), Some(s)) = (
                            entry.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                            entry.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        ) {
                            if s == Decimal::ZERO { book.bids.remove(&p); } else { book.bids.insert(p, s); }
                        }
                    }
                }

                // Process asks
                if let Some(asks) = data.get("asks").and_then(|b| b.as_array()) {
                    for entry in asks {
                        if let (Some(p), Some(s)) = (
                            entry.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                            entry.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        ) {
                            if s == Decimal::ZERO { book.asks.remove(&p); } else { book.asks.insert(p, s); }
                        }
                    }
                }

                self.sequence += 1;
                if let Some(mut tick) = self.emit_tick(instrument) {
                    tick.sequence = self.sequence;
                    let _ = tick_tx.send(tick);
                }
            }
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/feeds/cdna.rs
git commit -m "feat: add Crypto.com CDNA WebSocket feed handler"
```

---

### Task 5: ForecastEx FIX Protocol Feed Handler

**Files:**
- Create: `src/feeds/forecastex.rs`

- [ ] **Step 1: Write the ForecastEx FIX 4.4 feed handler**

`src/feeds/forecastex.rs`:
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::ForecastExConfig;
use crate::types::*;

/// Minimal FIX 4.4 client for ForecastEx market data
pub struct ForecastExFeed {
    config: ForecastExConfig,
    subscriptions: Vec<(String, Uuid)>, // fix_symbol -> unified_id
    books: std::collections::HashMap<String, FexOrderBook>,
    sequence: u64,
    msg_seq_num: u64,
    sender_comp_id: String,
    target_comp_id: String,
}

struct FexOrderBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
}

impl FexOrderBook {
    fn new() -> Self { Self { bids: BTreeMap::new(), asks: BTreeMap::new() } }
    fn best_bid(&self) -> (Decimal, Decimal) {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ZERO, Decimal::ZERO))
    }
    fn best_ask(&self) -> (Decimal, Decimal) {
        self.asks.iter().next().map(|(p, s)| (*p, *s)).unwrap_or((Decimal::ONE, Decimal::ZERO))
    }
    fn mid_price(&self) -> Decimal {
        let (b, _) = self.best_bid();
        let (a, _) = self.best_ask();
        if b + a > Decimal::ZERO { (b + a) / Decimal::from(2) } else { Decimal::ZERO }
    }
    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        for (p, s) in self.bids.iter().rev().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        for (p, s) in self.asks.iter().take(10) {
            levels.push(PriceLevel { price: *p, size: *s });
        }
        levels
    }
}

const SOH: char = '\x01'; // FIX field delimiter

impl ForecastExFeed {
    pub fn new(config: ForecastExConfig, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            subscriptions,
            books: std::collections::HashMap::new(),
            sequence: 0,
            msg_seq_num: 1,
            sender_comp_id: std::env::var("FEX_SENDER_COMP_ID").unwrap_or_else(|_| "MERCURY".into()),
            target_comp_id: std::env::var("FEX_TARGET_COMP_ID").unwrap_or_else(|_| "FORECASTEX".into()),
        }
    }

    fn symbol_to_market_id(&self, symbol: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(s, _)| s == symbol)
            .map(|(_, id)| *id)
    }

    fn emit_tick(&self, symbol: &str) -> Option<NormalizedTick> {
        let market_id = self.symbol_to_market_id(symbol)?;
        let book = self.books.get(symbol)?;
        let (bid_price, bid_size) = book.best_bid();
        let (ask_price, ask_size) = book.best_ask();

        Some(NormalizedTick {
            platform: Platform::ForecastEx,
            market_id,
            timestamp_ns: now_ns(),
            bid_price,
            bid_size,
            ask_price,
            ask_size,
            mid_price: book.mid_price(),
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: book.depth(),
            fee_rate_bps: 0, // ForecastEx: spread-embedded, no explicit fee
            sequence: 0,
        })
    }

    fn build_fix_message(&mut self, msg_type: &str, body_fields: &[(u32, &str)]) -> String {
        let mut body = format!("35={}{}", msg_type, SOH);
        body.push_str(&format!("49={}{}", self.sender_comp_id, SOH));
        body.push_str(&format!("56={}{}", self.target_comp_id, SOH));
        body.push_str(&format!("34={}{}", self.msg_seq_num, SOH));
        body.push_str(&format!("52={}{}", chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f"), SOH));
        for (tag, val) in body_fields {
            body.push_str(&format!("{}={}{}", tag, val, SOH));
        }
        self.msg_seq_num += 1;

        let header = format!("8=FIX.4.4{}9={}{}", SOH, body.len(), SOH);
        let full = format!("{}{}", header, body);

        // Calculate checksum
        let checksum: u32 = full.bytes().map(|b| b as u32).sum::<u32>() % 256;
        format!("{}10={:03}{}", full, checksum, SOH)
    }

    fn parse_fix_fields(msg: &str) -> std::collections::HashMap<u32, String> {
        let mut fields = std::collections::HashMap::new();
        for part in msg.split(SOH) {
            if let Some(eq_pos) = part.find('=') {
                if let Ok(tag) = part[..eq_pos].parse::<u32>() {
                    fields.insert(tag, part[eq_pos + 1..].to_string());
                }
            }
        }
        fields
    }
}

#[async_trait]
impl FeedHandler for ForecastExFeed {
    fn platform(&self) -> Platform {
        Platform::ForecastEx
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        if !self.config.enabled {
            info!("ForecastEx feed disabled, skipping");
            // Sleep forever (will be cancelled by supervisor)
            tokio::time::sleep(std::time::Duration::from_secs(u64::MAX)).await;
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.fix_host, self.config.fix_port);
        info!(addr = %addr, "Connecting to ForecastEx FIX gateway");

        let stream = TcpStream::connect(&addr)
            .await
            .context("Failed to connect to ForecastEx FIX gateway")?;

        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);

        // Send Logon (35=A)
        let logon = self.build_fix_message("A", &[
            (98, "0"),    // EncryptMethod: None
            (108, "30"),  // HeartBtInt: 30 seconds
        ]);
        writer.write_all(logon.as_bytes()).await?;
        info!("FIX Logon sent");

        // Initialize books
        for (symbol, _) in &self.subscriptions {
            self.books.entry(symbol.clone()).or_insert_with(FexOrderBook::new);
        }

        // Subscribe to market data (35=V)
        for (i, (symbol, _)) in self.subscriptions.iter().enumerate() {
            let md_req_id = format!("MDR{}", i);
            let md_request = self.build_fix_message("V", &[
                (262, &md_req_id),   // MDReqID
                (263, "1"),          // SubscriptionRequestType: Snapshot + Updates
                (264, "10"),         // MarketDepth: 10 levels
                (267, "2"),          // NoMDEntryTypes: 2
                (269, "0"),          // MDEntryType: Bid
                (269, "1"),          // MDEntryType: Offer
                (146, "1"),          // NoRelatedSym: 1
                (55, symbol),        // Symbol
            ]);
            writer.write_all(md_request.as_bytes()).await?;
        }
        info!(count = self.subscriptions.len(), "FIX market data requests sent");

        // Process incoming messages
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match tokio::time::timeout(
                std::time::Duration::from_secs(45), // heartbeat + margin
                buf_reader.read_line(&mut line_buf),
            ).await {
                Ok(Ok(0)) => {
                    info!("FIX connection closed");
                    return Ok(());
                }
                Ok(Ok(_)) => {
                    let fields = Self::parse_fix_fields(&line_buf);
                    let msg_type = fields.get(&35).map(|s| s.as_str()).unwrap_or("");

                    match msg_type {
                        "W" | "X" => {
                            // Market Data Snapshot/Incremental Refresh
                            self.handle_market_data(&fields, &tick_tx);
                        }
                        "0" => {
                            // Heartbeat - send heartbeat back
                            let hb = self.build_fix_message("0", &[]);
                            let _ = writer.write_all(hb.as_bytes()).await;
                        }
                        "1" => {
                            // Test Request - respond with heartbeat
                            let test_req_id = fields.get(&112).map(|s| s.as_str()).unwrap_or("0");
                            let hb = self.build_fix_message("0", &[(112, test_req_id)]);
                            let _ = writer.write_all(hb.as_bytes()).await;
                        }
                        "5" => {
                            // Logout
                            info!("FIX Logout received");
                            return Ok(());
                        }
                        _ => {
                            debug!(msg_type, "FIX message received");
                        }
                    }
                }
                Ok(Err(e)) => {
                    return Err(e).context("FIX read error");
                }
                Err(_) => {
                    // Timeout - send heartbeat
                    let hb = self.build_fix_message("0", &[]);
                    writer.write_all(hb.as_bytes()).await?;
                }
            }
        }
    }
}

impl ForecastExFeed {
    fn handle_market_data(
        &mut self,
        fields: &std::collections::HashMap<u32, String>,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let symbol = fields.get(&55).map(|s| s.as_str()).unwrap_or("");
        if symbol.is_empty() { return; }

        if let Some(book) = self.books.get_mut(symbol) {
            // Parse MD entries
            // Tag 268 = NoMDEntries, followed by repeating group:
            // 269 = MDEntryType (0=Bid, 1=Offer)
            // 270 = MDEntryPx
            // 271 = MDEntrySize
            let entry_type = fields.get(&269).map(|s| s.as_str()).unwrap_or("");
            let price = fields.get(&270)
                .and_then(|s| Decimal::from_str(s).ok())
                .unwrap_or(Decimal::ZERO);
            let size = fields.get(&271)
                .and_then(|s| Decimal::from_str(s).ok())
                .unwrap_or(Decimal::ZERO);

            match entry_type {
                "0" => { // Bid
                    if size == Decimal::ZERO { book.bids.remove(&price); }
                    else { book.bids.insert(price, size); }
                }
                "1" => { // Offer
                    if size == Decimal::ZERO { book.asks.remove(&price); }
                    else { book.asks.insert(price, size); }
                }
                _ => {}
            }

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(symbol) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/feeds/forecastex.rs
git commit -m "feat: add ForecastEx FIX 4.4 feed handler with TCP client"
```

---

### Task 6: Normalizer Utilities

**Files:**
- Create: `src/feeds/normalizer.rs`

- [ ] **Step 1: Write normalizer utilities**

`src/feeds/normalizer.rs`:
```rust
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

use crate::types::*;

/// Compute a deterministic unified market ID from question + resolution + expiry
pub fn compute_unified_market_id(question: &str, resolution_source: &str, expiry: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(question.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(resolution_source.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(expiry.as_bytes());
    let hash = hasher.finalize();
    // Use first 16 bytes as UUID v4-like ID
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    // Set version to 4 and variant bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Calculate Kalshi taker fee for a given contract price (0.01-0.99)
pub fn kalshi_taker_fee(contract_price: Decimal) -> Decimal {
    let seven_cents = Decimal::from_str("0.07").unwrap();
    seven_cents * contract_price * (Decimal::ONE - contract_price)
}

/// Calculate Kalshi maker fee (25% of taker fee)
pub fn kalshi_maker_fee(contract_price: Decimal) -> Decimal {
    kalshi_taker_fee(contract_price) * Decimal::from_str("0.25").unwrap()
}

/// Calculate Polymarket fee for given price and fee_rate_bps
pub fn polymarket_fee(price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
    let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
    let max_side = price.max(Decimal::ONE - price);
    rate * quantity * max_side
}

/// Calculate VWAP slippage for a target order size against order book depth
pub fn estimate_slippage(target_size: Decimal, depth: &[PriceLevel], is_buy: bool) -> Decimal {
    if depth.is_empty() || target_size == Decimal::ZERO {
        return Decimal::ZERO;
    }

    let levels: Vec<&PriceLevel> = if is_buy {
        // For buying, walk asks from lowest to highest
        depth.iter().filter(|l| l.size > Decimal::ZERO).collect()
    } else {
        // For selling, walk bids from highest to lowest
        depth.iter().filter(|l| l.size > Decimal::ZERO).collect()
    };

    if levels.is_empty() {
        return Decimal::ZERO;
    }

    let best_price = levels[0].price;
    let mut remaining = target_size;
    let mut total_cost = Decimal::ZERO;

    for level in &levels {
        let fill_qty = remaining.min(level.size);
        total_cost += fill_qty * level.price;
        remaining -= fill_qty;
        if remaining <= Decimal::ZERO {
            break;
        }
    }

    if remaining > Decimal::ZERO {
        // Not enough depth
        return Decimal::from_str("999").unwrap(); // Signal insufficient liquidity
    }

    let vwap = total_cost / target_size;
    (vwap - best_price).abs()
}
```

- [ ] **Step 2: Update main.rs module declaration**

Add to `src/main.rs`:
```rust
mod feeds;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/feeds/ src/main.rs
git commit -m "feat: add normalizer utilities with fee calculators and slippage estimation"
```

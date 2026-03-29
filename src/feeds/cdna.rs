use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message, Connector};
use tracing::{info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::CdnaConfig;
use crate::types::*;

pub struct CdnaFeed {
    config: CdnaConfig,
    subscriptions: Vec<(String, Uuid)>,
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
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / Decimal::from(2))
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
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        Some(NormalizedTick {
            platform: Platform::Cdna,
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
            fee_rate_bps: 150,
            sequence: 0,
        })
    }
}

#[async_trait]
impl FeedHandler for CdnaFeed {
    fn platform(&self) -> Platform {
        Platform::Cdna
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to CDNA WebSocket");

        let tls_connector = native_tls::TlsConnector::builder()
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .context("Failed to build CDNA TLS connector")?;
        let (ws_stream, _) = connect_async_tls_with_config(
            url, None, false, Some(Connector::NativeTls(tls_connector)),
        )
            .await
            .context("Failed to connect to CDNA WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

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

        // Accept both the initial subscription confirmation and subsequent push
        // updates. Push updates have a different (or absent) "method" value;
        // the reliable discriminator is the channel name in result.channel.
        let channel = v.get("result")
            .and_then(|r| r.get("channel"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if !channel.starts_with("book.") {
            return Ok(());
        }

        let instrument = &channel[5..];

        if let Some(data) = v.get("result").and_then(|r| r.get("data")) {
            if let Some(book) = self.books.get_mut(instrument) {
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

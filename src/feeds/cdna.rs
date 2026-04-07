use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message, Connector};
use tracing::{info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use super::common::LocalBookOps;
use crate::config::CdnaConfig;
use crate::types::*;

pub struct CdnaFeed {
    config: CdnaConfig,
    subscriptions: std::collections::HashMap<String, Uuid>,
    fee_rates: std::collections::HashMap<String, u16>,
    books: std::collections::HashMap<String, CdnaOrderBook>,
    sequence: u64,
    request_id: u64,
}

struct CdnaOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
}

impl CdnaOrderBook {
    fn new() -> Self { Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new() } }
}

impl super::common::LocalBookOps for CdnaOrderBook {
    fn bids(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.bids }
    fn asks(&self) -> &std::collections::BTreeMap<Decimal, Decimal> { &self.asks }
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
    pub fn new(config: CdnaConfig, subscriptions: Vec<(String, Uuid, u16)>) -> Self {
        let mut subs = std::collections::HashMap::new();
        let mut fees = std::collections::HashMap::new();
        for (inst, id, fee) in subscriptions {
            subs.insert(inst.clone(), id);
            fees.insert(inst, fee);
        }
        Self {
            config,
            subscriptions: subs,
            fee_rates: fees,
            books: std::collections::HashMap::new(),
            sequence: 0,
            request_id: 1,
        }
    }

    fn instrument_to_market_id(&self, instrument: &str) -> Option<Uuid> {
        self.subscriptions.get(instrument).copied()
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
            fee_rate_bps: self.fee_rates.get(instrument).copied().unwrap_or(150),
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

        let tls_connector = crate::crypto::tls::build_tls_connector()
            .context("Failed to build CDNA TLS connector")?;
        let (ws_stream, _) = connect_async_tls_with_config(
            url, None, false, Some(Connector::NativeTls(tls_connector)),
        )
            .await
            .context("Failed to connect to CDNA WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        let channels: Vec<String> = self.subscriptions.keys()
            .map(|instrument| format!("book.{}", instrument))
            .collect();

        if !channels.is_empty() {
            let sub = CdnaSubscribe {
                id: self.request_id,
                method: "subscribe".into(),
                params: CdnaSubParams { channels: channels.clone() },
            };
            self.request_id += 1;
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text)).await?;
            info!(count = channels.len(), "Subscribed to CDNA channels");
        }

        for instrument in self.subscriptions.keys() {
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
            {
                let book = self.books.entry(instrument.to_string()).or_insert_with(CdnaOrderBook::new);
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
            }  // mutable borrow on books ends here

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(instrument) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "cdna_tests.rs"]
mod cdna_tests;

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    subscriptions: Vec<(String, Uuid)>,
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
    pub fn new(config: KalshiConfig, auth: Option<KalshiAuth>, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            auth: auth.map(Arc::new),
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
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        let fee_per_contract = dec!(0.07) * mid * (Decimal::ONE - mid);
        let fee_bps = if mid > Decimal::ZERO {
            let bps = (fee_per_contract / mid) * Decimal::from(10000);
            bps.try_into().unwrap_or(175u16)
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
                let tls = native_tls::TlsConnector::builder()
                    .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
                    .build()
                    .context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(request, None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        } else {
            {
                let tls = native_tls::TlsConnector::builder()
                    .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
                    .build()
                    .context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(self.config.ws_url.as_str(), None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        };

        let (mut write, mut read) = ws_stream.split();

        let tickers: Vec<String> = self.subscriptions.iter().map(|(t, _)| t.clone()).collect();
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
                        book.asks.insert(p, s);
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
                            if s == Decimal::ZERO { book.asks.remove(&p); }
                            else { book.asks.insert(p, s); }
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

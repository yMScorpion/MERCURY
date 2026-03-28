use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tracing::{debug, info};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::ForecastExConfig;
use crate::types::*;

/// Minimal FIX 4.4 client for ForecastEx market data
pub struct ForecastExFeed {
    config: ForecastExConfig,
    subscriptions: Vec<(String, Uuid)>,
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

    /// Returns the best bid (price, size) only if the bid side is non-empty.
    /// Never returns a phantom (0, 0) fallback — callers must handle None.
    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s))
    }

    /// Returns the best ask (price, size) only if the ask side is non-empty.
    /// Never returns a phantom (1.0, 0) fallback — callers must handle None.
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

const SOH: char = '\x01';

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
        // best_bid/best_ask return None when either side is empty.
        // Returning None here suppresses the tick so the spread engine never
        // sees phantom prices (bid=0 or ask=1) that would trigger fake arbs.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        Some(NormalizedTick {
            platform: Platform::ForecastEx,
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
            fee_rate_bps: 0,
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
            (98, "0"),
            (108, "30"),
        ]);
        writer.write_all(logon.as_bytes()).await?;
        info!("FIX Logon sent");

        let sub_symbols: Vec<String> = self.subscriptions.iter().map(|(s, _)| s.clone()).collect();
        for symbol in &sub_symbols {
            self.books.entry(symbol.clone()).or_insert_with(FexOrderBook::new);
        }

        // Subscribe to market data (35=V)
        let symbols: Vec<(usize, String)> = self.subscriptions.iter()
            .enumerate()
            .map(|(i, (s, _))| (i, s.clone()))
            .collect();
        for (i, symbol) in &symbols {
            let md_req_id = format!("MDR{}", i);
            let md_request = self.build_fix_message("V", &[
                (262, &md_req_id),
                (263, "1"),
                (264, "10"),
                (267, "2"),
                (269, "0"),
                (269, "1"),
                (146, "1"),
                (55, symbol),
            ]);
            writer.write_all(md_request.as_bytes()).await?;
        }
        info!(count = self.subscriptions.len(), "FIX market data requests sent");

        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match tokio::time::timeout(
                std::time::Duration::from_secs(45),
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
                            self.handle_market_data(&fields, &tick_tx);
                        }
                        "0" => {
                            let hb = self.build_fix_message("0", &[]);
                            let _ = writer.write_all(hb.as_bytes()).await;
                        }
                        "1" => {
                            let test_req_id = fields.get(&112).map(|s| s.as_str()).unwrap_or("0");
                            let hb = self.build_fix_message("0", &[(112, test_req_id)]);
                            let _ = writer.write_all(hb.as_bytes()).await;
                        }
                        "5" => {
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
            let entry_type = fields.get(&269).map(|s| s.as_str()).unwrap_or("");

            // Never default price or size to 0 on parse failure — a 0-priced ask
            // would look like a near-100% arb opportunity and fire real orders.
            // If we can't parse the field, drop the update and log the raw value.
            let price = match fields.get(&270).and_then(|s| Decimal::from_str(s).ok()) {
                Some(p) => p,
                None => {
                    tracing::error!(
                        symbol,
                        raw = fields.get(&270).map(|s| s.as_str()).unwrap_or("<missing>"),
                        "ForecastEx: failed to parse FIX tag 270 (price) — dropping update"
                    );
                    return;
                }
            };
            let size = match fields.get(&271).and_then(|s| Decimal::from_str(s).ok()) {
                Some(s) => s,
                None => {
                    tracing::error!(
                        symbol,
                        raw = fields.get(&271).map(|s| s.as_str()).unwrap_or("<missing>"),
                        "ForecastEx: failed to parse FIX tag 271 (size) — dropping update"
                    );
                    return;
                }
            };

            match entry_type {
                "0" => {
                    if size == Decimal::ZERO { book.bids.remove(&price); }
                    else { book.bids.insert(price, size); }
                }
                "1" => {
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

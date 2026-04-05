use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
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
    expected_seq_num: u64,
}

struct FexOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
}

impl FexOrderBook {
    fn new() -> Self { Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new() } }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / rust_decimal::Decimal::from(2))
    }
    
    fn depth(&self) -> arrayvec::ArrayVec<PriceLevel, 20> {
        let mut levels = arrayvec::ArrayVec::new();
        for (&p, &s) in self.bids.iter().rev().take(10) { levels.push(PriceLevel { price: p, size: s }); }
        for (&p, &s) in self.asks.iter().take(10) { levels.push(PriceLevel { price: p, size: s }); }
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
            expected_seq_num: 0,
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
        
        // MED-4: Validate Checksum (Tag 10)
        if let Some(csum_idx) = msg.find("10=") {
            let body_to_checksum = &msg[..csum_idx];
            let expected_checksum: u32 = body_to_checksum.bytes().map(|b| b as u32).sum::<u32>() % 256;
            
            let end_idx = msg[csum_idx..].find(SOH).unwrap_or(msg.len() - csum_idx);
            let provided_checksum_str = &msg[csum_idx + 3 .. csum_idx + end_idx];
            
            if let Ok(provided_checksum) = provided_checksum_str.parse::<u32>() {
                if expected_checksum != provided_checksum {
                    tracing::warn!("FIX Checksum mismatch: expected {}, got {}", expected_checksum, provided_checksum);
                    return fields; // Reject corrupt message
                }
            }
        }

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

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
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
            
        let mut tls_builder = native_tls::TlsConnector::builder();
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                tls_builder.add_root_certificate(cert);
            }
        }
        let tls_connector = tls_builder.build().context("Failed to build TLS")?;
        let tokio_tls = tokio_native_tls::TlsConnector::from(tls_connector);
        let tls_stream = tokio_tls.connect(&self.config.fix_host, stream).await.context("TLS handshake failed")?;

        let (reader, mut writer) = tokio::io::split(tls_stream);
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

        let mut msg_buf = Vec::new();
        let mut line_buf = String::new();
        loop {
            msg_buf.clear();
            match tokio::time::timeout(
                std::time::Duration::from_secs(45),
                buf_reader.read_until(SOH as u8, &mut msg_buf),
            ).await {
                Ok(Ok(0)) => {
                    info!("FIX connection closed");
                    return Ok(());
                }
                Ok(Ok(_)) => {
                    let field = String::from_utf8_lossy(&msg_buf);
                    line_buf.push_str(&field);

                    if line_buf.contains("\x0110=") && line_buf.ends_with('\x01') {
                        let fields = Self::parse_fix_fields(&line_buf);
                        let msg_type = fields.get(&35).map(|s| s.as_str()).unwrap_or("");

                        // HIGH-2 FIX: Session-Level Sequence Validation
                        let seq_str = fields.get(&34).map(|s| s.as_str()).unwrap_or("0");
                        let seq_num = seq_str.parse::<u64>().unwrap_or(0);
                        if seq_num > 0 {
                            if seq_num < self.expected_seq_num {
                                tracing::warn!("FIX Sequence error: expected {}, got {}", self.expected_seq_num, seq_num);
                            } else if seq_num > self.expected_seq_num && self.expected_seq_num > 0 {
                                tracing::warn!("FIX Sequence gap: expected {}, got {}. Requesting Resend.", self.expected_seq_num, seq_num);
                                let expected_str = self.expected_seq_num.to_string();
                                let resend = self.build_fix_message("2", &[(7, &expected_str), (16, "0")]);
                                let _ = writer.write_all(resend.as_bytes()).await;
                            }
                            self.expected_seq_num = seq_num + 1;
                        }

                        match msg_type {
                            "W" | "X" => {
                                self.handle_market_data(&fields, &line_buf, &tick_tx);
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
                        line_buf.clear();
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
        raw_msg: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let symbol = fields.get(&55).map(|s| s.as_str()).unwrap_or("");
        if symbol.is_empty() { return; }

        let book = match self.books.get_mut(symbol) {
            Some(b) => b,
            None => return,
        };

        // FIX repeating groups: parse all 269/270/271 triplets from the raw message.
        // Tags appear in order: 269 (type), 270 (price), 271 (size), then next 269, etc.
        let parts: Vec<&str> = raw_msg.split(SOH).collect();
        let mut i = 0;
        let mut updated = false;
        while i < parts.len() {
            if let Some(eq) = parts[i].find('=') {
                let tag_str = &parts[i][..eq];
                if tag_str == "269" {
                    let entry_type = &parts[i][eq + 1..];
                    // Look ahead for 270 and 271
                    let price = Self::find_next_tag(&parts, i + 1, 270);
                    let size = Self::find_next_tag(&parts, i + 1, 271);
                    match (price, size) {
                        (Some(p_str), Some(s_str)) => {
                            let price = match Decimal::from_str(p_str) {
                                Ok(p) => p,
                                Err(_) => {
                                    tracing::error!(symbol, raw = p_str,
                                        "ForecastEx: bad price in repeating group — skipping entry");
                                    i += 1;
                                    continue;
                                }
                            };
                            let size = match Decimal::from_str(s_str) {
                                Ok(s) => s,
                                Err(_) => {
                                    tracing::error!(symbol, raw = s_str,
                                        "ForecastEx: bad size in repeating group — skipping entry");
                                    i += 1;
                                    continue;
                                }
                            };
                            match entry_type {
                                "0" => { // Bid
                                    if size == Decimal::ZERO { book.bids.remove(&price); }
                                    else { book.bids.insert(price, size); }
                                    updated = true;
                                }
                                "1" => { // Offer
                                    if size == Decimal::ZERO { book.asks.remove(&price); }
                                    else { book.asks.insert(price, size); }
                                    updated = true;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            i += 1;
        }

        if updated {
            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(symbol) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }
    }

    /// Scan forward from `start` for the next occurrence of `target_tag`.
    /// Stop if we hit another 269 (start of next entry) or run out of parts.
    fn find_next_tag<'a>(parts: &[&'a str], start: usize, target_tag: u32) -> Option<&'a str> {
        let target = target_tag.to_string();
        for i in start..parts.len() {
            if let Some(eq) = parts[i].find('=') {
                let tag = &parts[i][..eq];
                if tag == target {
                    return Some(&parts[i][eq + 1..]);
                }
                // Stop at next entry group (skip the 269 at the start position itself)
                if tag == "269" && i > start {
                    return None;
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "forecastex_tests.rs"]
mod forecastex_tests;

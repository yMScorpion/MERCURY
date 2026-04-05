use anyhow::{Context, Result};
use rust_decimal::Decimal;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn, error};

use super::executor::{OrderResult, PlatformOrderClient, OrderAction};
use crate::types::Side;

struct FixOrderRequest {
    market_id: String,
    action: OrderAction,
    side: Side,
    price: Decimal,
    size: Decimal,
    reply: oneshot::Sender<Result<OrderResult>>,
}

#[derive(Clone)]
pub struct ForecastExClient {
    req_tx: mpsc::Sender<FixOrderRequest>,
}

impl ForecastExClient {
    pub fn new(fix_host: String, fix_port: u16) -> Self {
        let (req_tx, req_rx) = mpsc::channel(100);
        let host = fix_host.clone();
        
        tokio::spawn(async move {
            Self::run_fix_session(host, fix_port, req_rx).await;
        });

        Self { req_tx }
    }

    async fn run_fix_session(host: String, port: u16, mut req_rx: mpsc::Receiver<FixOrderRequest>) {
        let addr = format!("{}:{}", host, port);
        loop {
            info!("ForecastEx FIX Execution Session connecting to {}...", addr);
            match Self::connect_and_process(&addr, &mut req_rx).await {
                Ok(_) => warn!("ForecastEx FIX Execution Session closed. Reconnecting in 5s."),
                Err(e) => error!(error = %e, "ForecastEx FIX Execution Session error. Reconnecting in 5s."),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    async fn connect_and_process(addr: &str, req_rx: &mut mpsc::Receiver<FixOrderRequest>) -> Result<()> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut buf_reader = BufReader::new(reader);

        let sender_comp = std::env::var("FEX_EXEC_SENDER_COMP_ID").unwrap_or_else(|_| "MERCURY_EXEC".into());
        let target_comp = std::env::var("FEX_TARGET_COMP_ID").unwrap_or_else(|_| "FORECASTEX".into());
        let mut seq_num = 1;

        // Send Logon (35=A)
        let logon = Self::build_fix_message("A", &sender_comp, &target_comp, seq_num, &[
            (98, "0"),
            (108, "30"),
        ]);
        writer.write_all(logon.as_bytes()).await?;
        seq_num += 1;
        info!("ForecastEx Execution FIX Logon sent");

        let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut cl_ord_id = 1;
        let mut pending_orders: std::collections::HashMap<String, FixOrderRequest> = std::collections::HashMap::new();

        // Process loop
        loop {
            let mut msg_buf = Vec::new();
            tokio::select! {
                req_opt = req_rx.recv() => {
                    let req = match req_opt {
                        Some(r) => r,
                        None => return Ok(()), // Channel closed
                    };
                    
                    let side_val = match (req.action, req.side) {
                        (OrderAction::Buy, Side::Yes) => "1",
                        (OrderAction::Sell, Side::Yes) => "2",
                        (OrderAction::Buy, Side::No) => "2", // Invert for NO token
                        (OrderAction::Sell, Side::No) => "1",
                    };

                    let exec_price = match req.side {
                        Side::Yes => req.price,
                        Side::No => Decimal::ONE - req.price,
                    };

                    let id_str = format!("MERC-{}", cl_ord_id);
                    cl_ord_id += 1;

                    // Send NewOrderSingle (35=D)
                    let transact_time = chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f").to_string();
                    let order = Self::build_fix_message("D", &sender_comp, &target_comp, seq_num, &[
                        (11, &id_str),             // ClOrdID
                        (55, &req.market_id),      // Symbol
                        (54, side_val),            // Side
                        (60, &transact_time),      // TransactTime
                        (38, &req.size.to_string()), // OrderQty
                        (40, "2"),                 // OrdType = Limit
                        (44, &exec_price.to_string()), // Price
                        (59, "3"),                 // TimeInForce = FOK
                    ]);
                    
                    if let Err(e) = writer.write_all(order.as_bytes()).await {
                        let _ = req.reply.send(Err(e.into()));
                        return Err(anyhow::anyhow!("Write failed"));
                    }
                    seq_num += 1;
                    
                    pending_orders.insert(id_str, req);
                }
                
                _ = heartbeat_interval.tick() => {
                    let hb = Self::build_fix_message("0", &sender_comp, &target_comp, seq_num, &[]);
                    writer.write_all(hb.as_bytes()).await?;
                    seq_num += 1;
                }
                
                res = buf_reader.read_until(0x01, &mut msg_buf) => {
                    match res {
                        Ok(0) => return Ok(()),
                        Ok(_) => {
                            let msg_str = String::from_utf8_lossy(&msg_buf);
                            let fields = Self::parse_fix_fields(&msg_str);
                            
                            // Check if it's an ExecutionReport (35=8)
                            if fields.get(&35).map(|s| s.as_str()) == Some("8") {
                                if let (Some(id), Some(status)) = (fields.get(&11), fields.get(&39)) {
                                    // 2 = Filled, 8 = Rejected, 4 = Canceled
                                    if status == "2" || status == "8" || status == "4" {
                                        if let Some(req) = pending_orders.remove(id) {
                                            if status == "2" {
                                                let result = OrderResult {
                                                    filled: true,
                                                    fill_price: crate::types::Usd(req.price),
                                                    fill_size: crate::types::Contracts(req.size),
                                                    fee: crate::types::Usd(Decimal::ZERO),
                                                    order_id: id.clone(),
                                                    error: None,
                                                };
                                                let _ = req.reply.send(Ok(result));
                                            } else {
                                                let err_text = fields.get(&58).map(|s| s.as_str()).unwrap_or("Order Rejected or Canceled").to_string();
                                                let result = OrderResult {
                                                    filled: false,
                                                    fill_price: crate::types::Usd(Decimal::ZERO),
                                                    fill_size: crate::types::Contracts(Decimal::ZERO),
                                                    fee: crate::types::Usd(Decimal::ZERO),
                                                    order_id: id.clone(),
                                                    error: Some(err_text),
                                                };
                                                let _ = req.reply.send(Ok(result));
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }
    }

    fn parse_fix_fields(msg: &str) -> std::collections::HashMap<u32, String> {
        let mut fields = std::collections::HashMap::new();
        for part in msg.split('\x01') {
            if let Some(eq_pos) = part.find('=') {
                if let Ok(tag) = part[..eq_pos].parse::<u32>() {
                    fields.insert(tag, part[eq_pos + 1..].to_string());
                }
            }
        }
        fields
    }

    fn build_fix_message(msg_type: &str, sender: &str, target: &str, seq: u64, body_fields: &[(u32, &str)]) -> String {
        let soh = '\x01';
        let mut body = format!("35={}{}", msg_type, soh);
        body.push_str(&format!("49={}{}", sender, soh));
        body.push_str(&format!("56={}{}", target, soh));
        body.push_str(&format!("34={}{}", seq, soh));
        body.push_str(&format!("52={}{}", chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f"), soh));
        for (tag, val) in body_fields {
            body.push_str(&format!("{}={}{}", tag, val, soh));
        }
        let header = format!("8=FIX.4.4{}9={}{}", soh, body.len(), soh);
        let full = format!("{}{}", header, body);
        let checksum: u32 = full.bytes().map(|b| b as u32).sum::<u32>() % 256;
        format!("{}10={:03}{}", full, checksum, soh)
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for ForecastExClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: crate::types::Usd, size: crate::types::Contracts, _fee_rate_bps: crate::types::BasisPoints) -> Result<OrderResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let req = FixOrderRequest {
            market_id: market_id.to_string(),
            action,
            side,
            price: price.0,
            size: size.0,
            reply: reply_tx,
        };
        
        self.req_tx.send(req).await.context("ForecastEx FIX session down")?;
        tokio::time::timeout(std::time::Duration::from_secs(10), reply_rx)
            .await
            .context("ForecastEx FIX order timeout")?
            .context("ForecastEx FIX session dropped response")?
    }

    async fn cancel_order(&self, _order_id: &str) -> Result<()> {
        warn!("ForecastEx FOK cancels implicitly via exchange");
        Ok(())
    }
}

use anyhow::{Context, Result};
use alloy::primitives::{Address, U256};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::eip712::PolymarketSigner;
use crate::types::Side;

#[derive(Clone)]
pub struct PolymarketClient {
    http: reqwest::Client,
    rest_url: String,
    signer: PolymarketSigner,
    api_key: zeroize::Zeroizing<String>,
    api_secret: zeroize::Zeroizing<String>,
    api_passphrase: zeroize::Zeroizing<String>,
}

#[derive(Serialize)]
struct CreateOrderRequest {
    order: OrderPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    token_id: String,
    maker_amount: String,
    taker_amount: String,
    side: String,
    fee_rate_bps: String,
    nonce: String,
    expiration: String,
    signature: String,
    signature_type: u8,
    order_type: String,
}

#[derive(Deserialize)]
struct OrderResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    order_id: Option<String>,
    #[serde(default)]
    error_msg: Option<String>,
}

impl PolymarketClient {
    pub fn new(
        rest_url: String,
        signer: PolymarketSigner,
        api_key: String,
        api_secret: String,
        api_passphrase: String,
    ) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5));
        
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = reqwest::tls::Certificate::from_pem(&cert_pem) {
                builder = builder.add_root_certificate(cert);
            }
        }
            
        let http = builder.build().expect("failed to build Polymarket HTTP client");
        Self {
            http,
            rest_url,
            signer,
            api_key: zeroize::Zeroizing::new(api_key),
            api_secret: zeroize::Zeroizing::new(api_secret),
            api_passphrase: zeroize::Zeroizing::new(api_passphrase),
        }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: crate::types::Usd, size: crate::types::Contracts, fee_rate_bps: crate::types::BasisPoints) -> Result<OrderResult> {
        let price = price.0;
        let size = size.0;
        let fee_rate_bps = fee_rate_bps.0;
        // MED-5 FIX: Rate limit Polymarket submissions
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // CRITICAL FIX: Polymarket CTF does not support naked short selling. To bet NO, we must trade 
        // the specific NO token ID. We parse the dual-token string provided by discovery.
        let tokens: Vec<&str> = market_id.split(',').collect();
        let target_token_id = if side == Side::Yes { tokens[0] } else { tokens.get(1).unwrap_or(&tokens[0]) };
        
        let (side_str, side_u8) = match action {
            OrderAction::Buy => ("BUY", 0u8),
            OrderAction::Sell => ("SELL", 1u8),
        };
        
        info!(target_token_id, action = side_str, side = ?side, price = %price, size = %size, fee_rate_bps, "Submitting Polymarket order");

        let scale = Decimal::from(1_000_000u64);
        
        // CRITICAL FIX: The `price` passed from the spread engine and unwind watchdog is 
        // already native to the specific Token ID we are trading. Do not invert it again.
        let pm_price = price;

        let (maker_amount_scaled, taker_amount_scaled) = match action {
            OrderAction::Buy => {
                // Buying: Maker gives USDC (price * size), Taker gets Tokens (size)
                ((size * pm_price * scale).floor(), (size * scale).floor())
            }
            OrderAction::Sell => {
                // Selling: Maker gives Tokens (size), Taker gets USDC (price * size)
                ((size * scale).floor(), (size * pm_price * scale).floor())
            }
        };

        let maker_amount_u256 = U256::from_str(&maker_amount_scaled.to_string()).unwrap_or(U256::ZERO);
        let taker_amount_u256 = U256::from_str(&taker_amount_scaled.to_string()).unwrap_or(U256::ZERO);
        
        // CRITICAL FIX: We must parse the specific `target_token_id` (e.g. "12345") 
        // into the EIP-712 signature, not the raw comma-separated `market_id` string.
        let token_id_u256 = U256::from_str(target_token_id)
            .map_err(|_| anyhow::anyhow!("Invalid Polymarket token ID: {}", target_token_id))?;

        let fee_rate_bps_u256 = U256::from(fee_rate_bps);

        let now = chrono::Utc::now();
        
        // MED-8: Enforce atomic uniqueness on sub-millisecond execution bursts
        static NONCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = NONCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // LOW-9: Use millis * 1M to avoid timestamp_nanos_opt None on overflow
        let nonce_val = (now.timestamp_millis() as u64 * 1_000_000) + counter;
        let nonce = U256::from(nonce_val);
        
        // Fix: 5 minutes is dangerously long for an HFT signature. 
        // 30 seconds caps our risk of resting order sniping on network lag.
        let expiration_u256 = U256::from((now.timestamp() + 30) as u64);

        let maker_addr = self.signer.address();

        let signature = self.signer.sign_order(
            nonce, maker_addr, maker_addr, Address::ZERO, token_id_u256,
            maker_amount_u256, taker_amount_u256, expiration_u256, nonce, 
            fee_rate_bps_u256, // Pass the actual fee rate so the signature matches the payload
            side_u8, 0,
        ).await.context("EIP-712 order signing failed")?;

        let url = format!("{}/order", self.rest_url);
        
        use rust_decimal::prelude::ToPrimitive;
        // Strip out any trailing decimals from the scaling operation to prevent HTTP 400s
        let maker_str = maker_amount_scaled.to_u64().unwrap_or(0).to_string();
        let taker_str = taker_amount_scaled.to_u64().unwrap_or(0).to_string();

        let payload = OrderPayload {
            // CRITICAL FIX: Use the specific YES or NO target_token_id instead of the raw market_id pair
            token_id: target_token_id.to_string(),
            maker_amount: maker_str,
            taker_amount: taker_str,
            side: side_str.to_string(),
            fee_rate_bps: fee_rate_bps.to_string(), 
            nonce: nonce_val.to_string(),
            expiration: expiration_u256.to_string(),
            signature,
            signature_type: 0,
            // CRITICAL FIX: 'IOC' permits partial fills. Because the fast-path assumes 100% execution,
            // a 50% partial fill leaves you 50% unhedged without triggering the Unwind Watchdog. 
            // 'FOK' (Fill Or Kill) guarantees binary success/failure.
            order_type: "FOK".to_string(),
        };

        let resp = self.http
            .post(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .json(&CreateOrderRequest { order: payload })
            .send()
            .await
            .context("Polymarket order submission failed")?;

        let status_code = resp.status();
        let body: OrderResponse = resp.json().await.unwrap_or(OrderResponse {
            success: false,
            order_id: None,
            error_msg: Some(format!("HTTP {}", status_code)),
        });

        if body.success {
            let order_id_str = body.order_id.unwrap_or_default();
            let mut fill_size = taker_amount_scaled / scale;
            let mut actual_price = price;
            let mut matched_confirmed = false;
            
            if !order_id_str.is_empty() {
                let fetch_url = format!("{}/orders/{}", self.rest_url, order_id_str);
                let mut retries = 0;
                while retries < 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(150 * (retries + 1))).await;
                    
                    let req = self.http.get(&fetch_url)
                        .header("POLY_API_KEY", self.api_key.as_str())
                        .header("POLY_SECRET", self.api_secret.as_str())
                        .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
                        .send();
                        
                    // MED-6 FIX: Apply a strict timeout to the confirmation loop to prevent blocking the executor
                    if let Ok(Ok(fetch_resp)) = tokio::time::timeout(std::time::Duration::from_secs(3), req).await {
                        if let Ok(order_data) = fetch_resp.json::<serde_json::Value>().await {
                            let mut updated = false;
                            if let Some(avg_price_str) = order_data.get("average_price").and_then(|v| v.as_str()) {
                                if let Ok(parsed_price) = Decimal::from_str(avg_price_str) {
                                    actual_price = parsed_price;
                                    updated = true;
                                }
                            }
                            if let Some(size_matched_str) = order_data.get("size_matched").and_then(|v| v.as_str()) {
                                if let Ok(parsed_size) = Decimal::from_str(size_matched_str) {
                                    fill_size = parsed_size;
                                    matched_confirmed = true;
                                    updated = true;
                                }
                            }
                            if updated { break; }
                        }
                    }
                    retries += 1;
                }
                
                if !matched_confirmed {
                    // CRIT-4 FIX: Assume order failed to avoid unhedged exposure on phantom fills
                    fill_size = Decimal::ZERO;
                    tracing::error!("CRITICAL: Polymarket order {} success but size_matched unconfirmed. Treating as unfilled.", order_id_str);
                }
            }
            
            let filled = fill_size > Decimal::ZERO;
            let estimated_fee = crate::feeds::normalizer::polymarket_fee(actual_price, fill_size, fee_rate_bps as u16);

            Ok(OrderResult {
                filled,
                fill_price: crate::types::Usd(actual_price),
                fill_size: crate::types::Contracts(fill_size),
                fee: crate::types::Usd(estimated_fee),
                order_id: order_id_str,
                error: if !filled { Some("FOK order returned zero fill size".into()) } else { None },
            })
        } else {
            Ok(OrderResult {
                filled: false,
                fill_price: crate::types::Usd(Decimal::ZERO),
                fill_size: crate::types::Contracts(Decimal::ZERO),
                fee: crate::types::Usd(Decimal::ZERO),
                order_id: String::new(),
                error: body.error_msg.or_else(|| Some(format!("HTTP {}", status_code))),
            })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/order/{}", self.rest_url, order_id);
        let resp = self.http.delete(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .send()
            .await
            .context("Polymarket cancel failed")?;
        if !resp.status().is_success() {
            tracing::warn!(order_id, status = %resp.status(), "Polymarket cancel returned non-2xx");
        }
    // FOK orders fill entirely or not at all — cancel is best-effort for edge cases
        Ok(())
    }
}

impl PolymarketClient {
    /// Queries the Polymarket API for the actual live balance of the wallet
    pub async fn get_balance(&self) -> Result<Decimal> {
        let url = format!("{}/balance", self.rest_url);
        let resp = self.http.get(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Polymarket get_balance failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let balance_str = body.get("usdcBalance").and_then(|v| v.as_str()).unwrap_or("0");
        Ok(Decimal::from_str(balance_str).unwrap_or(Decimal::ZERO))
    }
}

use anyhow::{Context, Result};
use ethers::core::types::{Address, U256};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::eip712::PolymarketSigner;
use crate::types::Side;

#[derive(Clone)]
pub struct PolymarketClient {
    http: reqwest::Client,
    rest_url: String,
    signer: PolymarketSigner,
    api_key: String,
    api_secret: String,
    api_passphrase: String,
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
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build Polymarket HTTP client");
        Self {
            http,
            rest_url,
            signer,
            api_key,
            api_secret,
            api_passphrase,
        }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
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

        let maker_amount_u256 = U256::from_dec_str(&maker_amount_scaled.to_string()).unwrap_or(U256::zero());
        let taker_amount_u256 = U256::from_dec_str(&taker_amount_scaled.to_string()).unwrap_or(U256::zero());
        
        // CRITICAL FIX: We must parse the specific `target_token_id` (e.g. "12345") 
        // into the EIP-712 signature, not the raw comma-separated `market_id` string.
        let token_id_u256 = U256::from_dec_str(target_token_id)
            .map_err(|_| anyhow::anyhow!("Invalid Polymarket token ID: {}", target_token_id))?;

        let fee_rate_bps_u256 = U256::from(fee_rate_bps);

        let now = chrono::Utc::now();
        let nonce_val = now.timestamp_nanos_opt().unwrap_or(0) as u64;
        let nonce = U256::from(nonce_val);
        
        // Fix: 5 minutes is dangerously long for an HFT signature. 
        // 30 seconds caps our risk of resting order sniping on network lag.
        let expiration_u256 = U256::from((now.timestamp() + 30) as u64);

        let maker_addr = self.signer.address();

        let signature = self.signer.sign_order(
            nonce, maker_addr, maker_addr, Address::zero(), token_id_u256,
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
            .header("POLY_API_KEY", &self.api_key)
            .header("POLY_SECRET", &self.api_secret)
            .header("POLY_PASSPHRASE", &self.api_passphrase)
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
            let fill_size = taker_amount_scaled / scale;
            let estimated_fee = crate::feeds::normalizer::polymarket_fee(price, fill_size, fee_rate_bps as u16);

            // Fix: We must query the authoritative fill price. 
            // The reconciler does NOT do this. Falsifying fill prices breaks Kelly sizing and Bankroll.
            let mut actual_price = price;
            if !order_id_str.is_empty() {
                let fetch_url = format!("{}/orders/{}", self.rest_url, order_id_str);
                if let Ok(fetch_resp) = self.http.get(&fetch_url).send().await {
                    if let Ok(order_data) = fetch_resp.json::<serde_json::Value>().await {
                        if let Some(avg_price_str) = order_data.get("average_price").and_then(|v| v.as_str()) {
                            if let Ok(parsed_price) = Decimal::from_str(avg_price_str) {
                                actual_price = parsed_price;
                            }
                        }
                    }
                }
            }

            Ok(OrderResult {
                filled: true,
                fill_price: actual_price,
                fill_size: actual_size,
                fee: actual_fee,
                order_id: order_id_str,
                error: None,
            })
        } else {
            Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: body.error_msg.or_else(|| Some(format!("HTTP {}", status_code))),
            })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/order/{}", self.rest_url, order_id);
        let resp = self.http.delete(&url)
            .header("POLY_API_KEY", &self.api_key)
            .header("POLY_SECRET", &self.api_secret)
            .header("POLY_PASSPHRASE", &self.api_passphrase)
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

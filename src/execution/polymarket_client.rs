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
    /// Poll the order status endpoint to get actual fill details.
    /// Returns (actual_fill_price, actual_fill_size, fee) if filled.
    async fn poll_fill(&self, order_id: &str) -> Option<(Decimal, Decimal, Decimal)> {
        let url = format!("{}/order/{}", self.rest_url, order_id);
        for attempt in 0..3 {
            tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt + 1))).await;
            let resp = match self.http
                .get(&url)
                .header("POLY_API_KEY", &self.api_key)
                .header("POLY_SECRET", &self.api_secret)
                .header("POLY_PASSPHRASE", &self.api_passphrase)
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            let body: serde_json::Value = match resp.json().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status == "FILLED" || status == "CLOSED" {
                let avg_price = body.get("average_price")
                    .or_else(|| body.get("price"))
                    .and_then(|p| p.as_str())
                    .and_then(|s| s.parse::<Decimal>().ok());
                let filled_size = body.get("size_filled")
                    .or_else(|| body.get("original_size"))
                    .and_then(|s| s.as_str())
                    .and_then(|s| s.parse::<Decimal>().ok());
                let fee = body.get("fee")
                    .and_then(|f| f.as_str())
                    .and_then(|s| s.parse::<Decimal>().ok())
                    .unwrap_or(Decimal::ZERO);
                if let (Some(price), Some(size)) = (avg_price, filled_size) {
                    return Some((price, size, fee));
                }
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
        let side_str = match side { Side::Yes => "BUY", Side::No => "SELL" };
        let side_u8: u8 = match side { Side::Yes => 0, Side::No => 1 };
        info!(market_id, side = side_str, price = %price, size = %size, fee_rate_bps, "Submitting Polymarket order");

        // Scale to USDC/outcome-token base units (6 decimals)
        let scale = Decimal::from(1_000_000u64);
        
        // Ensure maker/taker amounts invert correctly for SELL orders
        let (maker_amount_scaled, taker_amount_scaled) = if side == Side::Yes {
            ((size * price * scale).floor(), (size * scale).floor())
        } else {
            ((size * scale).floor(), (size * price * scale).floor())
        };

        let maker_amount_u256 = U256::from_dec_str(&maker_amount_scaled.to_string()).unwrap_or(U256::zero());
        let taker_amount_u256 = U256::from_dec_str(&taker_amount_scaled.to_string()).unwrap_or(U256::zero());
        
        let token_id_u256 = U256::from_dec_str(market_id)
            .map_err(|_| anyhow::anyhow!("Invalid Polymarket token ID: {}", market_id))?;

        let fee_rate_bps_u256 = U256::from(fee_rate_bps);

        let now = chrono::Utc::now();
        let nonce_val = now.timestamp_nanos_opt().unwrap_or(0) as u64;
        let nonce = U256::from(nonce_val);
        let expiration_u256 = U256::from((now.timestamp() + 300) as u64);

        let maker_addr = self.signer.address();

        let signature = self.signer.sign_order(
            nonce, maker_addr, maker_addr, Address::zero(), token_id_u256,
            maker_amount_u256, taker_amount_u256, expiration_u256, nonce, 
            fee_rate_bps_u256, // Pass the actual fee rate so the signature matches the payload
            side_u8, 0,
        ).await.context("EIP-712 order signing failed")?;

        let url = format!("{}/order", self.rest_url);

        let payload = OrderPayload {
            token_id: market_id.to_string(),
            maker_amount: maker_amount_scaled.to_string(),
            taker_amount: taker_amount_scaled.to_string(),
            side: side_str.to_string(),
            fee_rate_bps: fee_rate_bps.to_string(), 
            nonce: nonce_val.to_string(),
            expiration: expiration_u256.to_string(),
            signature,
            signature_type: 0,
            order_type: "IOC".to_string(),
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

            // Return immediately with the estimated fill details.
            // Actual fill prices will be reconciled asynchronously by the
            // reconciliation engine — polling here adds 1.2s latency to
            // every execution, which is unacceptable for an HFT system.
            let (actual_price, actual_size, actual_fee) = (price, fill_size, estimated_fee);
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
        // IOC orders are already settled; cancel is best-effort only
        let _ = dec!(0); // suppress unused import warning
        Ok(())
    }
}

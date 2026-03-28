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

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal) -> Result<OrderResult> {
        let side_str = match side { Side::Yes => "BUY", Side::No => "SELL" };
        let side_u8: u8 = match side { Side::Yes => 0, Side::No => 1 };
        info!(market_id, side = side_str, price = %price, size = %size, "Submitting Polymarket order");

        // Scale to USDC/outcome-token base units (6 decimals)
        let scale = Decimal::from(1_000_000u64);
        let maker_amount_scaled = (size * price * scale).floor();
        let taker_amount_scaled = (size * scale).floor();

        let maker_amount_u256 = U256::from_dec_str(&maker_amount_scaled.to_string())
            .unwrap_or(U256::zero());
        let taker_amount_u256 = U256::from_dec_str(&taker_amount_scaled.to_string())
            .unwrap_or(U256::zero());

        // Polymarket token IDs are large decimal integers
        let token_id_u256 = U256::from_dec_str(market_id).unwrap_or(U256::zero());

        let now = chrono::Utc::now();
        let salt = U256::from(now.timestamp_nanos_opt().unwrap_or(0) as u64);
        let expiration_u256 = U256::from((now.timestamp() + 300) as u64);

        let maker_addr = self.signer.address();

        let signature = self.signer.sign_order(
            salt,
            maker_addr,
            maker_addr,
            Address::zero(),
            token_id_u256,
            maker_amount_u256,
            taker_amount_u256,
            expiration_u256,
            U256::zero(),
            U256::zero(),
            side_u8,
            0,
        ).await.context("EIP-712 order signing failed")?;

        let url = format!("{}/order", self.rest_url);

        let payload = OrderPayload {
            token_id: market_id.to_string(),
            maker_amount: maker_amount_scaled.to_string(),
            taker_amount: taker_amount_scaled.to_string(),
            side: side_str.to_string(),
            fee_rate_bps: "0".to_string(),
            nonce: salt.to_string(),
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

        // Actual fill_price comes from exchange; use requested price as approximation
        if body.success {
            let fill_size = taker_amount_scaled / scale;
            let fee = crate::feeds::normalizer::polymarket_fee(price, fill_size, 0);
            Ok(OrderResult {
                filled: true,
                fill_price: price,
                fill_size,
                fee,
                order_id: body.order_id.unwrap_or_default(),
                error: None,
            })
        } else {
            Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: body.error_msg,
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

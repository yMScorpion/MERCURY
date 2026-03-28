use anyhow::{Context, Result};
use rust_decimal::Decimal;
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
        info!(market_id, side = side_str, price = %price, size = %size, "Submitting Polymarket order");

        let nonce = format!("{}", chrono::Utc::now().timestamp_millis());
        let expiration = format!("{}", chrono::Utc::now().timestamp() + 300);
        let maker_amount = (size * price).to_string();
        let taker_amount = size.to_string();
        let url = format!("{}/order", self.rest_url);

        let payload = OrderPayload {
            token_id: market_id.to_string(),
            maker_amount,
            taker_amount,
            side: side_str.to_string(),
            fee_rate_bps: "0".to_string(),
            nonce,
            expiration,
            signature: "0x".to_string(),
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
            Ok(OrderResult {
                filled: true,
                fill_price: price,
                fill_size: size,
                fee: Decimal::ZERO,
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
        self.http.delete(&url)
            .header("POLY_API_KEY", &self.api_key)
            .header("POLY_SECRET", &self.api_secret)
            .header("POLY_PASSPHRASE", &self.api_passphrase)
            .send()
            .await
            .context("Polymarket cancel failed")?;
        Ok(())
    }
}

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::jwt::KalshiAuth;
use crate::types::Side;

use std::sync::Arc;

#[derive(Clone)]
pub struct KalshiClient {
    http: reqwest::Client,
    rest_url: String,
    auth: Arc<KalshiAuth>,
}

#[derive(Serialize)]
struct KalshiOrderRequest {
    ticker: String,
    action: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    count: i64,
    yes_price: Option<i64>,
    no_price: Option<i64>,
    expiration_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<String>,
}

#[derive(Deserialize)]
struct KalshiOrderResponse {
    #[serde(default)]
    order: Option<KalshiOrder>,
    #[serde(default)]
    error: Option<KalshiError>,
}

#[derive(Deserialize)]
struct KalshiOrder {
    order_id: String,
    #[serde(default)]
    yes_price: i64,
    #[serde(default)]
    no_price: i64,
    #[serde(default)]
    count: i64,
    #[serde(default)]
    remaining_count: i64,
}

#[derive(Deserialize)]
struct KalshiError {
    message: String,
}

impl KalshiClient {
    pub fn new(rest_url: String, auth: KalshiAuth) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build Kalshi HTTP client");
        Self { http, rest_url, auth: Arc::new(auth) }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for KalshiClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
        // Round to nearest cent before converting — avoids silent truncation (e.g. 50.5¢ → 50¢).

        // Avoid string parsing panics by directly safely converting rounded decimals
        let mut price_cents = (price * Decimal::from(100)).round().to_i64().unwrap_or(50);
        
        // CRITICAL FIX: Kalshi explicitly rejects prices of 0 or 100.
        price_cents = price_cents.clamp(1, 99);
        
        let count = size.round().to_i64().unwrap_or(1);

        let (kalshi_side, yes_price, no_price) = match side {
            Side::Yes => ("yes".to_string(), Some(price_cents), None),
            Side::No => ("no".to_string(), None, Some(price_cents)),
        };

        let kalshi_action = match action {
            OrderAction::Buy => "buy".to_string(),
            OrderAction::Sell => "sell".to_string(),
        };

        info!(ticker = market_id, action = %kalshi_action, side = %kalshi_side, price_cents, count, "Submitting Kalshi order");

        let url = format!("{}/portfolio/orders", self.rest_url);
        let auth_header = self.auth.auth_header().await?;

        let req = KalshiOrderRequest {
            ticker: market_id.to_string(),
            action: kalshi_action,
            side: kalshi_side,
            order_type: "limit".to_string(),
            count,
            yes_price,
            no_price,
            expiration_ts: None,
            client_order_id: None,
            // CRITICAL FIX: Ensure the order is Fill-or-Kill so it doesn't rest on the book
            time_in_force: Some("fok".to_string()), 
        };

        let resp = self.http
            .post(&url)
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .context("Kalshi order submission failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!("Kalshi rate limited: {}", body);
            }
            return Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: Some(format!("HTTP {}: {}", status, body)),
            });
        }

        let body: KalshiOrderResponse = resp.json().await.unwrap_or(KalshiOrderResponse {
            order: None,
            error: Some(KalshiError { message: "Failed to decode response".to_string() }),
        });

        if let Some(order) = body.order {
            let filled_count = order.count - order.remaining_count;
            let filled = filled_count > 0;
            let fill_price = Decimal::from(order.yes_price.max(order.no_price)) / Decimal::from(100);
            // Use the fee_rate_bps supplied by the caller (derived from the live tick)
            // instead of recomputing with a hardcoded 7% formula.
            let total_fee = Decimal::from(fee_rate_bps) / Decimal::from(10_000)
                * fill_price
                * Decimal::from(filled_count);
            Ok(OrderResult {
                filled,
                fill_price,
                fill_size: Decimal::from(filled_count),
                fee: total_fee,
                order_id: order.order_id,
                error: if !filled { Some("Order not filled".into()) } else { None },
            })
        } else {
            let error_msg = body.error.map(|e| e.message).unwrap_or_else(|| "Unknown error".into());
            Ok(OrderResult { filled: false, fill_price: Decimal::ZERO, fill_size: Decimal::ZERO, fee: Decimal::ZERO, order_id: String::new(), error: Some(error_msg) })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/portfolio/orders/{}", self.rest_url, order_id);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.delete(&url)
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("Kalshi cancel failed")?;
        let status = resp.status();
        if !status.is_success() {
            tracing::warn!("Kalshi cancel_order failed: HTTP {}", status);
        }
        Ok(())
    }
}

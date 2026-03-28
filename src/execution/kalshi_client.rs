use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::jwt::KalshiAuth;
use crate::types::Side;

#[derive(Clone)]
pub struct KalshiClient {
    http: reqwest::Client,
    rest_url: String,
    auth: KalshiAuth,
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
        Self { http: reqwest::Client::new(), rest_url, auth }
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for KalshiClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal) -> Result<OrderResult> {
        let price_cents = (price * Decimal::from(100)).to_string().parse::<i64>().unwrap_or(50);
        let count = size.to_string().parse::<i64>().unwrap_or(1);

        let (kalshi_side, yes_price, no_price) = match side {
            Side::Yes => ("yes".to_string(), Some(price_cents), None),
            Side::No => ("no".to_string(), None, Some(price_cents)),
        };

        info!(ticker = market_id, side = %kalshi_side, price_cents, count, "Submitting Kalshi order");

        let url = format!("{}/portfolio/orders", self.rest_url);
        let auth_header = self.auth.auth_header()?;

        let req = KalshiOrderRequest {
            ticker: market_id.to_string(),
            action: "buy".to_string(),
            side: kalshi_side,
            order_type: "limit".to_string(),
            count,
            yes_price,
            no_price,
            expiration_ts: None,
        };

        let resp = self.http
            .post(&url)
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .context("Kalshi order submission failed")?;

        let status_code = resp.status();
        let body: KalshiOrderResponse = resp.json().await.unwrap_or(KalshiOrderResponse {
            order: None,
            error: Some(KalshiError { message: format!("HTTP {}", status_code) }),
        });

        if let Some(order) = body.order {
            let filled_count = order.count - order.remaining_count;
            let filled = filled_count > 0;
            let fill_price = Decimal::from(order.yes_price.max(order.no_price)) / Decimal::from(100);
            let fee_per = Decimal::from(7) / Decimal::from(100) * fill_price * (Decimal::ONE - fill_price);
            let total_fee = fee_per * Decimal::from(filled_count);
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
        let auth_header = self.auth.auth_header()?;
        self.http.delete(&url)
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("Kalshi cancel failed")?;
        Ok(())
    }
}

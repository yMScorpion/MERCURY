use anyhow::{Context, Result};
use rust_decimal::Decimal;
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::types::Side;

#[derive(Clone)]
pub struct CdnaClient {
    http: reqwest::Client,
    rest_url: String,
    api_key: String,
    api_secret: String,
}

impl CdnaClient {
    pub fn new(rest_url: String, api_key: String, api_secret: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to build CDNA HTTP client");
        Self { http, rest_url, api_key, api_secret }
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for CdnaClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal, _fee_rate_bps: u32) -> Result<OrderResult> {
        info!(market_id, side = %side, price = %price, size = %size, "Submitting CDNA order");
        let side_str = match side { Side::Yes => "BUY", Side::No => "SELL" };
        let url = format!("{}/private/create-order", self.rest_url);
        let body = serde_json::json!({
            "instrument_name": market_id,
            "side": side_str,
            "type": "LIMIT",
            "price": price.to_string(),
            "quantity": size.to_string(),
            "time_in_force": "IOC",
        });

        let resp = self.http
            .post(&url)
            .header("X-API-KEY", &self.api_key)
            .json(&body)
            .send()
            .await
            .context("CDNA order failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: Some(format!("HTTP {}: {}", status, body_text)),
            });
        }

        let result: serde_json::Value = resp.json().await.unwrap_or_default();

        let filled = result.get("result").and_then(|r| r.get("status"))
            .and_then(|s| s.as_str()).map(|s| s == "FILLED").unwrap_or(false);
        let fill_price = result.get("result").and_then(|r| r.get("avg_price"))
            .and_then(|p| p.as_str()).and_then(|s| s.parse::<Decimal>().ok()).unwrap_or(price);

        let api_status = result.get("result").and_then(|r| r.get("status"))
            .and_then(|s| s.as_str()).unwrap_or("UNKNOWN").to_string();

        Ok(OrderResult {
            filled,
            fill_price,
            fill_size: if filled { size } else { Decimal::ZERO },
            fee: Decimal::ZERO,
            order_id: result.get("result").and_then(|r| r.get("order_id"))
                .and_then(|o| o.as_str()).unwrap_or("").to_string(),
            error: if !filled { Some(api_status) } else { None },
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/private/cancel-order", self.rest_url);
        let resp = self.http.post(&url)
            .header("X-API-KEY", &self.api_key)
            .json(&serde_json::json!({"order_id": order_id}))
            .send()
            .await
            .context("CDNA cancel_order send failed")?;
        if !resp.status().is_success() {
            tracing::warn!("CDNA cancel_order returned HTTP {}", resp.status());
        }
        Ok(())
    }
}

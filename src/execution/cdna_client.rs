use anyhow::{Context, Result};
use ring::hmac;
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

    /// Build CDNA HMAC-SHA256 authentication headers.
    ///
    /// CDNA signs requests as:
    ///   signature = HMAC-SHA256(api_secret, nonce + method + path + body)
    /// where nonce is a millisecond-precision Unix timestamp string.
    fn auth_headers(&self, method: &str, path: &str, body: &str) -> Result<[(&'static str, String); 3]> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock before Unix epoch")?
            .as_millis()
            .to_string();

        let sign_payload = format!("{}{}{}{}", nonce, method, path, body);
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.api_secret.as_bytes());
        let tag = hmac::sign(&key, sign_payload.as_bytes());
        let signature = hex::encode(tag.as_ref());

        Ok([
            ("X-API-KEY", self.api_key.clone()),
            ("X-ACCESS-NONCE", nonce),
            ("X-ACCESS-SIGN", signature),
        ])
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for CdnaClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
        info!(market_id, side = %side, price = %price, size = %size, "Submitting CDNA order");
        let side_str = match side { Side::Yes => "BUY", Side::No => "SELL" };
        let path = "/private/create-order";
        let url = format!("{}{}", self.rest_url, path);
        let body_json = serde_json::json!({
            "instrument_name": market_id,
            "side": side_str,
            "type": "LIMIT",
            "price": price.to_string(),
            "quantity": size.to_string(),
            "time_in_force": "IOC",
        });
        let body_str = body_json.to_string();
        let auth = self.auth_headers("POST", path, &body_str)?;

        let resp = self.http
            .post(&url)
            .header(auth[0].0, &auth[0].1)
            .header(auth[1].0, &auth[1].1)
            .header(auth[2].0, &auth[2].1)
            .header("Content-Type", "application/json")
            .body(body_str)
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

        let fill_size = if filled { size } else { Decimal::ZERO };
        // Compute taker fee from the bps rate supplied by the caller.
        let fee = Decimal::from(fee_rate_bps) / Decimal::from(10_000) * fill_price * fill_size;
        Ok(OrderResult {
            filled,
            fill_price,
            fill_size,
            fee,
            order_id: result.get("result").and_then(|r| r.get("order_id"))
                .and_then(|o| o.as_str()).unwrap_or("").to_string(),
            error: if !filled { Some(api_status) } else { None },
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let path = "/private/cancel-order";
        let url = format!("{}{}", self.rest_url, path);
        let body_str = serde_json::json!({"order_id": order_id}).to_string();
        let auth = self.auth_headers("POST", path, &body_str)?;
        let resp = self.http.post(&url)
            .header(auth[0].0, &auth[0].1)
            .header(auth[1].0, &auth[1].1)
            .header(auth[2].0, &auth[2].1)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .context("CDNA cancel_order send failed")?;
        if !resp.status().is_success() {
            tracing::warn!("CDNA cancel_order returned HTTP {}", resp.status());
        }
        Ok(())
    }
}

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
        // Use the centralized TLS builder which configures keep-alive, connection pool,
        // TCP_NODELAY, and cert pinning consistently across all platform clients.
        let http = crate::crypto::tls::build_reqwest_client()
            .expect("failed to build Kalshi HTTP client");
        Self { http, rest_url, auth: Arc::new(auth) }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for KalshiClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: crate::types::Usd, size: crate::types::Contracts, _fee_rate_bps: crate::types::BasisPoints) -> Result<OrderResult> {
        let price = price.0;
        let size = size.0;
        // Round to nearest cent before converting — avoids silent truncation (e.g. 50.5¢ → 50¢).

        // Avoid string parsing panics by directly safely converting rounded decimals
        let mut price_cents = (price * Decimal::from(100)).round().to_i64().unwrap_or(50);
        
        // Kalshi rejects prices of 0 or 100 cents. If we reach these extremes the
        // market is essentially resolved — abort rather than execute at a mutated price
        // that no longer matches the spread engine's computation.
        if price_cents <= 0 || price_cents >= 100 {
            return Ok(OrderResult {
                filled: false,
                fill_price: crate::types::Usd(Decimal::ZERO),
                fill_size: crate::types::Contracts(Decimal::ZERO),
                fee: crate::types::Usd(Decimal::ZERO),
                order_id: String::new(),
                error: Some(format!("Price {}¢ is at or beyond Kalshi's 1–99¢ valid range — market likely near resolution", price_cents)),
            });
        }
        
        // CRIT-1 / MED-10 FIX: Prevent catastrophic fallback to i64::MAX on extreme size overflows
        let count = size.floor().to_i64().unwrap_or(0).clamp(1, 10_000);

        // Rate limiting is handled by CB6 (execution failure rate circuit breaker).
        // A fixed 50ms sleep per order adds unacceptable latency for arbitrage.
        // Remove unconditional sleep; the executor's rate_limits HashMap handles 429 backoff.

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
                fill_price: crate::types::Usd(Decimal::ZERO),
                fill_size: crate::types::Contracts(Decimal::ZERO),
                fee: crate::types::Usd(Decimal::ZERO),
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
            // CRIT-5 FIX: Centralize fee math to ensure consistency with spread engine
            let total_fee = crate::feeds::normalizer::kalshi_fee(fill_price, Decimal::from(filled_count));
            Ok(OrderResult {
                filled,
                fill_price: crate::types::Usd(fill_price),
                fill_size: crate::types::Contracts(Decimal::from(filled_count)),
                fee: crate::types::Usd(total_fee),
                order_id: order.order_id,
                error: if !filled { Some("Order not filled".into()) } else { None },
            })
        } else {
            let error_msg = body.error.map(|e| e.message).unwrap_or_else(|| "Unknown error".into());
            Ok(OrderResult { filled: false, fill_price: crate::types::Usd(Decimal::ZERO), fill_size: crate::types::Contracts(Decimal::ZERO), fee: crate::types::Usd(Decimal::ZERO), order_id: String::new(), error: Some(error_msg) })
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

impl KalshiClient {
    /// Queries the Kalshi API for the actual revenue generated by a settled market position.
    pub async fn fetch_settlement_payout(&self, ticker: &str, quantity: Decimal, avg_entry: Decimal) -> Result<Decimal> {
        let url = format!("{}/portfolio/settlements?ticker={}", self.rest_url, ticker);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("Kalshi settlement fetch failed")?;
            
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        
        let mut total_revenue_cents: i64 = 0;
        let mut total_contracts: i64 = 0;
        
        // FIX: Compute the average payout per contract to prevent multi-arb double counting
        if let Some(settlements) = body.get("settlements").and_then(|v| v.as_array()) {
            for settlement in settlements {
                total_revenue_cents += settlement.get("revenue").and_then(|v| v.as_i64()).unwrap_or(0);
                total_contracts += settlement.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }
        
        let revenue_per_contract = if total_contracts > 0 {
            Decimal::from(total_revenue_cents) / Decimal::from(total_contracts) / Decimal::from(100)
        } else {
            Decimal::ZERO
        };
        
        let total_revenue = quantity * revenue_per_contract;
        let cost_basis = quantity * avg_entry;
        let realized_pnl = total_revenue - cost_basis;
        
        Ok(realized_pnl)
    }

    /// Queries the Kalshi API for the actual live balance of the wallet
    pub async fn get_balance(&self) -> Result<Decimal> {
        let url = format!("{}/portfolio/balance", self.rest_url);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Kalshi get_balance failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let balance_cents = body.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(Decimal::from(balance_cents) / Decimal::from(100))
    }

    /// Queries the Kalshi API for all active positions
    pub async fn get_positions(&self) -> Result<Vec<(String, Decimal)>> {
        let url = format!("{}/portfolio/positions", self.rest_url);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            anyhow::bail!("Kalshi get_positions failed: {}", resp.status());
        }
        
        let body: serde_json::Value = resp.json().await?;
        let mut positions = Vec::new();
        
        if let Some(list) = body.get("positions").and_then(|v| v.as_array()) {
            for item in list {
                if let (Some(ticker), Some(size)) = (
                    item.get("ticker").and_then(|v| v.as_str()),
                    item.get("position").and_then(|v| v.as_i64())
                ) {
                    if size > 0 {
                        positions.push((ticker.to_string(), Decimal::from(size)));
                    }
                }
            }
        }
        
        Ok(positions)
    }
}

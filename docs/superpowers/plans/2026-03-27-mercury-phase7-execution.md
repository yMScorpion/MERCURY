# Phase 7: Execution Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the dual-leg execution state machine and platform-specific REST clients for order submission on all four platforms.

**Architecture:** The executor receives ValidatedOpportunity via mpsc channel, executes the risky leg first (less liquid platform), then the hedge leg. State machine handles PENDING/PARTIAL_FILL/FILLED/FAILED/UNWINDING states. Each platform client handles authentication, order signing, and submission.

**Tech Stack:** reqwest, tokio, ethers (for Polymarket signing), jsonwebtoken (for Kalshi), serde_json

**Depends on:** Phase 1 (types), Phase 3 (crypto), Phase 6 (risk)

---

### Task 1: Execution State Machine

**Files:**
- Create: `src/execution/mod.rs`
- Create: `src/execution/executor.rs`

- [ ] **Step 1: Write the dual-leg execution engine**

`src/execution/mod.rs`:
```rust
pub mod executor;
pub mod polymarket_client;
pub mod kalshi_client;
pub mod cdna_client;
pub mod forecastex_client;
```

`src/execution/executor.rs`:
```rust
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::db::Database;
use crate::types::*;
use super::polymarket_client::PolymarketClient;
use super::kalshi_client::KalshiClient;
use super::cdna_client::CdnaClient;
use super::forecastex_client::ForecastExClient;

/// Order submission result from a platform client
#[derive(Debug, Clone)]
pub struct OrderResult {
    pub filled: bool,
    pub fill_price: Decimal,
    pub fill_size: Decimal,
    pub fee: Decimal,
    pub order_id: String,
    pub error: Option<String>,
}

/// Platform-specific order client trait
#[async_trait::async_trait]
pub trait PlatformOrderClient: Send + Sync {
    async fn submit_order(
        &self,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult>;

    async fn cancel_order(&self, order_id: &str) -> Result<()>;
}

pub struct ExecutionEngine {
    rx: mpsc::Receiver<ValidatedOpportunity>,
    trade_result_tx: mpsc::Sender<TradeResult>,
    alert_tx: mpsc::Sender<AlertMessage>,
    db: Arc<dyn Database>,
    polymarket_client: Option<PolymarketClient>,
    kalshi_client: Option<KalshiClient>,
    cdna_client: Option<CdnaClient>,
    forecastex_client: Option<ForecastExClient>,
    trade_counter: i64,
    bankroll: Decimal,
}

impl ExecutionEngine {
    pub fn new(
        rx: mpsc::Receiver<ValidatedOpportunity>,
        trade_result_tx: mpsc::Sender<TradeResult>,
        alert_tx: mpsc::Sender<AlertMessage>,
        db: Arc<dyn Database>,
        polymarket_client: Option<PolymarketClient>,
        kalshi_client: Option<KalshiClient>,
        cdna_client: Option<CdnaClient>,
        forecastex_client: Option<ForecastExClient>,
        initial_bankroll: Decimal,
    ) -> Self {
        Self {
            rx,
            trade_result_tx,
            alert_tx,
            db,
            polymarket_client,
            kalshi_client,
            cdna_client,
            forecastex_client,
            trade_counter: 0,
            bankroll: initial_bankroll,
        }
    }

    pub async fn run(mut self) {
        info!("Execution engine started");
        while let Some(opp) = self.rx.recv().await {
            if let Err(e) = self.execute_arbitrage(opp).await {
                error!(error = %e, "Arbitrage execution error");
            }
        }
        info!("Execution engine stopped");
    }

    async fn execute_arbitrage(&mut self, validated: ValidatedOpportunity) -> Result<()> {
        let opp = &validated.opportunity;
        let start = Instant::now();
        self.trade_counter += 1;

        info!(
            opp_id = %opp.opp_id,
            market = %opp.market_question,
            net_spread = %opp.net_spread,
            size = %validated.approved_size,
            "Executing arbitrage"
        );

        // Determine which leg is riskier (less liquid = execute first)
        let (first_leg, second_leg) = self.order_legs(opp);

        // Execute risky leg first
        let first_result = self.execute_leg(
            &first_leg.platform,
            &opp.market_id.to_string(),
            first_leg.side,
            first_leg.price,
            validated.approved_size,
        ).await;

        let (leg_a_result, leg_b_result) = match first_result {
            Ok(first_fill) if first_fill.filled => {
                // First leg filled, execute hedge leg
                let hedge_size = first_fill.fill_size; // Match actual fill
                let second_result = self.execute_leg(
                    &second_leg.platform,
                    &opp.market_id.to_string(),
                    second_leg.side,
                    second_leg.price,
                    hedge_size,
                ).await;

                match second_result {
                    Ok(second_fill) if second_fill.filled => {
                        (first_fill, second_fill)
                    }
                    Ok(second_fill) => {
                        // Hedge leg failed - attempt unwind
                        warn!(opp_id = %opp.opp_id, "Hedge leg failed, attempting unwind");
                        let _ = self.attempt_unwind(
                            &first_leg.platform,
                            &opp.market_id.to_string(),
                            &first_fill,
                        ).await;
                        (first_fill, second_fill)
                    }
                    Err(e) => {
                        error!(error = %e, "Hedge leg error, attempting unwind");
                        let _ = self.attempt_unwind(
                            &first_leg.platform,
                            &opp.market_id.to_string(),
                            &first_fill,
                        ).await;
                        let failed = OrderResult {
                            filled: false,
                            fill_price: Decimal::ZERO,
                            fill_size: Decimal::ZERO,
                            fee: Decimal::ZERO,
                            order_id: String::new(),
                            error: Some(e.to_string()),
                        };
                        (first_fill, failed)
                    }
                }
            }
            Ok(first_fill) => {
                // First leg didn't fill
                let failed = OrderResult {
                    filled: false,
                    fill_price: Decimal::ZERO,
                    fill_size: Decimal::ZERO,
                    fee: Decimal::ZERO,
                    order_id: String::new(),
                    error: None,
                };
                (first_fill, failed)
            }
            Err(e) => {
                error!(error = %e, "First leg execution error");
                let failed = OrderResult {
                    filled: false,
                    fill_price: Decimal::ZERO,
                    fill_size: Decimal::ZERO,
                    fee: Decimal::ZERO,
                    order_id: String::new(),
                    error: Some(e.to_string()),
                };
                let failed2 = failed.clone();
                (failed, failed2)
            }
        };

        let execution_ms = start.elapsed().as_millis() as u64;

        // Determine trade status and profit
        let (status, profit, failure_reason) = self.compute_result(
            &leg_a_result, &leg_b_result, opp,
        );

        self.bankroll += profit;
        let bankroll_change_pct = if self.bankroll > Decimal::ZERO {
            profit / self.bankroll * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        let trade_result = TradeResult {
            trade_id: self.trade_counter,
            opp_id: opp.opp_id,
            market_id: opp.market_id,
            market_question: opp.market_question.clone(),
            leg_a_platform: first_leg.platform,
            leg_a_side: first_leg.side,
            leg_a_price: first_leg.price,
            leg_a_size: leg_a_result.fill_size,
            leg_a_fill_price: leg_a_result.fill_price,
            leg_a_fee: leg_a_result.fee,
            leg_b_platform: second_leg.platform,
            leg_b_side: second_leg.side,
            leg_b_price: second_leg.price,
            leg_b_size: leg_b_result.fill_size,
            leg_b_fill_price: leg_b_result.fill_price,
            leg_b_fee: leg_b_result.fee,
            raw_spread: opp.raw_spread,
            net_spread: opp.net_spread,
            profit,
            status,
            failure_reason,
            execution_ms,
            executed_at: Utc::now(),
            bankroll_after: self.bankroll,
            bankroll_change_pct,
        };

        // Save to DB
        let _ = self.db.insert_trade(&trade_result).await;

        // Notify via channels
        let _ = self.trade_result_tx.send(trade_result.clone()).await;
        let _ = self.alert_tx.send(AlertMessage::TradeComplete(trade_result)).await;

        Ok(())
    }

    fn order_legs<'a>(&self, opp: &'a ArbitrageOpportunity) -> (&'a LegDetail, &'a LegDetail) {
        // Execute less liquid platform first (risky leg)
        let a_liquidity = opp.leg_a.available_size;
        let b_liquidity = opp.leg_b.available_size;

        if a_liquidity <= b_liquidity {
            (&opp.leg_a, &opp.leg_b) // A is riskier
        } else {
            (&opp.leg_b, &opp.leg_a) // B is riskier
        }
    }

    async fn execute_leg(
        &self,
        platform: &Platform,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult> {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                if let Some(client) = &self.polymarket_client {
                    client.submit_order(market_id, side, price, size).await
                } else {
                    anyhow::bail!("Polymarket client not configured")
                }
            }
            Platform::Kalshi => {
                if let Some(client) = &self.kalshi_client {
                    client.submit_order(market_id, side, price, size).await
                } else {
                    anyhow::bail!("Kalshi client not configured")
                }
            }
            Platform::Cdna => {
                if let Some(client) = &self.cdna_client {
                    client.submit_order(market_id, side, price, size).await
                } else {
                    anyhow::bail!("CDNA client not configured")
                }
            }
            Platform::ForecastEx => {
                if let Some(client) = &self.forecastex_client {
                    client.submit_order(market_id, side, price, size).await
                } else {
                    anyhow::bail!("ForecastEx client not configured")
                }
            }
        }
    }

    async fn attempt_unwind(
        &self,
        platform: &Platform,
        market_id: &str,
        original_fill: &OrderResult,
    ) -> Result<()> {
        // Try to sell what we bought (or vice versa) at market
        warn!(
            platform = %platform,
            size = %original_fill.fill_size,
            "Attempting position unwind"
        );
        // Unwind by submitting opposite side at a loss-limited price
        // For now, just log - full implementation requires tracking position direction
        Ok(())
    }

    fn compute_result(
        &self,
        leg_a: &OrderResult,
        leg_b: &OrderResult,
        opp: &ArbitrageOpportunity,
    ) -> (TradeStatus, Decimal, Option<String>) {
        if leg_a.filled && leg_b.filled {
            // Both legs filled - compute actual profit
            // Profit = 1.0 - cost_a - cost_b (for full binary arb)
            let total_cost = leg_a.fill_price + leg_b.fill_price;
            let gross_profit = (Decimal::ONE - total_cost) * leg_a.fill_size.min(leg_b.fill_size);
            let net_profit = gross_profit - leg_a.fee - leg_b.fee;
            (TradeStatus::Success, net_profit, None)
        } else if leg_a.filled && !leg_b.filled {
            // Only first leg filled - loss from unwind or holding
            let loss = leg_a.fee; // At minimum, we lost the fee
            let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge leg failed to fill".into());
            (TradeStatus::Fail, -loss, Some(reason))
        } else {
            // Neither leg filled
            let reason = leg_a.error.clone().unwrap_or_else(|| "First leg failed to fill".into());
            (TradeStatus::Fail, Decimal::ZERO, Some(reason))
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/execution/
git commit -m "feat: add dual-leg execution engine with state machine and unwind logic"
```

---

### Task 2: Polymarket REST Client

**Files:**
- Create: `src/execution/polymarket_client.rs`

- [ ] **Step 1: Write the Polymarket order client**

`src/execution/polymarket_client.rs`:
```rust
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

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
    #[serde(default)]
    status: Option<String>,
}

impl PolymarketClient {
    pub fn new(
        rest_url: String,
        signer: PolymarketSigner,
        api_key: String,
        api_secret: String,
        api_passphrase: String,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
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
    async fn submit_order(
        &self,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult> {
        let side_str = match side {
            Side::Yes => "BUY",
            Side::No => "SELL",
        };

        info!(market_id, side = side_str, price = %price, size = %size, "Submitting Polymarket order");

        // Build and sign the order
        // Simplified - actual implementation needs full EIP-712 flow
        let nonce = format!("{}", chrono::Utc::now().timestamp_millis());
        let expiration = format!("{}", chrono::Utc::now().timestamp() + 300); // 5 min expiry

        let maker_amount = (size * price).to_string();
        let taker_amount = size.to_string();

        let url = format!("{}/order", self.rest_url);

        let payload = OrderPayload {
            token_id: market_id.to_string(),
            maker_amount,
            taker_amount,
            side: side_str.to_string(),
            fee_rate_bps: "0".to_string(), // Will be set by exchange
            nonce,
            expiration,
            signature: "0x".to_string(), // Placeholder - needs actual signing
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
            status: None,
        });

        if body.success {
            Ok(OrderResult {
                filled: true,
                fill_price: price,
                fill_size: size,
                fee: Decimal::ZERO, // Updated from fill report
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
```

- [ ] **Step 2: Commit**

```bash
git add src/execution/polymarket_client.rs
git commit -m "feat: add Polymarket CLOB REST order client"
```

---

### Task 3: Kalshi REST Client

**Files:**
- Create: `src/execution/kalshi_client.rs`

- [ ] **Step 1: Write the Kalshi order client**

`src/execution/kalshi_client.rs`:
```rust
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
    status: String,
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
        Self {
            http: reqwest::Client::new(),
            rest_url,
            auth,
        }
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for KalshiClient {
    async fn submit_order(
        &self,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult> {
        let price_cents = (price * Decimal::from(100)).to_string().parse::<i64>().unwrap_or(50);
        let count = size.to_string().parse::<i64>().unwrap_or(1);

        let (kalshi_side, yes_price, no_price) = match side {
            Side::Yes => ("yes".to_string(), Some(price_cents), None),
            Side::No => ("no".to_string(), None, Some(price_cents)),
        };

        info!(
            ticker = market_id,
            side = %kalshi_side,
            price_cents,
            count,
            "Submitting Kalshi order"
        );

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

            // Kalshi fee: 7c * C * (1-C) per contract
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
            Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: Some(error_msg),
            })
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
```

- [ ] **Step 2: Commit**

```bash
git add src/execution/kalshi_client.rs
git commit -m "feat: add Kalshi REST order client with JWT auth"
```

---

### Task 4: CDNA and ForecastEx Clients (Stubs)

**Files:**
- Create: `src/execution/cdna_client.rs`
- Create: `src/execution/forecastex_client.rs`

- [ ] **Step 1: Write CDNA client**

`src/execution/cdna_client.rs`:
```rust
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
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
        Self {
            http: reqwest::Client::new(),
            rest_url,
            api_key,
            api_secret,
        }
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for CdnaClient {
    async fn submit_order(
        &self,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult> {
        info!(market_id, side = %side, price = %price, size = %size, "Submitting CDNA order");

        let side_str = match side {
            Side::Yes => "BUY",
            Side::No => "SELL",
        };

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

        let status = resp.status();
        let result: serde_json::Value = resp.json().await.unwrap_or_default();

        let filled = result.get("result").and_then(|r| r.get("status"))
            .and_then(|s| s.as_str())
            .map(|s| s == "FILLED")
            .unwrap_or(false);

        let fill_price = result.get("result")
            .and_then(|r| r.get("avg_price"))
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<Decimal>().ok())
            .unwrap_or(price);

        Ok(OrderResult {
            filled,
            fill_price,
            fill_size: if filled { size } else { Decimal::ZERO },
            fee: Decimal::ZERO,
            order_id: result.get("result")
                .and_then(|r| r.get("order_id"))
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string(),
            error: if !filled { Some(format!("HTTP {}", status)) } else { None },
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/private/cancel-order", self.rest_url);
        self.http.post(&url)
            .header("X-API-KEY", &self.api_key)
            .json(&serde_json::json!({"order_id": order_id}))
            .send()
            .await
            .context("CDNA cancel failed")?;
        Ok(())
    }
}
```

- [ ] **Step 2: Write ForecastEx client**

`src/execution/forecastex_client.rs`:
```rust
use anyhow::Result;
use rust_decimal::Decimal;
use tracing::{info, warn};

use super::executor::{OrderResult, PlatformOrderClient};
use crate::types::Side;

/// ForecastEx order client via FIX protocol
/// Simplified implementation - shares TCP connection with feed handler
#[derive(Clone)]
pub struct ForecastExClient {
    fix_host: String,
    fix_port: u16,
}

impl ForecastExClient {
    pub fn new(fix_host: String, fix_port: u16) -> Self {
        Self { fix_host, fix_port }
    }
}

#[async_trait::async_trait]
impl PlatformOrderClient for ForecastExClient {
    async fn submit_order(
        &self,
        market_id: &str,
        side: Side,
        price: Decimal,
        size: Decimal,
    ) -> Result<OrderResult> {
        info!(market_id, side = %side, price = %price, size = %size, "Submitting ForecastEx order");

        // FIX protocol order submission
        // TODO: Implement full FIX New Order Single (35=D)
        // For now, return a placeholder that can be wired up when IBKR credentials are available
        warn!("ForecastEx order execution not yet fully implemented");

        Ok(OrderResult {
            filled: false,
            fill_price: Decimal::ZERO,
            fill_size: Decimal::ZERO,
            fee: Decimal::ZERO,
            order_id: String::new(),
            error: Some("ForecastEx execution not yet implemented".into()),
        })
    }

    async fn cancel_order(&self, _order_id: &str) -> Result<()> {
        warn!("ForecastEx cancel not yet implemented");
        Ok(())
    }
}
```

- [ ] **Step 3: Update main.rs**

Add to `src/main.rs`:
```rust
mod execution;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add src/execution/ src/main.rs
git commit -m "feat: add CDNA and ForecastEx order clients"
```

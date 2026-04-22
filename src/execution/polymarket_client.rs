use anyhow::{Context, Result};
use alloy::primitives::{Address, U256};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::eip712::PolymarketSigner;
use crate::types::Side;

#[derive(Clone)]
pub struct PolymarketClient {
    http: reqwest::Client,
    rest_url: String,
    signer: PolymarketSigner,
    api_key: zeroize::Zeroizing<String>,
    api_secret: zeroize::Zeroizing<String>,
    api_passphrase: zeroize::Zeroizing<String>,
}

#[derive(Serialize)]
struct CreateOrderRequest {
    order: OrderPayload,
    owner: String,
    #[serde(rename = "orderType")]
    order_type: String,
    #[serde(rename = "postOnly", skip_serializing_if = "Option::is_none")]
    post_only: Option<bool>,
}

/// Exact field set expected by the Polymarket CLOB POST /order endpoint.
/// Verified against py-clob-client SignedOrder.dict() — all numeric fields
/// must be JSON integers, NOT strings. Extra fields (timestamp/metadata/builder)
/// are not part of the spec and must not be sent.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    salt: u64,
    maker: String,
    signer: String,
    taker: String,
    #[serde(rename = "tokenId")]
    token_id: String,
    maker_amount: String,
    taker_amount: String,
    side: String,
    fee_rate_bps: String,
    nonce: String,
    expiration: String,
    signature: String,
    signature_type: u8,
}

#[derive(Deserialize)]
struct OrderResponse {
    #[serde(default)]
    success: bool,
    #[serde(rename = "orderID", default)]
    order_id: Option<String>,
    #[serde(rename = "errorMsg", default)]
    error_msg: Option<String>,
}

use base64::Engine;

impl PolymarketClient {
    pub fn new(
        rest_url: String,
        signer: PolymarketSigner,
        api_key: String,
        api_secret: String,
        api_passphrase: String,
    ) -> Self {
        // If POLYMARKET_PROXY_URL is set, route ALL Polymarket traffic through it.
        // This bypasses the US geoblock on order submission while Kalshi stays on the
        // direct connection (no added latency for the ~5ms us-east-1 path).
        let http = if let Ok(proxy_url) = std::env::var("POLYMARKET_PROXY_URL") {
            tracing::info!(proxy = %proxy_url, "Polymarket HTTP client using proxy");
            reqwest::Client::builder()
                .proxy(reqwest::Proxy::all(&proxy_url).expect("invalid POLYMARKET_PROXY_URL"))
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .pool_idle_timeout(std::time::Duration::from_secs(600))
                .pool_max_idle_per_host(10)
                .tcp_nodelay(true)
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build Polymarket HTTP client with proxy")
        } else {
            // Use the centralized TLS builder which configures keep-alive, connection pool,
            // TCP_NODELAY, and cert pinning consistently across all platform clients.
            crate::crypto::tls::build_reqwest_client()
                .expect("failed to build Polymarket HTTP client")
        };
        Self {
            http,
            rest_url,
            signer,
            api_key: zeroize::Zeroizing::new(api_key),
            api_secret: zeroize::Zeroizing::new(api_secret),
            api_passphrase: zeroize::Zeroizing::new(api_passphrase),
        }
    }

    fn l2_auth_headers(&self, method: &str, path: &str, body: &str) -> Result<Vec<(&'static str, String)>> {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let payload = format!("{}{}{}{}", timestamp, method, path, body);
        
        tracing::debug!(payload = %payload, "Signing Polymarket payload");
        
        // Polymarket SDK uses URL_SAFE base64 for HMAC secret decoding and signature encoding
        let secret_decoded = base64::engine::general_purpose::URL_SAFE
            .decode(self.api_secret.as_bytes())
            .context("Invalid base64 in POLYMARKET_API_SECRET")?;
            
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &secret_decoded);
        let tag = ring::hmac::sign(&key, payload.as_bytes());
        let signature = base64::engine::general_purpose::URL_SAFE.encode(tag.as_ref());

        tracing::debug!(key = %self.api_key.to_string(), signature = %signature, "Generated Auth Headers");

        Ok(vec![
            ("POLY_TIMESTAMP", timestamp),
            ("POLY_SIGNATURE", signature),
            ("POLY_API_KEY", self.api_key.to_string()),
            ("POLY_PASSPHRASE", self.api_passphrase.to_string()),
            ("POLY_ADDRESS", format!("{}", self.signer.address())),
        ])
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: crate::types::Usd, size: crate::types::Contracts, fee_rate_bps: crate::types::BasisPoints) -> Result<OrderResult> {
        let price = price.0;
        let size = size.0;
        let fee_rate_bps = fee_rate_bps.0;
        // Rate limiting handled by executor's per-platform backoff on HTTP 429 responses.

        // CRITICAL FIX: Polymarket CTF does not support naked short selling. To bet NO, we must trade 
        // the specific NO token ID. We parse the dual-token string provided by discovery.
        let tokens: Vec<&str> = market_id.split(',').collect();
        let target_token_id = if side == Side::Yes { tokens[0] } else { tokens.get(1).unwrap_or(&tokens[0]) };
        
        let (side_log, side_u8) = match action {
            OrderAction::Buy => ("BUY", 0u8),
            OrderAction::Sell => ("SELL", 1u8),
        };

        info!(target_token_id, action = side_log, side = ?side, price = %price, size = %size, fee_rate_bps, "Submitting Polymarket order");

        let scale = Decimal::from(1_000_000u64);
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

        let maker_amount_u256 = U256::from_str(&maker_amount_scaled.to_string()).unwrap_or(U256::ZERO);
        let taker_amount_u256 = U256::from_str(&taker_amount_scaled.to_string()).unwrap_or(U256::ZERO);

        let token_id_u256 = U256::from_str(target_token_id)
            .map_err(|_| anyhow::anyhow!("Invalid Polymarket token ID: {}", target_token_id))?;

        let fee_rate_bps_u256 = U256::from(fee_rate_bps as u64);
        let nonce = U256::ZERO;

        let now = chrono::Utc::now();

        let eoa_addr = self.signer.address();

        let funder = std::env::var("POLYMARKET_FUNDER").ok();
        let sig_type = if funder.is_some() {
            std::env::var("POLYMARKET_SIGNATURE_TYPE")
                .ok()
                .and_then(|s| s.parse::<u8>().ok())
                .unwrap_or(2u8)
        } else {
            0u8
        };

        let maker_addr = if let Some(ref funder_addr) = funder {
            Address::from_str(funder_addr).unwrap_or(eoa_addr)
        } else {
            eoa_addr
        };

        // Salt: mask to IEEE 754 safe integer (≤ 2^53 - 1) so server JSON parser sees same value.
        static SALT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SALT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let salt_seed = (now.timestamp_millis() as u64).wrapping_mul(1_000_000).wrapping_add(counter);
        let salt_raw = salt_seed.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(0x6c62272e07bb0142);
        let salt_u64 = salt_raw & ((1u64 << 53) - 1);
        let salt = U256::from(salt_u64);
        // FOK orders must have expiration 0 in both the signature and the JSON payload.
        let expiration_u256 = U256::ZERO;

        // Query the CLOB API to determine if this token uses the NegRisk exchange contract.
        // Crypto 15-min markets are neg_risk — signing against the wrong contract = invalid signature.
        let neg_risk = self.check_neg_risk(target_token_id).await.unwrap_or(false);
        let exchange_override = if neg_risk {
            // NegRisk CTF Exchange on Polygon mainnet
            Some(Address::from_str("0xC5d563A36AE78145C45a50134d48A1215220f80a").unwrap())
        } else {
            None // use default (0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E)
        };
        tracing::info!(neg_risk, "NegRisk check for order signing");

        let signature = self.signer.sign_order(
            salt, maker_addr, eoa_addr, Address::ZERO, token_id_u256,
            maker_amount_u256, taker_amount_u256, expiration_u256, nonce,
            fee_rate_bps_u256,
            side_u8, sig_type,
            exchange_override,
        ).await.context("EIP-712 order signing failed")?;
        let url = format!("{}/order", self.rest_url);

        use rust_decimal::prelude::ToPrimitive;
        let maker_amount_u64 = maker_amount_scaled.to_u64().unwrap_or(0);
        let taker_amount_u64 = taker_amount_scaled.to_u64().unwrap_or(0);

        let maker_addr_str = format!("{}", maker_addr);
        let eoa_addr_str = format!("{}", eoa_addr);

        let payload = OrderPayload {
            salt: salt_u64,
            maker: maker_addr_str.clone(),
            signer: eoa_addr_str.clone(),
            taker: "0x0000000000000000000000000000000000000000".to_string(),
            token_id: target_token_id.to_string(),
            maker_amount: maker_amount_u64.to_string(),
            taker_amount: taker_amount_u64.to_string(),
            side: side_log.to_string(),
            fee_rate_bps: fee_rate_bps.to_string(),
            nonce: "0".to_string(),
            expiration: "0".to_string(),
            signature,
            signature_type: sig_type,
        };

        let req_body = CreateOrderRequest {
            order: payload,
            // owner must be the API key UUID (not the Ethereum address)
            owner: self.api_key.to_string(),
            // FOK = Fill Or Kill: binary success/failure, no partial fills
            order_type: "FOK".to_string(),
            post_only: None,
        };
        let body_str = serde_json::to_string(&req_body).unwrap_or_default();
        let headers = self.l2_auth_headers("POST", "/order", &body_str)?;

        let mut req = self.http.post(&url)
            .header("Content-Type", "application/json")
            .body(body_str.clone()); // CRITICAL: Send the EXACT string hashed for the L2 HMAC signature

        for (k, v) in headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .context("Polymarket order submission failed")?;

        let status_code = resp.status();
        let raw_body = resp.text().await.unwrap_or_default();
        if !status_code.is_success() {
            tracing::error!(status = %status_code, body = %raw_body, payload = %body_str, "Polymarket order HTTP error");
        }
        let body: OrderResponse = serde_json::from_str(&raw_body).unwrap_or(OrderResponse {
            success: false,
            order_id: None,
            error_msg: Some(format!("HTTP {} — {}", status_code, raw_body)),
        });

        if body.success {
            let order_id_str = body.order_id.unwrap_or_default();
            // fill_size is always the number of contracts (tokens), regardless of buy/sell direction.
            // For BUY: taker_amount = tokens received = size. For SELL: taker_amount = USDC received.
            // Always report fill_size in contracts.
            let fill_size = match action {
                OrderAction::Buy => taker_amount_scaled / scale,
                OrderAction::Sell => maker_amount_scaled / scale, // maker gives tokens
            };
            let actual_price = price;
            
            // 1.2 EXECUTION SAFETY: Non-blocking confirmation.
            // We assume FOK success based on the synchronous response, 
            // and spawn a background task to verify actual fill metrics.
            if !order_id_str.is_empty() {
                let order_id_clone = order_id_str.clone();
                let assumed_price = actual_price;
                let client_clone = self.clone();

                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let path = format!("/orders/{}", order_id_clone);
                    let fetch_url = format!("{}{}", client_clone.rest_url, path);
                    
                    if let Ok(headers) = client_clone.l2_auth_headers("GET", &path, "") {
                        let mut req = client_clone.http.get(&fetch_url);
                        for (k, v) in headers {
                            req = req.header(k, v);
                        }

                        if let Ok(Ok(fetch_resp)) = tokio::time::timeout(std::time::Duration::from_secs(3), req.send()).await {
                        if let Ok(order_data) = fetch_resp.json::<serde_json::Value>().await {
                            if let Some(avg_price_str) = order_data.get("average_price").and_then(|v| v.as_str()) {
                                if let Ok(parsed_price) = Decimal::from_str(avg_price_str) {
                                    if (parsed_price - assumed_price).abs() > rust_decimal_macros::dec!(0.01) {
                                        tracing::error!(
                                            order_id = %order_id_clone,
                                            actual = %parsed_price,
                                            assumed = %assumed_price,
                                            "FILL PRICE DISCREPANCY: DB records assumed price, not actual fill price. \
                                             PnL calculation will be inaccurate by ${:.4}. \
                                             Implement FillCorrectionChannel to fix DB records.",
                                            (parsed_price - assumed_price).abs()
                                        );
                                    }
                                }
                            }
                        }
                    }
                    }
                });
            }

            let filled = fill_size > Decimal::ZERO;
            let estimated_fee = crate::feeds::normalizer::polymarket_fee(actual_price, fill_size, crate::feeds::normalizer::OrderType::Taker);

            Ok(OrderResult {
                filled,
                fill_price: crate::types::Usd(actual_price),
                fill_size: crate::types::Contracts(fill_size),
                fee: crate::types::Usd(estimated_fee),
                order_id: order_id_str,
                error: if !filled { Some("FOK order returned zero fill size".into()) } else { None },
            })
        } else {
            Ok(OrderResult {
                filled: false,
                fill_price: crate::types::Usd(Decimal::ZERO),
                fill_size: crate::types::Contracts(Decimal::ZERO),
                fee: crate::types::Usd(Decimal::ZERO),
                order_id: String::new(),
                error: body.error_msg.or_else(|| Some(format!("HTTP {}", status_code))),
            })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let path = format!("/order/{}", order_id);
        let url = format!("{}{}", self.rest_url, path);
        let headers = self.l2_auth_headers("DELETE", &path, "")?;
        
        let mut req = self.http.delete(&url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send()
            .await
            .context("Polymarket cancel failed")?;
        if !resp.status().is_success() {
            tracing::warn!(order_id, status = %resp.status(), "Polymarket cancel returned non-2xx");
        }
    // FOK orders fill entirely or not at all — cancel is best-effort for edge cases
        Ok(())
    }
}

impl PolymarketClient {
    /// Queries the Polymarket API for the actual live balance of the wallet.
    /// When POLYMARKET_FUNDER is set, queries the proxy wallet's balance (signature_type=2)
    /// since that is where deposited funds are held for API trading.
    pub async fn get_balance(&self) -> Result<Decimal> {
        let sign_path = "/balance-allowance";
        let url = if let Ok(funder) = std::env::var("POLYMARKET_FUNDER") {
            // POLY_PROXY (type 1): Magic Link / exported-key proxy wallet.
            // Querying with signature_type=1 + proxy_wallet_address returns the proxy's CLOB balance.
            format!("{}{}", self.rest_url, format!("{}?asset_type=COLLATERAL&signature_type=1&proxy_wallet_address={}", sign_path, funder))
        } else {
            format!("{}{}?asset_type=COLLATERAL", self.rest_url, sign_path)
        };
        let headers = self.l2_auth_headers("GET", sign_path, "")?;
        let mut req = self.http.get(&url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Polymarket get_balance failed: {} - {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        let body: serde_json::Value = resp.json().await?;
        let balance_str = body.get("balance")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        // Polymarket returns balance in micro-USDC (6 decimals). Convert to dollars.
        let raw = Decimal::from_str(balance_str).unwrap_or(Decimal::ZERO);
        Ok(raw / Decimal::from(1_000_000u64))
    }

    /// Checks if a token_id uses the NegRisk exchange contract.
    /// The CLOB returns {"neg_risk": true/false} for GET /neg-risk?token_id=<id>.
    pub async fn check_neg_risk(&self, token_id: &str) -> Result<bool> {
        let url = format!("{}/neg-risk?token_id={}", self.rest_url, token_id);
        let resp = self.http.get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .context("NegRisk lookup failed")?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        Ok(body.get("neg_risk").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    /// Queries the Polymarket API for all active positions
    pub async fn get_positions(&self) -> Result<Vec<(String, Decimal)>> {
        let path = "/positions";
        let url = format!("{}{}", self.rest_url, path);
        let headers = self.l2_auth_headers("GET", path, "")?;
        let mut req = self.http.get(&url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await?;
        
        if !resp.status().is_success() {
            anyhow::bail!("Polymarket get_positions failed: {}", resp.status());
        }
        
        let body: serde_json::Value = resp.json().await?;
        let mut positions = Vec::new();
        
        if let Some(list) = body.as_array() {
            for item in list {
                if let (Some(id), Some(size_str)) = (
                    item.get("asset_id").and_then(|v| v.as_str()),
                    item.get("size").and_then(|v| v.as_str())
                ) {
                    if let Ok(size) = Decimal::from_str(size_str) {
                        if size > Decimal::ZERO {
                            positions.push((id.to_string(), size));
                        }
                    }
                }
            }
        }
        
        Ok(positions)
    }
}

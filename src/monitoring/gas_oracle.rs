use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::{interval_at, Duration, Instant};
use tracing::{debug, error, info, warn};

/// Sent to the main event loop whenever live prices are refreshed.
#[derive(Debug, Clone)]
pub struct GasUpdate {
    /// Polygon network gas price in gwei.
    pub gas_gwei: u64,
    /// MATIC/USD spot price.
    pub matic_usd: Decimal,
}

pub struct GasOracle {
    rpc_url: String,
    poll_interval_secs: u64,
    update_tx: mpsc::Sender<GasUpdate>,
    http: reqwest::Client,
}

// ── JSON shapes for eth_gasPrice RPC ───────────────────────────────────────

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<String>,
    /// JSON-RPC error object returned with HTTP 200 on RPC-level errors.
    /// Must be captured to produce useful diagnostics instead of "null result".
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}


/// Alert if this many consecutive CoinGecko fetches fail (rate-limited or down).
const COINGECKO_ALERT_THRESHOLD: u32 = 5;

impl GasOracle {
    pub fn new(
        rpc_url: String,
        poll_interval_secs: u64,
        update_tx: mpsc::Sender<GasUpdate>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("mercury-gas-oracle/1.0")
            .build()
            .expect("failed to build gas oracle HTTP client");
        Self { rpc_url, poll_interval_secs, update_tx, http }
    }

    pub async fn run(self) {
        // Start with sane defaults so the engine is never un-initialised.
        let mut last_gwei: u64 = 50;
        let mut last_matic: Decimal = dec!(0.50);
        let mut coingecko_failures: u32 = 0;

        let poll_secs = self.poll_interval_secs.max(10); // floor at 10s

        // Use interval_at(now) so the FIRST tick fires immediately, pushing live
        // values to the spread engine before any arb detection begins.
        let mut ticker = interval_at(Instant::now(), Duration::from_secs(poll_secs));

        info!(
            interval_secs = poll_secs,
            rpc_url = %self.rpc_url,
            "Gas oracle started (first fetch is immediate)"
        );

        loop {
            ticker.tick().await;

            match self.fetch_gas_gwei().await {
                Ok(gwei) => {
                    if gwei != last_gwei {
                        info!(gwei, prev_gwei = last_gwei, "Polygon gas price updated");
                    } else {
                        debug!(gwei, "Polygon gas price unchanged");
                    }
                    last_gwei = gwei;
                }
                Err(e) => {
                    warn!(error = %e, last_gwei, "Failed to fetch Polygon gas price — keeping last value");
                }
            }

            match self.fetch_matic_usd().await {
                Ok(raw_price) => {
                    // CRITICAL FIX (3-D): Hard floor/ceiling to prevent severe oracle glitches
                    // from multiplying gas estimates by 1000x and breaking the spread math.
                    let price = raw_price.clamp(dec!(0.20), dec!(5.00));
                    
                    let change_pct = if last_matic > Decimal::ZERO {
                        ((price - last_matic) / last_matic * Decimal::from(100)).abs()
                    } else {
                        Decimal::from(100)
                    };
                    if change_pct > dec!(1) {
                        info!(matic_usd = %price, prev = %last_matic, "MATIC/USD price updated");
                    } else {
                        debug!(matic_usd = %price, "MATIC/USD price refreshed");
                    }
                    last_matic = price;
                    coingecko_failures = 0;
                }
                Err(e) => {
                    coingecko_failures += 1;
                    if coingecko_failures >= COINGECKO_ALERT_THRESHOLD {
                        error!(
                            failures = coingecko_failures,
                            last_matic = %last_matic,
                            error = %e,
                            "CoinGecko MATIC/USD fetch has failed {} consecutive times — \
                             gas costs may be stale, arb profitability estimates unreliable",
                            coingecko_failures
                        );
                    } else {
                        warn!(
                            error = %e,
                            last_matic = %last_matic,
                            failures = coingecko_failures,
                            "Failed to fetch MATIC/USD — keeping last value"
                        );
                    }
                }
            }

            let update = GasUpdate { gas_gwei: last_gwei, matic_usd: last_matic };
            // Use try_send (non-blocking) so the oracle never blocks the main loop
            if let Err(e) = self.update_tx.try_send(update) {
                match e {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => {
                        warn!("Gas oracle channel full — skipping update");
                    }
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                        debug!("Gas oracle channel closed — exiting cleanly");
                        return;
                    }
                }
            }
        }
    }

    /// Call `eth_gasPrice` on the configured Polygon RPC endpoint.
    async fn fetch_gas_gwei(&self) -> Result<u64> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_gasPrice",
            "params": [],
            "id": 1
        });

        let resp: RpcResponse = self.http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        // Surface JSON-RPC errors (returned with HTTP 200 by most RPC providers).
        if let Some(rpc_err) = resp.error {
            return Err(anyhow::anyhow!(
                "Polygon RPC error {}: {}",
                rpc_err.code,
                rpc_err.message
            ));
        }

        let hex = resp.result
            .ok_or_else(|| anyhow::anyhow!("eth_gasPrice returned null result (no error field)"))?;

        // Result is a hex string like "0x..." representing wei
        let hex_stripped = hex.trim_start_matches("0x");
        let wei = u64::from_str_radix(hex_stripped, 16)
            .map_err(|e| anyhow::anyhow!("failed to parse gas price hex '{}': {}", hex, e))?;
        let gwei = wei / 1_000_000_000;
        Ok(gwei.max(1)) // floor at 1 gwei to avoid zero gas cost in spread calc
    }

    /// Fetch MATIC/USD (or POL/USD) from CoinGecko's free simple/price endpoint.
    /// Tries both the legacy `matic-network` and new `polygon-ecosystem-token` IDs.
    async fn fetch_matic_usd(&self) -> Result<Decimal> {
        // Try the current ID first, fall back to legacy
        for coin_id in &["polygon-ecosystem-token", "matic-network"] {
            match self.try_coingecko_price(coin_id).await {
                Ok(price) => return Ok(price),
                Err(e) => {
                    tracing::debug!(coin_id, error = %e, "CoinGecko fetch failed, trying next ID");
                }
            }
        }
        // MED-6: Binance fallback for stability
        match self.try_binance_price("MATICUSDT").await {
            Ok(price) => return Ok(price),
            Err(e) => tracing::debug!(error = %e, "Binance fallback failed"),
        }
        Err(anyhow::anyhow!("All price oracles failed for MATIC/POL price"))
    }

    async fn try_binance_price(&self, symbol: &str) -> Result<Decimal> {
        let http_resp = self.http.get("https://api.binance.com/api/v3/ticker/price")
            .query(&[("symbol", symbol)]).send().await?;
        if !http_resp.status().is_success() {
            return Err(anyhow::anyhow!("Binance HTTP {}", http_resp.status()));
        }
        let resp: serde_json::Value = http_resp.json().await?;
        let price_str = resp.get("price").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing price"))?;
        std::str::FromStr::from_str(price_str).map_err(|e| anyhow::anyhow!("Parse error: {}", e))
    }

    async fn try_coingecko_price(&self, coin_id: &str) -> Result<Decimal> {
        let http_resp = self.http
            .get("https://api.coingecko.com/api/v3/simple/price")
            .query(&[("ids", coin_id), ("vs_currencies", "usd")])
            .send()
            .await?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "CoinGecko HTTP {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            ));
        }

        let resp: serde_json::Value = http_resp.json().await?;
        let usd = resp.get(coin_id)
            .and_then(|v| v.get("usd"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("CoinGecko response missing '{}.usd' field", coin_id))?;

        if !usd.is_finite() {
            return Err(anyhow::anyhow!("CoinGecko value is not finite: {}", usd));
        }
        Decimal::from_f64(usd)
            .ok_or_else(|| anyhow::anyhow!("Failed to convert f64 to Decimal: {}", usd))
    }
}

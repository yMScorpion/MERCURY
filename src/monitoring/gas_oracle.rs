use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

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
}

// ── JSON shapes for CoinGecko simple/price ─────────────────────────────────

#[derive(Deserialize)]
struct CoinGeckoPrice {
    #[serde(rename = "matic-network")]
    matic_network: Option<MaticEntry>,
}

#[derive(Deserialize)]
struct MaticEntry {
    usd: f64,
}

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

        let poll_secs = self.poll_interval_secs.max(10); // floor at 10s
        let mut ticker = interval(Duration::from_secs(poll_secs));

        info!(
            interval_secs = poll_secs,
            rpc_url = %self.rpc_url,
            "Gas oracle started"
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
                Ok(price) => {
                    // Only log when price moves by more than 1% to avoid log spam.
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
                }
                Err(e) => {
                    warn!(error = %e, last_matic = %last_matic, "Failed to fetch MATIC/USD — keeping last value");
                }
            }

            let update = GasUpdate { gas_gwei: last_gwei, matic_usd: last_matic };
            if self.update_tx.send(update).await.is_err() {
                info!("Gas oracle channel closed — shutting down");
                return;
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

        let hex = resp.result
            .ok_or_else(|| anyhow::anyhow!("eth_gasPrice returned null result"))?;

        // Result is a hex string like "0x..." representing wei
        let hex_stripped = hex.trim_start_matches("0x");
        let wei = u64::from_str_radix(hex_stripped, 16)
            .map_err(|e| anyhow::anyhow!("failed to parse gas price hex '{}': {}", hex, e))?;
        let gwei = wei / 1_000_000_000;
        Ok(gwei.max(1)) // floor at 1 gwei to avoid zero gas cost in spread calc
    }

    /// Fetch MATIC/USD from CoinGecko's free (no-key) simple/price endpoint.
    async fn fetch_matic_usd(&self) -> Result<Decimal> {
        let resp: CoinGeckoPrice = self.http
            .get("https://api.coingecko.com/api/v3/simple/price")
            .query(&[("ids", "matic-network"), ("vs_currencies", "usd")])
            .send()
            .await?
            .json()
            .await?;

        let usd = resp.matic_network
            .ok_or_else(|| anyhow::anyhow!("CoinGecko response missing 'matic-network' field"))?
            .usd;

        Decimal::from_f64_retain(usd)
            .ok_or_else(|| anyhow::anyhow!("CoinGecko MATIC/USD value is not finite: {}", usd))
    }
}

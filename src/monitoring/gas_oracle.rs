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
            .tcp_keepalive(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(5)
            .tcp_nodelay(true)
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
                    // Hard floor/ceiling to prevent severe oracle glitches.
                    // MATIC historically traded up to ~$2.92. Ceiling at $10 gives headroom
                    // for appreciation while still blocking 1000x glitches.
                    let price = raw_price.clamp(dec!(0.10), dec!(10.00));
                    
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

    /// PHASE 4: Deterministic Consensus Oracle for Gas
    async fn fetch_gas_gwei(&self) -> Result<u64> {
        let (rpc_primary, rpc_ankr, gas_station) = tokio::join!(
            self.try_rpc_gas(&self.rpc_url),
            self.try_rpc_gas("https://rpc.ankr.com/polygon"),
            self.try_polygon_gas_station()
        );

        let mut values = Vec::new();
        if let Ok(g) = rpc_primary { values.push(g); }
        if let Ok(g) = rpc_ankr { values.push(g); }
        if let Ok(g) = gas_station { values.push(g); }

        if values.is_empty() {
            return Err(anyhow::anyhow!("Consensus Oracle Failure: All Gas sources down"));
        }

        values.sort_unstable();
        let median = if values.len() % 2 == 0 {
            (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2
        } else {
            values[values.len() / 2]
        };

        Ok(median.max(1)) // Floor at 1 gwei
    }

    /// PHASE 4: Deterministic Consensus Oracle for MATIC/USD
    async fn fetch_matic_usd(&self) -> Result<Decimal> {
        let (cg, bin, cb) = tokio::join!(
            self.try_coingecko_price("polygon-ecosystem-token"),
            self.try_binance_price("MATICUSDT"),
            self.try_coinbase_price("MATIC-USD")
        );

        let mut prices = Vec::new();
        if let Ok(p) = cg { prices.push(p); }
        if let Ok(p) = bin { prices.push(p); }
        if let Ok(p) = cb { prices.push(p); }

        if prices.is_empty() {
            return Err(anyhow::anyhow!("Consensus Oracle Failure: All MATIC/USD sources down"));
        }

        prices.sort();
        let median = if prices.len() % 2 == 0 {
            (prices[prices.len() / 2 - 1] + prices[prices.len() / 2]) / dec!(2.0)
        } else {
            prices[prices.len() / 2]
        };

        Ok(median)
    }

    async fn try_rpc_gas(&self, url: &str) -> Result<u64> {
        let body = serde_json::json!({ "jsonrpc": "2.0", "method": "eth_gasPrice", "params": [], "id": 1 });
        let resp: RpcResponse = self.http.post(url).json(&body).send().await?.json().await?;
        if let Some(rpc_err) = resp.error { return Err(anyhow::anyhow!("RPC error: {}", rpc_err.message)); }
        let hex = resp.result.ok_or_else(|| anyhow::anyhow!("eth_gasPrice returned null result"))?;
        let wei = u64::from_str_radix(hex.trim_start_matches("0x"), 16)?;
        Ok(wei / 1_000_000_000)
    }

    async fn try_polygon_gas_station(&self) -> Result<u64> {
        let resp: serde_json::Value = self.http.get("https://gasstation.polygon.technology/v2").send().await?.json().await?;
        let standard = resp.pointer("/standard/maxFee").and_then(|v| v.as_f64()).ok_or_else(|| anyhow::anyhow!("Gas station missing maxFee"))?;
        Ok(standard.round() as u64)
    }

    async fn try_binance_price(&self, symbol: &str) -> Result<Decimal> {
        let resp: serde_json::Value = self.http.get("https://api.binance.com/api/v3/ticker/price").query(&[("symbol", symbol)]).send().await?.json().await?;
        let price_str = resp.get("price").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing price"))?;
        std::str::FromStr::from_str(price_str).map_err(Into::into)
    }

    async fn try_coinbase_price(&self, pair: &str) -> Result<Decimal> {
        let url = format!("https://api.coinbase.com/v2/prices/{}/spot", pair);
        let resp: serde_json::Value = self.http.get(&url).send().await?.json().await?;
        let price_str = resp.pointer("/data/amount").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Coinbase missing amount"))?;
        std::str::FromStr::from_str(price_str).map_err(Into::into)
    }

    async fn try_coingecko_price(&self, coin_id: &str) -> Result<Decimal> {
        let resp: serde_json::Value = self.http.get("https://api.coingecko.com/api/v3/simple/price").query(&[("ids", coin_id), ("vs_currencies", "usd")]).send().await?.json().await?;
        let usd = resp.get(coin_id).and_then(|v| v.get("usd")).and_then(|v| v.as_f64()).ok_or_else(|| anyhow::anyhow!("Missing usd field"))?;
        if !usd.is_finite() { return Err(anyhow::anyhow!("Non-finite value")); }
        rust_decimal::Decimal::from_f64(usd).ok_or_else(|| anyhow::anyhow!("Decimal conversion failed"))
    }
}
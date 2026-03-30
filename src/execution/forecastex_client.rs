use anyhow::Result;
use rust_decimal::Decimal;
use tracing::{info, warn};

use super::executor::{OrderResult, PlatformOrderClient};
use crate::types::Side;

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

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for ForecastExClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, _fee_rate_bps: u32) -> Result<OrderResult> {
        info!(market_id, action = ?action, side = %side, price = %price, size = %size, "ForecastEx order rejected — FIX execution not yet implemented");
        warn!(
            fix_host = %self.fix_host,
            fix_port = self.fix_port,
            "ForecastEx FIX execution not implemented — rejecting order to prevent unhedged leg-A positions"
        );
        // Return Err so the executor aborts the arb opportunity BEFORE executing any
        // counterpart leg. An Ok(filled: false) response only blocks leg-B but still
        // allows leg-A to run, which would require an unwind trade on a real platform.
        anyhow::bail!("ForecastEx FIX execution not yet implemented")
    }

    async fn cancel_order(&self, _order_id: &str) -> Result<()> {
        warn!("ForecastEx cancel not yet implemented");
        Ok(())
    }
}

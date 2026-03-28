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

#[async_trait::async_trait]
impl PlatformOrderClient for ForecastExClient {
    async fn submit_order(&self, market_id: &str, side: Side, price: Decimal, size: Decimal) -> Result<OrderResult> {
        info!(market_id, side = %side, price = %price, size = %size, "Submitting ForecastEx order");
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

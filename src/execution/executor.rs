use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::db::Database;
use crate::types::*;
use super::polymarket_client::PolymarketClient;
use super::kalshi_client::KalshiClient;
use super::cdna_client::CdnaClient;
use super::forecastex_client::ForecastExClient;

#[derive(Debug, Clone)]
pub struct OrderResult {
    pub filled: bool,
    pub fill_price: Decimal,
    pub fill_size: Decimal,
    pub fee: Decimal,
    pub order_id: String,
    pub error: Option<String>,
}

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

        let (first_leg, second_leg) = self.order_legs(opp);

        let first_result = self.execute_leg(
            &first_leg.platform,
            &opp.market_id.to_string(),
            first_leg.side,
            first_leg.price,
            validated.approved_size,
        ).await;

        let (leg_a_result, leg_b_result) = match first_result {
            Ok(first_fill) if first_fill.filled => {
                let hedge_size = first_fill.fill_size;
                let second_result = self.execute_leg(
                    &second_leg.platform,
                    &opp.market_id.to_string(),
                    second_leg.side,
                    second_leg.price,
                    hedge_size,
                ).await;

                match second_result {
                    Ok(second_fill) if second_fill.filled => (first_fill, second_fill),
                    Ok(second_fill) => {
                        warn!(opp_id = %opp.opp_id, "Hedge leg failed, attempting unwind");
                        let _ = self.attempt_unwind(&first_leg.platform, &opp.market_id.to_string(), first_leg.side, &first_fill).await;
                        (first_fill, second_fill)
                    }
                    Err(e) => {
                        error!(error = %e, "Hedge leg error, attempting unwind");
                        let _ = self.attempt_unwind(&first_leg.platform, &opp.market_id.to_string(), first_leg.side, &first_fill).await;
                        let failed = OrderResult { filled: false, fill_price: Decimal::ZERO, fill_size: Decimal::ZERO, fee: Decimal::ZERO, order_id: String::new(), error: Some(e.to_string()) };
                        (first_fill, failed)
                    }
                }
            }
            Ok(first_fill) => {
                let failed = OrderResult { filled: false, fill_price: Decimal::ZERO, fill_size: Decimal::ZERO, fee: Decimal::ZERO, order_id: String::new(), error: None };
                (first_fill, failed)
            }
            Err(e) => {
                error!(error = %e, "First leg execution error");
                let failed = OrderResult { filled: false, fill_price: Decimal::ZERO, fill_size: Decimal::ZERO, fee: Decimal::ZERO, order_id: String::new(), error: Some(e.to_string()) };
                let failed2 = failed.clone();
                (failed, failed2)
            }
        };

        let execution_ms = start.elapsed().as_millis() as u64;
        let (status, profit, failure_reason) = self.compute_result(&leg_a_result, &leg_b_result, opp);

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

        let _ = self.db.insert_trade(&trade_result).await;
        let _ = self.trade_result_tx.try_send(trade_result.clone());
        let _ = self.alert_tx.try_send(AlertMessage::TradeComplete(trade_result));

        Ok(())
    }

    fn order_legs<'a>(&self, opp: &'a ArbitrageOpportunity) -> (&'a LegDetail, &'a LegDetail) {
        if opp.leg_a.available_size <= opp.leg_b.available_size {
            (&opp.leg_a, &opp.leg_b)
        } else {
            (&opp.leg_b, &opp.leg_a)
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

    /// Hedge an open one-sided position after the second leg fails.
    /// Submits an aggressive contra-order (opposite outcome) so the position
    /// is always matched: YES + NO = $1 regardless of market outcome.
    async fn attempt_unwind(
        &self,
        platform: &Platform,
        market_id: &str,
        original_side: Side,
        original_fill: &OrderResult,
    ) -> Result<()> {
        if !original_fill.filled || original_fill.fill_size == Decimal::ZERO {
            return Ok(());
        }

        warn!(
            platform = %platform,
            side = ?original_side,
            size = %original_fill.fill_size,
            "Attempting position unwind via contra-order"
        );

        // Buy the complementary outcome at an aggressive limit so the IOC order
        // fills immediately at the current market price.
        // Buying the complement always creates a $1 pay-off regardless of result.
        let (contra_side, contra_price) = match original_side {
            // Had bought YES → buy NO at up to 0.99 (fills at current NO ask ~0.50)
            Side::Yes => (Side::No, dec!(0.99)),
            // Had bought NO  → buy YES at up to 0.99 (fills at current YES ask ~0.50)
            Side::No => (Side::Yes, dec!(0.99)),
        };

        match self.execute_leg(platform, market_id, contra_side, contra_price, original_fill.fill_size).await {
            Ok(result) if result.filled => {
                info!(platform = %platform, "Unwind contra-order filled — position hedged");
            }
            Ok(_) => {
                error!(platform = %platform, "Unwind contra-order did not fill — position may be unhedged, manual review required");
            }
            Err(e) => {
                error!(error = %e, platform = %platform, "Unwind contra-order failed — position may be unhedged, manual review required");
            }
        }

        Ok(())
    }

    fn compute_result(&self, leg_a: &OrderResult, leg_b: &OrderResult, _opp: &ArbitrageOpportunity) -> (TradeStatus, Decimal, Option<String>) {
        if leg_a.filled && leg_b.filled {
            let total_cost = leg_a.fill_price + leg_b.fill_price;
            let gross_profit = (Decimal::ONE - total_cost) * leg_a.fill_size.min(leg_b.fill_size);
            let net_profit = gross_profit - leg_a.fee - leg_b.fee;
            (TradeStatus::Success, net_profit, None)
        } else if leg_a.filled && !leg_b.filled {
            let loss = leg_a.fee;
            let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge leg failed to fill".into());
            (TradeStatus::Fail, -loss, Some(reason))
        } else {
            let reason = leg_a.error.clone().unwrap_or_else(|| "First leg failed to fill".into());
            (TradeStatus::Fail, Decimal::ZERO, Some(reason))
        }
    }
}

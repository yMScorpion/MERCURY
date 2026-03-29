use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use rust_decimal::prelude::ToPrimitive;

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
        fee_rate_bps: u32,
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
        initial_trade_count: i64,
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
            trade_counter: initial_trade_count,
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
            &first_leg.platform_market_id,
            first_leg.side,
            first_leg.price,
            validated.approved_size,
            first_leg.fee_rate_bps, // Pass actual BPS rate
        ).await;

        let (leg_a_result, leg_b_result) = match first_result {
            Ok(first_fill) if first_fill.filled => {
                let hedge_size = first_fill.fill_size; 
                let second_result = self.execute_leg(
                    &second_leg.platform,
                    &second_leg.platform_market_id,
                    second_leg.side,
                    second_leg.price,
                    hedge_size,
                    second_leg.fee_rate_bps, // Pass actual BPS rate
                ).await;

                match second_result {
                    Ok(second_fill) if second_fill.filled => {
                        (first_fill, second_fill)
                    }
                    Ok(second_fill) => {
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
                    let err_str = e.to_string();
                    let failed = OrderResult {
                        filled: false,
                        fill_price: Decimal::ZERO,
                        fill_size: Decimal::ZERO,
                        fee: Decimal::ZERO,
                        order_id: String::new(),
                        error: Some(err_str.clone()),
                    };
                    let failed2 = OrderResult {
                        filled: false,
                        fill_price: Decimal::ZERO,
                        fill_size: Decimal::ZERO,
                        fee: Decimal::ZERO,
                        order_id: String::new(),
                        error: Some(err_str),
                    };
                    (failed, failed2)
                    }
                }
            }
            Ok(first_fill) => {
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
            approved_size: validated.approved_size,
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

        // NOTIFY IMMEDIATELY so we do not block for IO
        let _ = self.trade_result_tx.send(trade_result.clone()).await;
        let _ = self.alert_tx.send(AlertMessage::TradeComplete(trade_result.clone())).await;

        // NON-BLOCKING SQL DB INSERT
        let db_clone = self.db.clone();
        tokio::spawn(async move {
            if let Err(e) = db_clone.insert_trade(&trade_result).await {
                tracing::error!(error = %e, "Failed to persist trade result to database");
            }
        });

        Ok(())
    }

    fn order_legs<'a>(&self, opp: &'a ArbitrageOpportunity) -> (&'a LegDetail, &'a LegDetail) {
        let a_liquidity = opp.leg_a.available_size;
        let b_liquidity = opp.leg_b.available_size;

        if a_liquidity <= b_liquidity {
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
        fee_rate_bps: u32,
    ) -> Result<OrderResult> {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                if let Some(client) = &self.polymarket_client {
                    client.submit_order(market_id, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Polymarket client not configured")
                }
            }
            Platform::Kalshi => {
                if let Some(client) = &self.kalshi_client {
                    client.submit_order(market_id, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Kalshi client not configured")
                }
            }
            Platform::Cdna => {
                if let Some(client) = &self.cdna_client {
                    client.submit_order(market_id, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("CDNA client not configured")
                }
            }
            Platform::ForecastEx => {
                if let Some(client) = &self.forecastex_client {
                    client.submit_order(market_id, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("ForecastEx client not configured")
                }
            }
        }
    }

    async fn attempt_unwind(
        &self,
        platform: &Platform,
        _market_id: &str,
        original_fill: &OrderResult,
    ) -> Result<()> {
        warn!(
            platform = %platform,
            size = %original_fill.fill_size,
            "Attempting position unwind"
        );
        Ok(())
    }

    fn compute_result(
        &self,
        leg_a: &OrderResult,
        leg_b: &OrderResult,
        _opp: &ArbitrageOpportunity,
    ) -> (TradeStatus, Decimal, Option<String>) {
        if leg_a.filled && leg_b.filled {
            let total_cost = leg_a.fill_price + leg_b.fill_price;
            let gross_profit = (Decimal::ONE - total_cost) * leg_a.fill_size.min(leg_b.fill_size);
            let net_profit = gross_profit - leg_a.fee - leg_b.fee;
            
            let status = if leg_a.fill_size == leg_b.fill_size {
                TradeStatus::Success
            } else {
                TradeStatus::Partial
            };
            
            (status, net_profit, None)
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
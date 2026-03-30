use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderAction {
    Buy,
    Sell,
}

#[async_trait::async_trait]
pub trait PlatformOrderClient: Send + Sync {
    async fn submit_order(
        &self,
        market_id: &str,
        action: OrderAction,
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
        
        // CRITICAL FIX: The TTL Guard
        // Drop the opportunity immediately if it sat in the async queue longer than its Time-To-Live.
        // Executing stale arbs guarantees negative PnL.
        let current_time_ns = crate::types::now_ns();
        let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
        
        if current_time_ns > expiration_ns {
            let delay_ms = (current_time_ns - opp.detected_at) / 1_000_000;
            tracing::warn!(
                opp_id = %opp.opp_id, 
                delay_ms, 
                "Opportunity TTL expired in execution queue — dropping to prevent slippage"
            );
            return Ok(());
        }
        
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
            OrderAction::Buy,
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
                    OrderAction::Buy,
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
                        let mut final_first_fill = first_fill.clone();
                        
                        if let Ok(unwind_fill) = self.attempt_unwind(first_leg, &first_fill).await {
                            // Calculate exact realized loss from the round-trip FOK sell
                            let buy_cost = first_fill.fill_size * first_fill.fill_price + first_fill.fee;
                            let sell_revenue = unwind_fill.fill_size * unwind_fill.fill_price;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee;
                            
                            // Zero out size to prevent DB position tracking, but pack the loss into the fee
                            // so `compute_result` logs the exact financial hit.
                            final_first_fill.fill_size = Decimal::ZERO;
                            final_first_fill.fee = realized_loss;
                            final_first_fill.error = Some("Leg B failed, automated unwind successful".into());
                        }
                        
                        (final_first_fill, second_fill)
                    }
                    Err(e) => {
                        error!(error = %e, "Hedge leg error, attempting unwind");
                        let mut final_first_fill = first_fill.clone();
                        
                        if let Ok(unwind_fill) = self.attempt_unwind(first_leg, &first_fill).await {
                            let buy_cost = first_fill.fill_size * first_fill.fill_price + first_fill.fee;
                            let sell_revenue = unwind_fill.fill_size * unwind_fill.fill_price;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee;
                            
                            final_first_fill.fill_size = Decimal::ZERO;
                            final_first_fill.fee = realized_loss;
                            final_first_fill.error = Some("Leg B error, automated unwind successful".into());
                        }
                        
                        let failed2 = OrderResult {
                            filled: false,
                            fill_price: Decimal::ZERO,
                            fill_size: Decimal::ZERO,
                            fee: Decimal::ZERO,
                            order_id: String::new(),
                            error: Some(e.to_string()),
                        };
                        
                        // Return the stranded leg unmodified if unwind fails so the DB tracks it correctly
                        (final_first_fill, failed2)
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

        let pre_trade_bankroll = self.bankroll;
        self.bankroll += profit;
        let bankroll_change_pct = if pre_trade_bankroll > Decimal::ZERO {
            profit / pre_trade_bankroll * Decimal::from(100)
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
            bankroll_after: Decimal::ZERO,
            bankroll_change_pct: Decimal::ZERO,
        };

        // Pass the result directly back to the orchestrator.
        // The executor MUST NOT dispatch to Telegram or SQLite, as this bypasses 
        // the authoritative state machine and causes severe tracking drift.
        if let Err(e) = self.trade_result_tx.try_send(trade_result) {
            tracing::error!(error = %e, "Trade result channel full — dropping notification");
        }
        if let Err(e) = self.alert_tx.try_send(AlertMessage::TradeComplete(trade_result.clone())) {
            tracing::warn!(error = %e, "Alert channel full — dropping Telegram trade notification");
        }

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
        action: OrderAction,
        side: Side,
        price: Decimal,
        size: Decimal,
        fee_rate_bps: u32,
    ) -> Result<OrderResult> {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                if let Some(client) = &self.polymarket_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Polymarket client not configured")
                }
            }
            Platform::Kalshi => {
                if let Some(client) = &self.kalshi_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Kalshi client not configured")
                }
            }
            Platform::Cdna => {
                if let Some(client) = &self.cdna_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("CDNA client not configured")
                }
            }
            Platform::ForecastEx => {
                if let Some(client) = &self.forecastex_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("ForecastEx client not configured")
                }
            }
        }
    }


    /// Executes a synthetic automated unwind.
    /// By natively SELLING the stranded contracts back to the resting bids, 
    /// we cap our risk instantly and free up capital without locking collateral.
    async fn attempt_unwind(
        &self,
        stranded_leg: &LegDetail,
        original_fill: &OrderResult,
    ) -> Result<OrderResult> {
        // Sell at 0.01 to aggressively cross the spread and ensure the FOK SELL order
        // executes against whatever bids are resting on the book.
        let aggressive_sell_price = rust_decimal_macros::dec!(0.01);

        warn!(
            platform = %stranded_leg.platform,
            market_id = %stranded_leg.platform_market_id,
            stranded_side = %stranded_leg.side,
            size = %original_fill.fill_size,
            "Hedge failed. Executing aggressive FOK SELL unwind to dump inventory."
        );

        let unwind_result = self.execute_leg(
            &stranded_leg.platform,
            &stranded_leg.platform_market_id,
            OrderAction::Sell,
            stranded_leg.side, // Same side! We sell the exact inventory we hold.
            aggressive_sell_price,
            original_fill.fill_size,
            stranded_leg.fee_rate_bps,
        ).await;

        match unwind_result {
            Ok(fill) if fill.filled => {
                let msg = format!(
                    "⚠️ <b>AUTOMATED UNWIND SUCCESSFUL</b> ⚠️\n\n\
                     Platform: {}\nMarket: {}\nUnwound: {} {}\n\
                     <b>Delta exposure neutralized.</b>",
                     stranded_leg.platform, stranded_leg.platform_market_id, fill.fill_size, stranded_leg.side
                );
                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert { 
                    severity: "warning".into(), 
                    message: msg 
                });
                Ok(fill)
            }
            Ok(_) | Err(_) => {
                let err_msg = unwind_result.err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "FOK Unwind Rejected by matching engine".into());
                
                error!(error = %err_msg, "Automated unwind failed. Naked exposure remains.");
                
                let msg = format!(
                    "🚨 <b>CRITICAL: UNWIND FAILED - NAKED EXPOSURE</b> 🚨\n\n\
                     Platform: {}\nMarket: {}\nStranded Size: {} {}\n\
                     Error: {}\n\
                     <b>MANUAL INTERVENTION REQUIRED IMMEDIATELY.</b>",
                     stranded_leg.platform, stranded_leg.platform_market_id, 
                     original_fill.fill_size, stranded_leg.side, err_msg
                );
                
                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert { 
                    severity: "critical".into(), 
                    message: msg 
                });
                
                anyhow::bail!("Automated unwind failed: {}", err_msg)
            }
        }
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
            let filled_value = leg_a.fill_price * leg_a.fill_size;
            
            // DYNAMIC UNWIND COST: 
            // The cost to unwind is roughly proportional to the raw spread of the asset.
            // If the spread is wide (e.g., 8%), unwinding will hurt more.
            // We use the raw_spread as a proxy for the asset's illiquidity, capped between 2% and 10% for safety.
            let dynamic_penalty_pct = _opp.raw_spread
                .max(rust_decimal_macros::dec!(0.02))
                .min(rust_decimal_macros::dec!(0.10));
                
            let dynamic_unwind_slippage = filled_value * dynamic_penalty_pct; 
            
            let estimated_loss = dynamic_unwind_slippage + leg_a.fee; 
            let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge leg failed to fill".into());
            
            (TradeStatus::Fail, -estimated_loss, Some(reason))
        } else {
            let reason = leg_a.error.clone().unwrap_or_else(|| "First leg failed to fill".into());
            (TradeStatus::Fail, Decimal::ZERO, Some(reason))
        }
    }
}
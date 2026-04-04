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
    uob: Arc<tokio::sync::RwLock<crate::engine::order_book::UnifiedOrderBook>>,
    polymarket_client: Option<PolymarketClient>,
    kalshi_client: Option<KalshiClient>,
    cdna_client: Option<CdnaClient>,
    forecastex_client: Option<ForecastExClient>,
    trade_counter: i64,
    executed_opps: lru::LruCache<uuid::Uuid, ()>,
}

impl ExecutionEngine {
    pub fn new(
        rx: mpsc::Receiver<ValidatedOpportunity>,
        trade_result_tx: mpsc::Sender<TradeResult>,
        alert_tx: mpsc::Sender<AlertMessage>,
        db: Arc<dyn Database>,
        uob: Arc<tokio::sync::RwLock<crate::engine::order_book::UnifiedOrderBook>>,
        polymarket_client: Option<PolymarketClient>,
        kalshi_client: Option<KalshiClient>,
        cdna_client: Option<CdnaClient>,
        forecastex_client: Option<ForecastExClient>,
        initial_trade_count: i64,
    ) -> Self {
        Self {
            rx,
            trade_result_tx,
            alert_tx,
            db,
            uob,
            polymarket_client,
            kalshi_client,
            cdna_client,
            forecastex_client,
            trade_counter: initial_trade_count,
            executed_opps: lru::LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
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
        
        let halves: Vec<&str> = opp.market_question.split(" / ").collect();
        if halves.len() == 2 {
            let nums_a: Vec<f64> = halves[0].split_whitespace().filter_map(|w| w.replace("$", "").replace(",", "").parse().ok()).collect();
            let nums_b: Vec<f64> = halves[1].split_whitespace().filter_map(|w| w.replace("$", "").replace(",", "").parse().ok()).collect();
            if nums_a != nums_b && (!nums_a.is_empty() || !nums_b.is_empty()) {
                tracing::error!(question = %opp.market_question, "Mismatched numerical targets in execution. Aborting trade.");
                return Ok(());
            }
        }
        
        // HIGH-4: Idempotency Guard
        if self.executed_opps.put(opp.opp_id, ()).is_some() {
            tracing::warn!(opp_id = %opp.opp_id, "Duplicate opportunity execution prevented");
            return Ok(());
        }

        // CRITICAL FIX: The TTL Guard
        // Drop the opportunity immediately if it sat in the async queue longer than its Time-To-Live.
        // Executing stale arbs guarantees negative PnL.
        let current_time_ns = crate::types::now_ns();
        let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
        
        let current_trade_id = self.trade_counter;
        self.trade_counter += 1;

        if current_time_ns > expiration_ns {
            let delay_ms = (current_time_ns - opp.detected_at) / 1_000_000;
            tracing::warn!(
                opp_id = %opp.opp_id, 
                delay_ms, 
                "Opportunity TTL expired in execution queue — dropping to prevent slippage"
            );
            
            let trade_result = TradeResult {
                trade_id: current_trade_id,
                opp_id: opp.opp_id,
                market_id: opp.market_id,
                market_question: opp.market_question.clone(),
                leg_a_platform: opp.leg_a.platform,
                leg_a_side: opp.leg_a.side,
                leg_a_price: opp.leg_a.price,
                leg_a_size: Decimal::ZERO,
                leg_a_fill_price: Decimal::ZERO,
                leg_a_fee: Decimal::ZERO,
                leg_b_platform: opp.leg_b.platform,
                leg_b_side: opp.leg_b.side,
                leg_b_price: opp.leg_b.price,
                leg_b_size: Decimal::ZERO,
                leg_b_fill_price: Decimal::ZERO,
                leg_b_fee: Decimal::ZERO,
                raw_spread: opp.raw_spread,
                net_spread: opp.net_spread,
                profit: Decimal::ZERO,
                status: TradeStatus::Fail,
                failure_reason: Some("Opportunity TTL expired in execution queue".into()),
                execution_ms: 0,
                executed_at: chrono::Utc::now(),
                bankroll_after: Decimal::ZERO,
                bankroll_change_pct: Decimal::ZERO,
                approved_size: validated.approved_size,
            };
            let _ = self.trade_result_tx.try_send(trade_result);
            return Ok(());
        }

        // H-1 FIX: Pre-execution price slippage guard
        {
            let uob_guard = self.uob.read().await;
            let book_a = uob_guard.get_book(&opp.market_id, &opp.leg_a.platform);
            let book_b = uob_guard.get_book(&opp.market_id, &opp.leg_b.platform);
            
            if let (Some(ba), Some(bb)) = (book_a, book_b) {
                let (current_price_a, current_price_b) = match (opp.leg_a.side, opp.leg_b.side) {
                    (Side::Yes, Side::No) => (ba.best_ask().map(|x| x.0), bb.best_bid().map(|x| Decimal::ONE - x.0)),
                    (Side::No, Side::Yes) => (ba.best_bid().map(|x| Decimal::ONE - x.0), bb.best_ask().map(|x| x.0)),
                    _ => (None, None),
                };

                if let (Some(pa), Some(pb)) = (current_price_a, current_price_b) {
                    let current_raw_spread = Decimal::ONE - pa - pb;
                    // If spread shrunk by over 50%, abort execution
                    if current_raw_spread < opp.raw_spread * rust_decimal_macros::dec!(0.5) {
                        tracing::warn!(
                            opp_id = %opp.opp_id,
                            old_spread = %opp.raw_spread,
                            new_spread = %current_raw_spread,
                            "Pre-execution slippage guard triggered: spread closed before execution. Aborting."
                        );
                        return Ok(());
                    }
                } else {
                    tracing::warn!("Pre-execution slippage guard: Order book missing depth. Aborting.");
                    return Ok(());
                }
            }
        }

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

        // CRITICAL FIX (4-B): The execution engine sorts legs by liquidity to reduce slippage risk.
        // We must map the execution results back to the original Opportunity's Leg A and Leg B
        // to prevent database/audit-trail cross-contamination.
        let (actual_leg_a_result, actual_leg_b_result) = if first_leg.platform == opp.leg_a.platform {
            (&leg_a_result, &leg_b_result)
        } else {
            (&leg_b_result, &leg_a_result)
        };

        let trade_result = TradeResult {
            trade_id: current_trade_id,
            opp_id: opp.opp_id,
            market_id: opp.market_id,
            market_question: opp.market_question.clone(),
            approved_size: validated.approved_size,
            leg_a_platform: opp.leg_a.platform,
            leg_a_side: opp.leg_a.side,
            leg_a_price: opp.leg_a.price,
            leg_a_size: actual_leg_a_result.fill_size,
            leg_a_fill_price: actual_leg_a_result.fill_price,
            leg_a_fee: actual_leg_a_result.fee,
            leg_b_platform: opp.leg_b.platform,
            leg_b_side: opp.leg_b.side,
            leg_b_price: opp.leg_b.price,
            leg_b_size: actual_leg_b_result.fill_size,
            leg_b_fill_price: actual_leg_b_result.fill_price,
            leg_b_fee: actual_leg_b_result.fee,
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
        // The executor MUST NOT dispatch to Telegram or SQLite. 
        if let Err(e) = self.trade_result_tx.try_send(trade_result) {
            tracing::error!(error = %e, "Trade result channel full — dropping notification");
        }

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
        opp: &ArbitrageOpportunity,
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
            if leg_a.fill_size == Decimal::ZERO && leg_a.fee > Decimal::ZERO {
                let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge failed, unwind successful".into());
                (TradeStatus::Fail, -leg_a.fee, Some(reason))
            } else {
                // ESTIMATED loss — the position is stranded and requires manual resolution.
                // We book a conservative estimate to prevent the bankroll from over-stating capital.
                let filled_value = leg_a.fill_price * leg_a.fill_size;
                let dynamic_penalty_pct = opp.raw_spread
                    .max(rust_decimal_macros::dec!(0.02))
                    .min(rust_decimal_macros::dec!(0.10));
                let dynamic_unwind_slippage = filled_value * dynamic_penalty_pct; 
                let estimated_loss = dynamic_unwind_slippage + leg_a.fee; 
                let reason = leg_b.error.clone().unwrap_or_else(|| {
                    format!("Hedge leg failed — ESTIMATED loss ${:.2} (manual resolution required)", estimated_loss)
                });
                (TradeStatus::Fail, -estimated_loss, Some(reason))
            }
        } else {
            let reason = leg_a.error.clone().unwrap_or_else(|| "First leg failed to fill".into());
            (TradeStatus::Fail, Decimal::ZERO, Some(reason))
        }
    }
}
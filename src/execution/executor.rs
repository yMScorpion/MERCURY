use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
#[cfg(test)]
use rust_decimal::prelude::FromPrimitive;
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
    pub fill_price: Usd,
    pub fill_size: Contracts,
    pub fee: Usd,
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
        price: Usd,
        size: Contracts,
        fee_rate_bps: BasisPoints,
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
    executed_opps: lru::LruCache<uuid::Uuid, ()>,
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
            executed_opps: lru::LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
        }
    }

    pub async fn run(mut self) {
        info!("Execution engine started");
        loop {
            // HIGH-3 FIX: Use recv() with a periodic timeout so the task
            // doesn't block forever if the sender is dropped or the system
            // is shutting down. This allows the JoinSet monitor to detect
            // executor health issues within 5 seconds.
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.rx.recv(),
            ).await {
                Ok(Some(opp)) => {
                    if let Err(e) = self.execute_arbitrage(opp).await {
                        error!(error = %e, "Arbitrage execution error");
                    }
                }
                Ok(None) => {
                    // Channel closed — sender dropped, begin graceful shutdown
                    info!("Execution engine: opportunity channel closed, draining");
                    break;
                }
                Err(_) => {
                    // Timeout — no opportunities in 5s, just loop and check again.
                    // This prevents the task from appearing "stuck" to the JoinSet monitor.
                    continue;
                }
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
        
        // Trade ID is assigned by the DB via AUTOINCREMENT. Use 0 as placeholder.
        let current_trade_id: i64 = 0;

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

        // Note: The software pre-execution slippage guard was removed.
        // Because the Engine routes trades instantly and all client execution calls mandate 
        // 'FOK' (Fill Or Kill), the target exchange matching engine provides a native,
        // zero-latency slippage guard. If the price moves, the order simply fails to fill.

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
            Usd(first_leg.price),
            Contracts(validated.approved_size),
            BasisPoints(first_leg.fee_rate_bps),
        ).await;

        let (leg_a_result, leg_b_result) = match first_result {
            Ok(first_fill) if first_fill.filled => {
                let hedge_size = first_fill.fill_size; 
                let second_result = self.execute_leg(
                    &second_leg.platform,
                    &second_leg.platform_market_id,
                    OrderAction::Buy,
                    second_leg.side,
                    Usd(second_leg.price),
                    Contracts(hedge_size.0),
                    BasisPoints(second_leg.fee_rate_bps),
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
                            let buy_cost = first_fill.fill_size.0 * first_fill.fill_price.0 + first_fill.fee.0;
                            let sell_revenue = unwind_fill.fill_size.0 * unwind_fill.fill_price.0;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee.0;
                            
                            // Zero out size to prevent DB position tracking, but pack the loss into the fee
                            // so `compute_result` logs the exact financial hit.
                            final_first_fill.fill_size = Contracts(Decimal::ZERO);
                            final_first_fill.fee = Usd(realized_loss);
                            final_first_fill.error = Some("Leg B failed, automated unwind successful".into());
                        }
                        
                        (final_first_fill, second_fill)
                    }
                    Err(e) => {
                        error!(error = %e, "Hedge leg error, attempting unwind");
                        let mut final_first_fill = first_fill.clone();
                        
                        if let Ok(unwind_fill) = self.attempt_unwind(first_leg, &first_fill).await {
                            let buy_cost = first_fill.fill_size.0 * first_fill.fill_price.0 + first_fill.fee.0;
                            let sell_revenue = unwind_fill.fill_size.0 * unwind_fill.fill_price.0;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee.0;
                            
                            final_first_fill.fill_size = Contracts(Decimal::ZERO);
                            final_first_fill.fee = Usd(realized_loss);
                            final_first_fill.error = Some("Leg B error, automated unwind successful".into());
                        }
                        
                        let failed2 = OrderResult {
                            filled: false,
                            fill_price: Usd(Decimal::ZERO),
                            fill_size: Contracts(Decimal::ZERO),
                            fee: Usd(Decimal::ZERO),
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
                    fill_price: Usd(Decimal::ZERO),
                    fill_size: Contracts(Decimal::ZERO),
                    fee: Usd(Decimal::ZERO),
                    order_id: String::new(),
                    error: None,
                };
                (first_fill, failed)
            }
            Err(e) => {
                error!(error = %e, "First leg execution error");
                let failed = OrderResult {
                    filled: false,
                    fill_price: Usd(Decimal::ZERO),
                    fill_size: Contracts(Decimal::ZERO),
                    fee: Usd(Decimal::ZERO),
                    order_id: String::new(),
                    error: Some(e.to_string()),
                };
                let failed2 = failed.clone();
                (failed, failed2)
            }
        };

    let execution_ms = start.elapsed().as_millis() as u64;

        let (status, profit, failure_reason) = Self::compute_result(
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
            leg_a_size: actual_leg_a_result.fill_size.0,
            leg_a_fill_price: actual_leg_a_result.fill_price.0,
            leg_a_fee: actual_leg_a_result.fee.0,
            leg_b_platform: opp.leg_b.platform,
            leg_b_side: opp.leg_b.side,
            leg_b_price: opp.leg_b.price,
            leg_b_size: actual_leg_b_result.fill_size.0,
            leg_b_fill_price: actual_leg_b_result.fill_price.0,
            leg_b_fee: actual_leg_b_result.fee.0,
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
        price: Usd,
        size: Contracts,
        fee_rate_bps: BasisPoints,
    ) -> Result<OrderResult> {
        let fut = async {
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
        };
        tokio::time::timeout(std::time::Duration::from_secs(8), fut)
            .await
            .map_err(|_| anyhow::anyhow!("execute_leg timeout after 8s (platform={:?})", platform))?
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
            Usd(aggressive_sell_price),
            original_fill.fill_size,
            BasisPoints(stranded_leg.fee_rate_bps),
        ).await;

        match unwind_result {
            Ok(fill) if fill.filled => {
                let msg = format!(
                    "⚠️ <b>AUTOMATED UNWIND SUCCESSFUL</b> ⚠️\n\n\
                     Platform: {}\nMarket: {}\nUnwound: {} {}\n\
                     <b>Delta exposure neutralized.</b>",
                     stranded_leg.platform, stranded_leg.platform_market_id, fill.fill_size.0, stranded_leg.side
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
                     original_fill.fill_size.0, stranded_leg.side, err_msg
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
        leg_a: &OrderResult,
        leg_b: &OrderResult,
        opp: &ArbitrageOpportunity,
    ) -> (TradeStatus, Decimal, Option<String>) {
        if leg_a.filled && leg_b.filled {
            let total_cost = leg_a.fill_price.0 + leg_b.fill_price.0;
            let gross_profit = (Decimal::ONE - total_cost) * leg_a.fill_size.0.min(leg_b.fill_size.0);
            let net_profit = gross_profit - leg_a.fee.0 - leg_b.fee.0;
            
            let status = if leg_a.fill_size.0 == leg_b.fill_size.0 {
                TradeStatus::Success
            } else {
                TradeStatus::Partial
            };
            
            (status, net_profit, None)
        } else if leg_a.filled && !leg_b.filled {
            if leg_a.fill_size.0 == Decimal::ZERO && leg_a.fee.0 > Decimal::ZERO {
                let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge failed, unwind successful".into());
                (TradeStatus::Fail, -leg_a.fee.0, Some(reason))
            } else {
                let filled_value = leg_a.fill_price.0 * leg_a.fill_size.0;
                let dynamic_penalty_pct = opp.raw_spread
                    .max(rust_decimal_macros::dec!(0.02))
                    .min(rust_decimal_macros::dec!(0.10));
                let dynamic_unwind_slippage = filled_value * dynamic_penalty_pct; 
                let estimated_loss = dynamic_unwind_slippage + leg_a.fee.0; 
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

#[cfg(test)]
mod verification {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    proptest! {
        #[test]
        fn formal_verify_compute_result_safety(
            filled_a in any::<bool>(),
            price_a in 0.01f64..0.99,
            size_a in 1.0f64..1000.0,
            fee_a in 0.0f64..50.0,
            filled_b in any::<bool>(),
            price_b in 0.01f64..0.99,
            size_b in 1.0f64..1000.0,
            fee_b in 0.0f64..50.0,
        ) {
            let leg_a = OrderResult {
                filled: filled_a,
                fill_price: Usd(Decimal::from_f64(price_a).unwrap()),
                fill_size: Contracts(if filled_a { Decimal::from_f64(size_a).unwrap() } else { Decimal::ZERO }),
                fee: Usd(Decimal::from_f64(fee_a).unwrap()),
                order_id: "A".into(),
                error: None,
            };
            
            let leg_b = OrderResult {
                filled: filled_b,
                fill_price: Usd(Decimal::from_f64(price_b).unwrap()),
                fill_size: Contracts(if filled_b { Decimal::from_f64(size_b).unwrap() } else { Decimal::ZERO }),
                fee: Usd(Decimal::from_f64(fee_b).unwrap()),
                order_id: "B".into(),
                error: None,
            };

            let opp = ArbitrageOpportunity {
                opp_id: uuid::Uuid::new_v4(),
                market_id: uuid::Uuid::new_v4(),
                market_question: "".into(),
                leg_a: LegDetail { platform: Platform::Polymarket, platform_market_id: "".into(), fee_rate_bps: 0, side: Side::Yes, price: dec!(0), available_size: dec!(0), fee_estimate: dec!(0) },
                leg_b: LegDetail { platform: Platform::Kalshi, platform_market_id: "".into(), fee_rate_bps: 0, side: Side::No, price: dec!(0), available_size: dec!(0), fee_estimate: dec!(0) },
                raw_spread: dec!(0.05),
                net_spread: dec!(0.03),
                kelly_fraction: dec!(0),
                recommended_size: dec!(0),
                score: dec!(0),
                detected_at: 0,
                ttl_ms: 0,
            };

            let (status, profit, _) = ExecutionEngine::compute_result(&leg_a, &leg_b, &opp);

            // Formal state validation
            if filled_a && filled_b {
                prop_assert!(status == TradeStatus::Success || status == TradeStatus::Partial);
                let max_possible_gross = leg_a.fill_size.0.min(leg_b.fill_size.0) * Decimal::ONE;
                prop_assert!(profit <= max_possible_gross); // Profit must always be logically bounded
            } else if filled_a && !filled_b {
                prop_assert_eq!(status, TradeStatus::Fail);
                prop_assert!(profit <= Decimal::ZERO); // Unwinds or stranded legs must correctly book a loss
            } else {
                prop_assert_eq!(status, TradeStatus::Fail);
                prop_assert_eq!(profit, Decimal::ZERO);
            }
        }
    }
}
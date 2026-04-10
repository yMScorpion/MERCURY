use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::types::*;
use crate::engine::order_book::UnifiedOrderBook;
use crate::engine::detector::ArbitrageDetector;
use crate::engine::spread::NetSpreadEngine;
use crate::engine::market_registry::MarketRegistry;
use crate::risk::bankroll::BankrollHandle;
use crate::risk::circuit_breaker::{CircuitBreakers, CheckParams};
use crate::risk::kelly::KellyCalculator;
use crate::monitoring::metrics::Metrics;
use tracing::{debug, info, warn};

pub struct MarketActor {
    market_id: Uuid,
    uob_shard: UnifiedOrderBook,
    rx: mpsc::Receiver<NormalizedTick>,
    execution_tx: mpsc::Sender<ValidatedOpportunity>,
    detector: Arc<std::sync::RwLock<ArbitrageDetector>>,
    spread_engine: Arc<RwLock<NetSpreadEngine>>,
    bankroll: BankrollHandle,
    registry: Arc<RwLock<MarketRegistry>>,
    circuit_breakers: Arc<RwLock<CircuitBreakers>>,
    kelly: Arc<RwLock<KellyCalculator>>,
    open_positions: Arc<AtomicUsize>,
    in_flight_trades: Arc<AtomicUsize>,
    metrics: Arc<Metrics>,
}

impl MarketActor {
    pub fn spawn(
        market_id: Uuid,
        rx: mpsc::Receiver<NormalizedTick>,
        execution_tx: mpsc::Sender<ValidatedOpportunity>,
        detector: Arc<std::sync::RwLock<ArbitrageDetector>>,
        spread_engine: Arc<RwLock<NetSpreadEngine>>,
        bankroll: BankrollHandle,
        registry: Arc<RwLock<MarketRegistry>>,
        circuit_breakers: Arc<RwLock<CircuitBreakers>>,
        kelly: Arc<RwLock<KellyCalculator>>,
        open_positions: Arc<AtomicUsize>,
        in_flight_trades: Arc<AtomicUsize>,
        metrics: Arc<Metrics>,
    ) {
        let mut actor = Self {
            market_id,
            uob_shard: UnifiedOrderBook::new(),
            rx,
            execution_tx,
            detector,
            spread_engine,
            bankroll,
            registry,
            circuit_breakers,
            kelly,
            open_positions,
            in_flight_trades,
            metrics,
        };

        tokio::spawn(async move {
            actor.run().await;
        });
    }

    async fn run(&mut self) {
        debug!(market_id = %self.market_id, "MarketActor started");
         while let Some(tick) = self.rx.recv().await {
            let mid = tick.mid_price;
            let market_id = tick.market_id;
            self.uob_shard.update(&tick);

            debug!(
                market_id = %market_id,
                platform = ?tick.platform,
                bid = %tick.bid_price,
                ask = %tick.ask_price,
                mid = %mid,
                bid_size = %tick.bid_size,
                ask_size = %tick.ask_size,
                "Tick received — running arb detection"
            );

            // Update volatility before detection so the gate uses current vol multiplier
            self.detector.write().unwrap().update_volatility(market_id, mid);

            let opps = {
                self.detector.read().unwrap().detect_for_market(
                    &self.market_id,
                    &self.registry.read().unwrap(),
                    &self.uob_shard,
                    &self.spread_engine.read().unwrap(),
                    rust_decimal_macros::dec!(10_000.0) // Evaluate deep liquidity, sizing capped downstream by Kelly
                )
            };

            for opp in opps {
                let bankroll = self.bankroll.clone();
                let exec_tx = self.execution_tx.clone();
                let cbs = self.circuit_breakers.clone();
                let kelly = self.kelly.clone();
                let open_positions = self.open_positions.clone();
                let in_flight_trades = self.in_flight_trades.clone();
                let metrics = self.metrics.clone();
                
                tokio::spawn(async move {
                    // Pre-flight TTL check before spending bankroll actor capacity
                    let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
                    if crate::types::now_ns() > expiration_ns {
                        return; // TTL expired before capital reservation — nothing was committed
                    }

                    // 1. Fetch live exposure state from the Bankroll Actor
                    let state = bankroll.get_risk_state(opp.leg_a.platform, opp.leg_b.platform, opp.market_id).await;
                    // 2. Strict Arb-Aware Kelly Sizing
                    let (approved_size, risk_score) = {
                        let k = kelly.read().unwrap();
                        let win_prob = state.exec_success_rate.max(rust_decimal_macros::dec!(0.5));
                        let fraction = k.optimal_fraction(win_prob, opp.net_spread);
                        let ideal_usd = k.position_size(state.bankroll, win_prob, opp.net_spread, rust_decimal_macros::dec!(0.10));
                        
                        let combined_price = opp.leg_a.price + opp.leg_b.price;
                        let contracts = if combined_price > rust_decimal::Decimal::ZERO { ideal_usd / combined_price } else { rust_decimal::Decimal::ZERO };

                        let mut size = if opp.leg_a.platform == Platform::Kalshi || opp.leg_b.platform == Platform::Kalshi {
                            contracts.min(opp.recommended_size).floor() // Strict integers for Kalshi
                        } else {
                            contracts.min(opp.recommended_size).trunc_with_scale(2) // Max 2 decimal places to prevent balance rounding rejections
                        };

                        let max_notional = rust_decimal_macros::dec!(500.0);
                        if combined_price > rust_decimal::Decimal::ZERO {
                            size = size.min(max_notional / combined_price);
                        }

                        (size, fraction)
                    };

                    let mut too_small = false;
                    let mut scaled_size = approved_size;
                    
                    if matches!(opp.leg_a.platform, Platform::Polymarket | Platform::PolymarketUs) {
                        let required_usd = rust_decimal_macros::dec!(5.0);
                        let current_usd = scaled_size * opp.leg_a.price;
                        if current_usd < required_usd {
                            let min_size = (required_usd / opp.leg_a.price).ceil();
                            if min_size <= opp.recommended_size {
                                scaled_size = scaled_size.max(min_size);
                            } else {
                                too_small = true;
                            }
                        }
                    }
                    if matches!(opp.leg_b.platform, Platform::Polymarket | Platform::PolymarketUs) {
                        let required_usd = rust_decimal_macros::dec!(5.0);
                        let current_usd = scaled_size * opp.leg_b.price;
                        if current_usd < required_usd {
                            let min_size = (required_usd / opp.leg_b.price).ceil();
                            if min_size <= opp.recommended_size {
                                scaled_size = scaled_size.max(min_size);
                            } else {
                                too_small = true;
                            }
                        }
                    }
                    
                    // Re-apply Kalshi strict integer rule if size was scaled
                    if opp.leg_a.platform == Platform::Kalshi || opp.leg_b.platform == Platform::Kalshi {
                        scaled_size = scaled_size.ceil(); // round up to ensure we still meet the 5.0 minimum
                    }

                    if too_small || scaled_size <= rust_decimal::Decimal::ZERO { return; }
                    
                    // Use the potentially scaled size for circuit breakers and execution
                    let approved_size = scaled_size;

                    // 3. Evaluate 10-Gate Circuit Breakers
                    let combined_price = opp.leg_a.price + opp.leg_b.price;
                    let passed_cbs = {
                        let mut cb_guard = cbs.write().unwrap();
                        let params = CheckParams {
                            trade_size: approved_size * combined_price, // Convert contracts to USD notional
                            bankroll: state.bankroll,
                            daily_loss_pct: state.daily_loss_pct,
                            drawdown_pct: state.drawdown_pct,
                            platform_exposure_pct: state.platform_a_exposure_pct.max(state.platform_b_exposure_pct),
                            open_positions: open_positions.load(Ordering::Relaxed),
                            involves_polymarket: opp.leg_a.platform == Platform::Polymarket || opp.leg_b.platform == Platform::Polymarket,
                            ms_since_last_tick: metrics.ms_since_last_tick(),
                            market_exposure_pct: state.market_exposure_pct,
                            total_exposure_pct: state.total_exposure_pct,
                        };
                        cb_guard.check_all(&params).is_empty()
                    };

                    if !passed_cbs {
                        warn!(
                            opp_id = %opp.opp_id,
                            market_id = %opp.market_id,
                            approved_size = %approved_size,
                            "Opportunity BLOCKED by circuit breakers"
                        );
                        return;
                    }

                    // 4. Reserve Exact Capital and Execute
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let leg_a_exposure = approved_size * opp.leg_a.price;
                    let leg_b_exposure = approved_size * opp.leg_b.price;
                    
                    let _ = bankroll.tx.send(crate::risk::bankroll::BankrollMsg::ReserveCapital {
                        leg_a_exposure,
                        leg_b_exposure,
                        platform_a: opp.leg_a.platform,
                        platform_b: opp.leg_b.platform,
                        market_id: opp.market_id,
                        reply: reply_tx,
                    }).await;

                    match reply_rx.await {
                        Ok(true) => {
                            // Immediately increment the trackers so the next check respects the new capacity
                            open_positions.fetch_add(1, Ordering::Relaxed);
                            in_flight_trades.fetch_add(1, Ordering::Relaxed);
                            info!(
                                opp_id = %opp.opp_id,
                                market_id = %opp.market_id,
                                approved_size = %approved_size,
                                risk_score = %risk_score,
                                net_spread = %opp.net_spread,
                                "Capital reserved — forwarding opportunity to executor"
                            );
                            let validated = ValidatedOpportunity {
                                opportunity: opp,
                                approved_size,
                                risk_score
                            };
                            if let Err(e) = exec_tx.try_send(validated) {
                                warn!("Execution queue full, dropping opportunity: {}", e);
                                open_positions.fetch_sub(1, Ordering::Relaxed);
                                in_flight_trades.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                        Ok(false) => {
                            warn!(
                                opp_id = %opp.opp_id,
                                approved_size = %approved_size,
                                "Capital reservation DENIED by bankroll manager"
                            );
                        }
                        Err(e) => {
                            warn!(opp_id = %opp.opp_id, error = %e, "Bankroll reply channel error");
                        }
                    }
                });
            }
        }
    }
}
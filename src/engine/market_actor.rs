use std::sync::Arc;
use std::sync::RwLock;
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
#[allow(unused_imports)]
use tracing::{debug, info, warn};

pub struct MarketActor {
    market_id: Uuid,
    uob_shard: Arc<RwLock<UnifiedOrderBook>>,
    rx: mpsc::Receiver<NormalizedTick>,
    execution_tx: mpsc::Sender<ValidatedOpportunity>,
    detector: Arc<std::sync::RwLock<ArbitrageDetector>>,
    spread_engine: Arc<RwLock<NetSpreadEngine>>,
    bankroll: BankrollHandle,
    registry: Arc<RwLock<MarketRegistry>>,
    circuit_breakers: Arc<RwLock<CircuitBreakers>>,
    kelly: Arc<RwLock<KellyCalculator>>,
    metrics: Arc<Metrics>,
    alert_tx: mpsc::Sender<crate::types::AlertMessage>,
    cached_open_positions: Arc<std::sync::atomic::AtomicUsize>,
}

impl MarketActor {
    pub fn spawn(
        market_id: Uuid,
        rx: mpsc::Receiver<NormalizedTick>,
        execution_tx: mpsc::Sender<ValidatedOpportunity>,
        alert_tx: mpsc::Sender<crate::types::AlertMessage>,
        detector: Arc<std::sync::RwLock<ArbitrageDetector>>,
        spread_engine: Arc<RwLock<NetSpreadEngine>>,
        bankroll: BankrollHandle,
        registry: Arc<RwLock<MarketRegistry>>,
        circuit_breakers: Arc<RwLock<CircuitBreakers>>,
        kelly: Arc<RwLock<KellyCalculator>>,
        metrics: Arc<Metrics>,
        cached_open_positions: Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let mut actor = Self {
            market_id,
            uob_shard: Arc::new(RwLock::new(UnifiedOrderBook::new())),
            rx,
            execution_tx,
            alert_tx,
            detector,
            spread_engine,
            bankroll,
            registry,
            circuit_breakers,
            kelly,
            metrics,
            cached_open_positions,
        };

        tokio::spawn(async move {
            actor.run().await;
        });
    }

    async fn run(&mut self) {
        debug!(market_id = %self.market_id, "MarketActor started");

        let _alert_tx = self.alert_tx.clone();
        let metrics = self.metrics.clone();
        let bankroll = self.bankroll.clone();
        let execution_tx = self.execution_tx.clone();
        let circuit_breakers = self.circuit_breakers.clone();
        let kelly = self.kelly.clone();
        let detector = self.detector.clone();
        let market_id = self.market_id;
        let cached_open_positions = self.cached_open_positions.clone();
        let uob_shard = self.uob_shard.clone();

        while let Some(tick) = self.rx.recv().await {
            uob_shard.write().unwrap().update(&tick);
            self.metrics.inc_ticks_for_platform(tick.platform);

            let opps = {
                detector.read().unwrap().detect_for_market(
                    &market_id,
                    &self.registry.read().unwrap(),
                    &uob_shard.read().unwrap(),
                    &self.spread_engine.read().unwrap(),
                    rust_decimal_macros::dec!(500)
                )
            };

            for o in &opps {
                let opp = o.clone();
                let bankroll = bankroll.clone();
                let exec_tx = execution_tx.clone();
                let cbs = circuit_breakers.clone();
                let kelly = kelly.clone();
                let metrics = metrics.clone();
                let _alert_tx_inner = _alert_tx.clone();
                let cached_open_pos = cached_open_positions.clone();
                let uob_shard = uob_shard.clone();
                
                tokio::spawn(async move {
                    let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
                    if crate::types::now_ns() > expiration_ns {
                        tracing::warn!(opp_id = %opp.opp_id, "Opportunity TTL expired in market actor — dropping");
                        return;
                    }

                    let state = bankroll.get_risk_state(opp.leg_a.platform, opp.leg_b.platform, opp.market_id).await;
                    
                    let (approved_size, risk_score) = {
                        let k = kelly.read().unwrap();
                        let win_prob = state.exec_success_rate.max(rust_decimal_macros::dec!(0.5));
                        let fraction = k.optimal_fraction(win_prob, opp.net_spread);
                        let ideal_usd = k.position_size(state.bankroll, win_prob, opp.net_spread, rust_decimal_macros::dec!(0.10));
                        
                        let available_capital = state.bankroll * (rust_decimal::Decimal::ONE - state.total_exposure_pct);
                        let affordable_usd = ideal_usd.min(available_capital);

                        let combined_price = opp.leg_a.price + opp.leg_b.price;
                        let contracts = if combined_price > rust_decimal::Decimal::ZERO { affordable_usd / combined_price } else { rust_decimal::Decimal::ZERO };
                        let mut size = if opp.leg_a.platform == Platform::Kalshi || opp.leg_b.platform == Platform::Kalshi {
                            contracts.min(opp.recommended_size).floor()
                        } else {
                            contracts.min(opp.recommended_size).trunc_with_scale(2)
                        };

                        let max_notional = rust_decimal_macros::dec!(500.0);
                        if combined_price > rust_decimal::Decimal::ZERO { size = size.min(max_notional / combined_price); }
                        (size, fraction)
                    };

                    // Per-position stop-loss: if the current mid-prices have already moved
                    // against us by more than 3% since detection, the opportunity is stale
                    // and executing it is expected-negative. Drop it immediately rather than
                    // waiting for the unwind watchdog to fire 5 minutes later.
                    {
                        let current_spread = {
                            let uob = uob_shard.read().unwrap();
                            uob.get_book(&opp.market_id, &opp.leg_a.platform)
                                .and_then(|ba| uob.get_book(&opp.market_id, &opp.leg_b.platform)
                                    .map(|bb| {
                                        let live_price_a = match opp.leg_a.side {
                                            Side::Yes => ba.best_ask().map(|(p,_)| p).unwrap_or(rust_decimal::Decimal::ONE),
                                            Side::No => ba.best_bid().map(|(p,_)| rust_decimal::Decimal::ONE - p).unwrap_or(rust_decimal::Decimal::ONE),
                                        };
                                        let live_price_b = match opp.leg_b.side {
                                            Side::Yes => bb.best_ask().map(|(p,_)| p).unwrap_or(rust_decimal::Decimal::ONE),
                                            Side::No => bb.best_bid().map(|(p,_)| rust_decimal::Decimal::ONE - p).unwrap_or(rust_decimal::Decimal::ONE),
                                        };
                                        rust_decimal::Decimal::ONE - live_price_a - live_price_b
                                    }))
                        };
                        if let Some(live_spread) = current_spread {
                            let spread_decay = opp.raw_spread - live_spread;
                            let decay_threshold = rust_decimal_macros::dec!(0.03);
                            if spread_decay > decay_threshold {
                                tracing::warn!(
                                    opp_id = %opp.opp_id,
                                    detected_spread = %opp.raw_spread,
                                    live_spread = %live_spread,
                                    decay = %spread_decay,
                                    "Per-position stop-loss triggered: spread decayed beyond threshold before execution"
                                );
                                return;
                            }
                        }
                    }

                    let too_small = false;
                    let scaled_size = approved_size;
                    if too_small || scaled_size <= rust_decimal::Decimal::ZERO {
                        tracing::warn!(
                            opp_id = %opp.opp_id,
                            recommended_size = %opp.recommended_size,
                            approved_size = %approved_size,
                            "Opportunity dropped: Kelly sizing produced zero size"
                        );
                        return;
                    }
                    let approved_size = scaled_size;

                    let combined_price = opp.leg_a.price + opp.leg_b.price;
                    let (passed_cbs, _violations) = {
                        let mut cb_guard = cbs.write().unwrap();
                        let params = CheckParams {
                            trade_size: approved_size * combined_price,
                            bankroll: state.bankroll,
                            daily_loss_pct: state.daily_loss_pct,
                            drawdown_pct: state.drawdown_pct,
                            platform_exposure_pct: state.platform_a_exposure_pct.max(state.platform_b_exposure_pct),
                            open_positions: cached_open_pos.load(std::sync::atomic::Ordering::Relaxed),
                            involves_polymarket: opp.leg_a.platform == Platform::Polymarket || opp.leg_b.platform == Platform::Polymarket,
                            ms_since_last_tick: metrics.ms_since_last_tick(),
                            market_exposure_pct: state.market_exposure_pct,
                            total_exposure_pct: state.total_exposure_pct,
                        };
                        let v = cb_guard.check_all(&params);
                        (v.is_empty(), v)
                    };

                    if !passed_cbs {
                        let breaker_types: Vec<&str> = _violations.iter().map(|t| t.breaker_type.as_str()).collect();
                        tracing::warn!(opp_id = %opp.opp_id, breakers = ?breaker_types, "Opportunity dropped: circuit breaker check failed");
                        return;
                    }

                    let leg_a_exposure = approved_size * opp.leg_a.price;
                    let leg_b_exposure = approved_size * opp.leg_b.price;
                    
                    // Attempt capital reservation exactly once. If the bankroll actor
                    // cannot satisfy it, the opportunity is dropped cleanly — no spin loop,
                    // no risk of holding the task alive past TTL.
                    if crate::types::now_ns() > expiration_ns {
                        tracing::warn!(opp_id = %opp.opp_id, "Opportunity TTL expired before capital reservation");
                        return;
                    }
                    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
                    let send_result = bankroll.tx.send(crate::risk::bankroll::BankrollMsg::ReserveCapital {
                        leg_a_exposure,
                        leg_b_exposure,
                        platform_a: opp.leg_a.platform,
                        platform_b: opp.leg_b.platform,
                        market_id: opp.market_id,
                        reply: res_tx,
                    }).await;
                    if send_result.is_err() {
                        tracing::error!(opp_id = %opp.opp_id, "Bankroll actor channel closed — dropping opportunity");
                        return;
                    }
                    let reserved = matches!(res_rx.await, Ok(true));
                    if !reserved {
                        tracing::warn!(opp_id = %opp.opp_id, leg_a_exposure = %leg_a_exposure, leg_b_exposure = %leg_b_exposure, "Capital reservation failed — dropping opportunity");
                        return;
                    }
                    // Capital reserved — forward to executor. On send failure, release
                    // immediately so the capital is not stranded.
                    tracing::info!(opp_id = %opp.opp_id, approved_size = %approved_size, "Capital reserved — forwarding to executor");
                    let validated = ValidatedOpportunity { opportunity: opp.clone(), approved_size, risk_score };
                    if exec_tx.send(validated).await.is_err() {
                        tracing::warn!(opp_id = %opp.opp_id, "Executor channel closed — releasing reserved capital");
                        let _ = bankroll.tx.send(crate::risk::bankroll::BankrollMsg::ReleaseCapital {
                            leg_a_exposure,
                            leg_b_exposure,
                            platform_a: opp.leg_a.platform,
                            platform_b: opp.leg_b.platform,
                            market_id: opp.market_id,
                        }).await;
                    }
                });
            }
        }
    }
}

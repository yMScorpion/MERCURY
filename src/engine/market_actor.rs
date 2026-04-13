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
    uob_shard: UnifiedOrderBook,
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
            uob_shard: UnifiedOrderBook::new(),
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

        while let Some(tick) = self.rx.recv().await {
            self.uob_shard.update(&tick);
            self.metrics.inc_ticks_for_platform(tick.platform);

            let opps = {
                detector.read().unwrap().detect_for_market(
                    &market_id,
                    &self.registry.read().unwrap(),
                    &self.uob_shard,
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
                
                tokio::spawn(async move {
                    let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
                    if crate::types::now_ns() > expiration_ns { return; }

                    let state = bankroll.get_risk_state(opp.leg_a.platform, opp.leg_b.platform, opp.market_id).await;
                    
                    let (approved_size, risk_score) = {
                        let k = kelly.read().unwrap();
                        let win_prob = state.exec_success_rate.max(rust_decimal_macros::dec!(0.5));
                        let fraction = k.optimal_fraction(win_prob, opp.net_spread);
                        let ideal_usd = k.position_size(state.bankroll, win_prob, opp.net_spread, rust_decimal_macros::dec!(0.10));
                        
                        let combined_price = opp.leg_a.price + opp.leg_b.price;
                        let contracts = if combined_price > rust_decimal::Decimal::ZERO { ideal_usd / combined_price } else { rust_decimal::Decimal::ZERO };
                        let mut size = if opp.leg_a.platform == Platform::Kalshi || opp.leg_b.platform == Platform::Kalshi {
                            contracts.min(opp.recommended_size).floor()
                        } else {
                            contracts.min(opp.recommended_size).trunc_with_scale(2)
                        };

                        let max_notional = rust_decimal_macros::dec!(500.0);
                        if combined_price > rust_decimal::Decimal::ZERO { size = size.min(max_notional / combined_price); }
                        (size, fraction)
                    };

                    let too_small = false;
                    let scaled_size = approved_size;
                    // ... (size scaling logic)
                    if too_small || scaled_size <= rust_decimal::Decimal::ZERO { return; }
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

                    if !passed_cbs { return; }

                    let leg_a_exposure = approved_size * opp.leg_a.price;
                    let leg_b_exposure = approved_size * opp.leg_b.price;
                    
                    let mut reserved = false;
                    while !reserved {
                        let (res_tx, res_rx) = tokio::sync::oneshot::channel();
                        let _ = bankroll.tx.send(crate::risk::bankroll::BankrollMsg::ReserveCapital {
                            leg_a_exposure,
                            leg_b_exposure,
                            platform_a: opp.leg_a.platform,
                            platform_b: opp.leg_b.platform,
                            market_id: opp.market_id,
                            reply: res_tx,
                        }).await;

                        if let Ok(true) = res_rx.await {
                            reserved = true;
                            let validated = ValidatedOpportunity { opportunity: opp.clone(), approved_size, risk_score };
                            if let Err(_) = exec_tx.try_send(validated) {
                            let _ = bankroll.tx.send(crate::risk::bankroll::BankrollMsg::ReleaseCapital {
                                leg_a_exposure,
                                leg_b_exposure,
                                platform_a: opp.leg_a.platform,
                                platform_b: opp.leg_b.platform,
                                market_id: opp.market_id,
                            }).await;
                        }
                        } else {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                });
            }
        }
    }
}

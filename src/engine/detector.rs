use rust_decimal::Decimal;
use tracing::{debug, info};
use uuid::Uuid;

use crate::engine::market_registry::MarketRegistry;
use crate::engine::order_book::UnifiedOrderBook;
use crate::engine::spread::{NetSpreadEngine, SpreadResult};
use crate::types::*;

/// Rejection reason for the 5-gate pipeline
#[derive(Debug, Clone)]
pub enum RejectionReason {
    BelowSpreadThreshold(Decimal),
    InsufficientLiquidity { available: Decimal, required: Decimal },
    StaleData { age_ms: u64, max_ms: u64 },
    CorrelationExposure { current: Decimal, max: Decimal },
    RiskBudgetExceeded { reason: String },
}

/// Deterministic natural logarithm approximation using Taylor series for `rust_decimal`
/// Eliminates non-deterministic `f64` math across CPU architectures.
fn decimal_ln(mut x: Decimal) -> Decimal {
    if x <= Decimal::ZERO { return Decimal::ZERO; }
    if x == Decimal::ONE { return Decimal::ZERO; }
    
    let mut shifts = 0;
    let two = rust_decimal_macros::dec!(2.0);
    
    // Range reduction to [0.5, 1.5] for faster convergence
    while x > rust_decimal_macros::dec!(1.5) { x /= two; shifts += 1; }
    while x < rust_decimal_macros::dec!(0.5) { x *= two; shifts -= 1; }

    let z = (x - Decimal::ONE) / (x + Decimal::ONE);
    let z_squared = z * z;
    let mut term = z;
    let mut sum = z;
    let mut n = Decimal::ONE;

    for _ in 1..20 {
        term *= z_squared;
        n += two;
        let next_sum = sum + term / n;
        if next_sum == sum { break; }
        sum = next_sum;
    }
    
    // ln(2) ≈ 0.6931471805599453
    (sum * two) + (Decimal::from(shifts) * rust_decimal_macros::dec!(0.6931471805599453))
}

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub struct DetectorStats {
    pub opportunities_detected: AtomicU64,
    pub gate1_rejected: AtomicU64,
    pub gate2_rejected: AtomicU64,
    pub gate3_rejected: AtomicU64,
    pub gate4_rejected: AtomicU64,
    pub gate5_rejected: AtomicU64,
    pub opportunities_passed: AtomicU64,
}

pub struct ArbitrageDetector {
    min_spread: Decimal,
    min_order_size: Decimal,
    stale_timeout_ms: u64,
    max_concurrent: usize,
    active_arbs: AtomicUsize,
    pub stats: DetectorStats,
    paused_until_ns: AtomicU64,
    platform_liveness: dashmap::DashMap<Platform, bool>,
    /// Rolling 1-minute mid-price samples per market for volatility calculation
    price_history: dashmap::DashMap<Uuid, std::collections::VecDeque<(u64, Decimal)>>,
    /// Cached per-market volatility multiplier (1.0 = normal, 1.5 = high vol)
    volatility_multipliers: dashmap::DashMap<Uuid, Decimal>,
}

impl ArbitrageDetector {
    pub fn new(
        min_spread: Decimal,
        min_order_size: Decimal,
        stale_timeout_ms: u64,
        max_concurrent: usize,
    ) -> Self {
        Self {
            min_spread,
            min_order_size,
            stale_timeout_ms,
            max_concurrent,
            active_arbs: AtomicUsize::new(0),
            stats: DetectorStats::default(),
            paused_until_ns: AtomicU64::new(0),
            platform_liveness: dashmap::DashMap::new(),
            price_history: dashmap::DashMap::new(),
            volatility_multipliers: dashmap::DashMap::new(),
        }
    }

    pub fn set_platform_liveness(&self, platform: Platform, is_alive: bool) {
        self.platform_liveness.insert(platform, is_alive);
    }

    pub fn pause_detection_until(&self, ns: u64) {
        self.paused_until_ns.store(ns, Ordering::Relaxed);
    }

    // Refactored to require mutable access if updating core thresholds.
    pub fn update_thresholds(&mut self, min_spread: Decimal, stale_timeout_ms: u64, max_concurrent: usize) {
        self.min_spread = min_spread;
        self.stale_timeout_ms = stale_timeout_ms;
        self.max_concurrent = max_concurrent;
    }

    pub fn set_active_arbs(&self, count: usize) {
        self.active_arbs.store(count, Ordering::Relaxed);
    }

    /// Evaluates spreads ONLY for the specific market that just updated.
    /// Update rolling volatility for a market based on the current mid-price.
    /// Call this every time a tick arrives for a market.
    pub fn update_volatility(&self, market_id: Uuid, mid_price: Decimal) {
        let now = crate::types::now_ns();
        let mut history = self.price_history.entry(market_id).or_default();
        history.push_back((now, mid_price));

        // Keep only last 60 minutes of data
        let cutoff = now.saturating_sub(3_600_000_000_000);
        while history.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            history.pop_front();
        }

        // Compute 1-minute return standard deviation if we have enough data (≥10 samples)
        if history.len() < 10 {
            self.volatility_multipliers.insert(market_id, rust_decimal_macros::dec!(1.0));
            return;
        }
        // Sample at 1-minute intervals
        let minute_ns: u64 = 60_000_000_000;
        let mut returns: Vec<Decimal> = Vec::new();
        let mut prev: Option<(u64, Decimal)> = None;
        for &(t, p) in history.iter() {
            if let Some((pt, pp)) = prev {
                if t.saturating_sub(pt) >= minute_ns && pp > Decimal::ZERO {
                    let ret = (p - pp) / pp;
                    returns.push(ret);
                    prev = Some((t, p));
                }
            } else {
                prev = Some((t, p));
            }
        }
        if returns.len() < 5 {
            self.volatility_multipliers.insert(market_id, rust_decimal_macros::dec!(1.0));
            return;
        }
        let n = Decimal::from(returns.len() as u64);
        let mean = returns.iter().sum::<Decimal>() / n;
        let variance = returns.iter().map(|r| {
            let diff = *r - mean;
            diff * diff
        }).sum::<Decimal>() / n;
        // Historical average volatility baseline: ~0.005 std dev per minute for prediction markets
        let baseline_variance = rust_decimal_macros::dec!(0.000025); // 0.005^2
        let multiplier = if variance > baseline_variance * rust_decimal_macros::dec!(4.0) {
            rust_decimal_macros::dec!(1.5) // High vol: raise threshold 50%
        } else {
            rust_decimal_macros::dec!(1.0)
        };
        self.volatility_multipliers.insert(market_id, multiplier);
    }

    pub fn detect_for_market(
        &self,
        market_id: &Uuid,
        registry: &MarketRegistry,
        uob: &UnifiedOrderBook,
        spread_engine: &NetSpreadEngine,
        target_size: Decimal,
    ) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        // HIGH-1 FIX: Do not evaluate spreads if the engine is in a cooldown period
        // (e.g., recovering from a broadcast::Lagged event repopulating the order books).
        if crate::types::now_ns() < self.paused_until_ns.load(std::sync::atomic::Ordering::Relaxed) {
            return opportunities;
        }

        let pairs = match registry.get_arb_pairs_for_market(market_id) {
            Some(p) => p,
            None => {
                return opportunities;
            }
        };

        for pair in pairs {
            let a_alive = self.platform_liveness.get(&pair.platform_a).map(|v| *v).unwrap_or(true);
            let b_alive = self.platform_liveness.get(&pair.platform_b).map(|v| *v).unwrap_or(true);
            if !a_alive || !b_alive {
                continue;
            }

            if let Some(market) = registry.get_market(&pair.market_id) {
                let seconds_to_exp = (market.expiration - chrono::Utc::now()).num_seconds();
                if seconds_to_exp < 60 {
                    continue; 
                }
            } else {
                continue;
            }

            let book_a = match uob.get_book(&pair.market_id, &pair.platform_a) {
                Some(b) => b,
                None => continue,
            };
            let book_b = match uob.get_book(&pair.market_id, &pair.platform_b) {
                Some(b) => b,
                None => continue,
            };

            let spreads = spread_engine.compute_spreads(book_a, book_b, target_size);

            let vol_multiplier = self.volatility_multipliers
                .get(&pair.market_id)
                .map(|v| *v)
                .unwrap_or(rust_decimal_macros::dec!(1.0));

            for spread in spreads {
            self.stats.opportunities_detected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            match self.run_gates_with_vol(&spread, book_a, book_b, pair.confidence, vol_multiplier) {
                Ok(()) => {
                    self.stats.opportunities_passed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // Fetch the native exchange identifiers and fee rates from the registry
                        let info_a = registry.get_platform_info(&pair.market_id, &pair.platform_a);
                        let info_b = registry.get_platform_info(&pair.market_id, &pair.platform_b);
                        
                        let (plat_id_a, fee_bps_a) = if let Some(i) = info_a { (i.platform_market_id.clone(), i.fee_rate_bps) } else { continue; };
                        let (plat_id_b, fee_bps_b) = if let Some(i) = info_b { (i.platform_market_id.clone(), i.fee_rate_bps) } else { continue; };

                        let liquidity = spread.leg_a_available.min(spread.leg_b_available);
                        let log_liq = if liquidity > rust_decimal_macros::dec!(1.0) {
                            decimal_ln(liquidity).max(rust_decimal_macros::dec!(0.1))
                        } else {
                            rust_decimal_macros::dec!(0.1)
                        };
                        let confidence_dec = Decimal::try_from(pair.confidence).unwrap_or(Decimal::ONE);
                        let score = spread.net_spread * log_liq * confidence_dec;

                        opportunities.push(ArbitrageOpportunity {
                            opp_id: Uuid::new_v4(),
                            market_id: spread.market_id,
                            market_question: String::new(), // M-9 FIX: Defer allocation until after truncation
                            leg_a: LegDetail {
                                platform: spread.leg_a_platform,
                                side: spread.leg_a_side,
                                price: spread.leg_a_price,
                                available_size: spread.leg_a_available,
                                fee_estimate: spread.leg_a_fee,
                                fee_rate_bps: fee_bps_a as u32, 
                                platform_market_id: plat_id_a, 
                            },
                            leg_b: LegDetail {
                                platform: spread.leg_b_platform,
                                side: spread.leg_b_side,
                                price: spread.leg_b_price,
                                available_size: spread.leg_b_available,
                                fee_estimate: spread.leg_b_fee,
                                fee_rate_bps: fee_bps_b as u32,
                                platform_market_id: plat_id_b, 
                            },
                            raw_spread: spread.raw_spread,
                            net_spread: spread.net_spread,
                            kelly_fraction: Decimal::ZERO, 
                            recommended_size: spread.leg_a_available.min(spread.leg_b_available), 
                            score,
                            detected_at: now_ns(),
                            // TTL: 500ms gives enough headroom for the async processing chain
                            // (bankroll actor, Kelly sizing, circuit-breaker lock, capital
                            // reservation) while still rejecting truly stale prices.
                            ttl_ms: 500,
                        });
                    }
                    Err(reason) => {
                        debug!(
                            reason = ?reason,
                            market_id = %spread.market_id,
                            raw_spread = %spread.raw_spread,
                            net_spread = %spread.net_spread,
                            leg_a_platform = ?spread.leg_a_platform,
                            leg_b_platform = ?spread.leg_b_platform,
                            "Opportunity rejected by gate"
                        );
                    }
                }
            }
        }

        opportunities.sort_by(|a, b| b.score.cmp(&a.score));
        let slots = self.max_concurrent.saturating_sub(self.active_arbs.load(std::sync::atomic::Ordering::Relaxed));
        opportunities.truncate(slots);

        if !opportunities.is_empty() {
            for opp in &opportunities {
                info!(
                    market_id = %opp.market_id,
                    net_spread = %opp.net_spread,
                    raw_spread = %opp.raw_spread,
                    score = %opp.score,
                    leg_a = ?opp.leg_a.platform,
                    leg_b = ?opp.leg_b.platform,
                    leg_a_price = %opp.leg_a.price,
                    leg_b_price = %opp.leg_b.price,
                    available_size = %opp.recommended_size,
                    "ARB OPPORTUNITY DETECTED — passing to execution pipeline"
                );
            }
        }

        // M-9 FIX: Allocate strings only for the opportunities that actually made the cut
        for opp in &mut opportunities {
            if let Some(market) = registry.get_market(&opp.market_id) {
                opp.market_question = market.question.clone();
            }
        }

        opportunities
    }

        fn run_gates_with_vol(
            &self,
            spread: &SpreadResult,
            book_a: &crate::engine::order_book::PlatformBook,
            book_b: &crate::engine::order_book::PlatformBook,
            confidence: f64,
            vol_multiplier: Decimal,
        ) -> Result<(), RejectionReason> {
            let effective_min_spread = self.min_spread * vol_multiplier;
            if spread.net_spread < effective_min_spread {
                self.stats.gate1_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Err(RejectionReason::BelowSpreadThreshold(spread.net_spread));
        }

        let min_available = spread.leg_a_available.min(spread.leg_b_available);
        if min_available < self.min_order_size {
            self.stats.gate2_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(RejectionReason::InsufficientLiquidity {
                available: min_available,
                required: self.min_order_size,
            });
        }

        let now = now_ns();

        // Stale data check: Prediction markets are illiquid and can sit unchanged for minutes.
        // We only reject if the specific book is older than 15 minutes to catch severe desyncs.
        // Global connection health is strictly handled by platform_liveness and CB9.
        let max_book_age_ns = 15 * 60 * 1_000_000_000u64; // 15 minutes

        if self.platform_liveness.get(&Platform::Polymarket).map(|v| *v).unwrap_or(true) {
            let age_a = now.saturating_sub(book_a.last_update_ns);
            if age_a > max_book_age_ns {
                self.stats.gate3_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Err(RejectionReason::StaleData {
                    age_ms: age_a / 1_000_000,
                    max_ms: max_book_age_ns / 1_000_000,
                });
            }
        }

        if self.platform_liveness.get(&Platform::Kalshi).map(|v| *v).unwrap_or(true) {
            let age_b = now.saturating_sub(book_b.last_update_ns);
            if age_b > max_book_age_ns {
                self.stats.gate3_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Err(RejectionReason::StaleData {
                    age_ms: age_b / 1_000_000,
                    max_ms: max_book_age_ns / 1_000_000,
                });
            }
        }

        if confidence < 0.95 {
            self.stats.gate4_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(RejectionReason::CorrelationExposure {
                current: Decimal::ZERO,
                max: Decimal::ZERO,
            });
        }

        if self.active_arbs.load(Ordering::Relaxed) >= self.max_concurrent {
            self.stats.gate5_rejected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(RejectionReason::RiskBudgetExceeded {
                reason: format!("Max concurrent arbs reached: {}", self.max_concurrent),
            });
        }

        Ok(())
    }
}
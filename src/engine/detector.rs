use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing::debug;
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

/// Detection statistics
#[derive(Debug, Default)]
pub struct DetectorStats {
    pub opportunities_detected: u64,
    pub gate1_rejected: u64,
    pub gate2_rejected: u64,
    pub gate3_rejected: u64,
    pub gate4_rejected: u64,
    pub gate5_rejected: u64,
    pub opportunities_passed: u64,
}

pub struct ArbitrageDetector {
    min_spread: Decimal,
    min_order_size: Decimal,
    stale_timeout_ms: u64,
    max_concurrent: usize,
    active_arbs: usize,
    pub stats: DetectorStats,
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
            active_arbs: 0,
            stats: DetectorStats::default(),
        }
    }

    pub fn set_active_arbs(&mut self, count: usize) {
        self.active_arbs = count;
    }

    /// Run the detection pipeline on all registered arb pairs
    pub fn detect(
        &mut self,
        registry: &MarketRegistry,
        uob: &UnifiedOrderBook,
        spread_engine: &NetSpreadEngine,
        target_size: Decimal,
    ) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        for pair in registry.get_arb_pairs() {
            let book_a = match uob.get_book(&pair.market_id, &pair.platform_a) {
                Some(b) => b,
                None => continue,
            };
            let book_b = match uob.get_book(&pair.market_id, &pair.platform_b) {
                Some(b) => b,
                None => continue,
            };

            let spreads = spread_engine.compute_spreads(book_a, book_b, target_size);

            for spread in spreads {
                self.stats.opportunities_detected += 1;

                match self.run_gates(&spread, book_a, book_b, pair.confidence) {
                    Ok(()) => {
                        self.stats.opportunities_passed += 1;

                        let market = registry.get_market(&pair.market_id);
                        let market_question = market.map(|m| m.question.clone()).unwrap_or_default();
                        let leg_a_pmi = market.and_then(|m| m.platforms.get(&spread.leg_a_platform));
                        let leg_b_pmi = market.and_then(|m| m.platforms.get(&spread.leg_b_platform));

                        let liquidity = spread.leg_a_available.min(spread.leg_b_available);
                        let log_liq = liquidity.to_f64()
                            .map(|l| if l > 0.0 { l.ln() } else { 1.0 })
                            .unwrap_or(1.0);
                        let score = spread.net_spread
                            * Decimal::from_f64_retain(log_liq).unwrap_or(Decimal::ONE)
                            * Decimal::from_f64_retain(pair.confidence).unwrap_or(Decimal::ONE);

                        opportunities.push(ArbitrageOpportunity {
                            opp_id: Uuid::new_v4(),
                            market_id: spread.market_id,
                            market_question,
                            leg_a: LegDetail {
                                platform: spread.leg_a_platform,
                                platform_market_id: leg_a_pmi.map(|p| p.platform_market_id.clone()).unwrap_or_default(),
                                fee_rate_bps: leg_a_pmi.map(|p| p.fee_rate_bps as u32).unwrap_or(0),
                                side: spread.leg_a_side,
                                price: spread.leg_a_price,
                                available_size: spread.leg_a_available,
                                fee_estimate: spread.leg_a_fee,
                            },
                            leg_b: LegDetail {
                                platform: spread.leg_b_platform,
                                platform_market_id: leg_b_pmi.map(|p| p.platform_market_id.clone()).unwrap_or_default(),
                                fee_rate_bps: leg_b_pmi.map(|p| p.fee_rate_bps as u32).unwrap_or(0),
                                side: spread.leg_b_side,
                                price: spread.leg_b_price,
                                available_size: spread.leg_b_available,
                                fee_estimate: spread.leg_b_fee,
                            },
                            raw_spread: spread.raw_spread,
                            net_spread: spread.net_spread,
                            kelly_fraction: Decimal::ZERO,
                            recommended_size: Decimal::ZERO,
                            score,
                            detected_at: now_ns(),
                            ttl_ms: 5000,
                        });
                    }
                    Err(reason) => {
                        debug!(?reason, market_id = %spread.market_id, "Opportunity rejected");
                    }
                }
            }
        }

        opportunities.sort_by(|a, b| b.score.cmp(&a.score));

        let slots = self.max_concurrent.saturating_sub(self.active_arbs);
        opportunities.truncate(slots);

        opportunities
    }

    fn run_gates(
        &mut self,
        spread: &SpreadResult,
        book_a: &crate::engine::order_book::PlatformBook,
        book_b: &crate::engine::order_book::PlatformBook,
        confidence: f64,
    ) -> Result<(), RejectionReason> {
        // G1: Spread Threshold
        if spread.net_spread < self.min_spread {
            self.stats.gate1_rejected += 1;
            return Err(RejectionReason::BelowSpreadThreshold(spread.net_spread));
        }

        // G2: Liquidity Check
        let min_available = spread.leg_a_available.min(spread.leg_b_available);
        if min_available < self.min_order_size {
            self.stats.gate2_rejected += 1;
            return Err(RejectionReason::InsufficientLiquidity {
                available: min_available,
                required: self.min_order_size,
            });
        }

        // G3: Staleness Filter
        let stale_timeout_ns = self.stale_timeout_ms * 1_000_000;
        let now = now_ns();

        let age_a = now.saturating_sub(book_a.last_update_ns);
        if age_a > stale_timeout_ns {
            self.stats.gate3_rejected += 1;
            return Err(RejectionReason::StaleData {
                age_ms: age_a / 1_000_000,
                max_ms: self.stale_timeout_ms,
            });
        }

        let age_b = now.saturating_sub(book_b.last_update_ns);
        if age_b > stale_timeout_ns {
            self.stats.gate3_rejected += 1;
            return Err(RejectionReason::StaleData {
                age_ms: age_b / 1_000_000,
                max_ms: self.stale_timeout_ms,
            });
        }

        // G4: Correlation Check
        if confidence < 0.95 {
            self.stats.gate4_rejected += 1;
            return Err(RejectionReason::CorrelationExposure {
                current: Decimal::ZERO,
                max: Decimal::ZERO,
            });
        }

        // G5: Risk Budget
        if self.active_arbs >= self.max_concurrent {
            self.stats.gate5_rejected += 1;
            return Err(RejectionReason::RiskBudgetExceeded {
                reason: format!("Max concurrent arbs reached: {}", self.max_concurrent),
            });
        }

        Ok(())
    }
}

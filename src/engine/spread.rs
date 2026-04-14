use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::engine::order_book::PlatformBook;
use crate::feeds::normalizer;
use crate::types::*;

/// Result of spread computation for one direction of an arb pair
#[derive(Debug, Clone)]
pub struct SpreadResult {
    pub market_id: Uuid,
    pub leg_a_platform: Platform,
    pub leg_a_side: Side,
    pub leg_a_price: Decimal,
    pub leg_a_available: Decimal,
    pub leg_a_fee: Decimal,
    pub leg_b_platform: Platform,
    pub leg_b_side: Side,
    pub leg_b_price: Decimal,
    pub leg_b_available: Decimal,
    pub leg_b_fee: Decimal,
    pub raw_spread: Decimal,
    pub net_spread: Decimal,
    pub slippage_a: Decimal,
    pub slippage_b: Decimal,
    pub gas_cost: Decimal,
}

#[derive(Clone)]
pub struct NetSpreadEngine {
    min_threshold: Decimal,
    gas_price_gwei: Decimal,
    matic_price_usd: Decimal,
}

impl NetSpreadEngine {
    pub fn new(min_threshold: Decimal) -> Self {
        Self {
            min_threshold,
            gas_price_gwei: Decimal::from(50),
            matic_price_usd: dec!(0.50),
        }
    }

    pub fn update_threshold(&mut self, min_threshold: Decimal) {
        self.min_threshold = min_threshold;
    }

    pub fn update_gas_price(&mut self, gwei: Decimal) {
        self.gas_price_gwei = gwei;
    }

    pub fn update_matic_price(&mut self, price: Decimal) {
        self.matic_price_usd = price;
    }

    /// Compute spread for both directions of an arb pair
    #[must_use]
    pub fn compute_spreads(
        &self,
        book_a: &PlatformBook,
        book_b: &PlatformBook,
        target_size: Decimal,
    ) -> Vec<SpreadResult> {
        let mut results = Vec::new();

        // Direction 1: Buy YES on A, Buy NO on B
        if let (Some((ask_a, ask_a_size)), Some((bid_b, bid_b_size))) = (book_a.best_ask(), book_b.best_bid()) {
            let ask_b_no = Decimal::ONE - bid_b;
            let raw_spread = Decimal::ONE - ask_a - ask_b_no;

            if raw_spread > Decimal::ZERO {
                let depth_a = book_a.ask_depth();
                let depth_b = book_b.bid_depth();
                let total_a: Decimal = depth_a.iter().map(|l| l.size).sum();
                let total_b: Decimal = depth_b.iter().map(|l| l.size).sum();
                
                let actual_target = total_a.min(total_b).min(target_size);

                let fee_a = self.compute_fee(book_a.platform, ask_a, actual_target, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b_no, actual_target, book_b.fee_rate_bps);
                
                match (
                    normalizer::estimate_slippage(actual_target, &depth_a),
                    normalizer::estimate_slippage(actual_target, &depth_b),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                        let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        
                        // FIX: Normalize against actual_target to prevent artificial spread inflation
                        let per_contract_fee_a = fee_a / actual_target;
                        let per_contract_fee_b = fee_b / actual_target;
                        let per_contract_gas = gas / actual_target;

                        let net_spread = raw_spread - per_contract_fee_a - per_contract_fee_b - slippage_a - slippage_b - per_contract_gas;

                        if net_spread > self.min_threshold {
                            results.push(SpreadResult {
                                market_id: book_a.market_id,
                                leg_a_platform: book_a.platform,
                                leg_a_side: Side::Yes,
                                leg_a_price: ask_a,
                                leg_a_available: ask_a_size.min(target_size),
                                leg_a_fee: fee_a,
                                leg_b_platform: book_b.platform,
                                leg_b_side: Side::No,
                                leg_b_price: ask_b_no,
                                leg_b_available: bid_b_size.min(target_size),
                                leg_b_fee: fee_b,
                                raw_spread,
                                net_spread,
                                slippage_a,
                                slippage_b,
                                gas_cost: gas,
                            });
                        }
                    }
                    _ => {
                        tracing::debug!(
                            market_id = %book_a.market_id,
                            direction = "YES-A/NO-B",
                            "Insufficient liquidity to size opportunity — skipping"
                        );
                    }
                }
            }
        }

        // Direction 2: Buy NO on A, Buy YES on B
        if let (Some((bid_a, bid_a_size)), Some((ask_b, ask_b_size))) = (book_a.best_bid(), book_b.best_ask()) {
            let ask_a_no = Decimal::ONE - bid_a;
            let raw_spread = Decimal::ONE - ask_a_no - ask_b;

            tracing::debug!(
                market_id = %book_a.market_id,
                direction = "NO-A/YES-B",
                raw_spread = %raw_spread,
                bid_a = %bid_a,
                ask_b = %ask_b,
                "Evaluated spread direction 2"
            );

            if raw_spread > Decimal::ZERO {
                // CRITICAL FIX: Clamp to the total available depth, not just the top-of-book size.
                // This allows the VWAP estimator to correctly "walk the book" and consume 
                // deeper liquidity if the total spread remains profitable.
                let depth_a = book_a.bid_depth();
                let depth_b = book_b.ask_depth();
                
                let total_a: Decimal = depth_a.iter().map(|l| l.size).sum();
                let total_b: Decimal = depth_b.iter().map(|l| l.size).sum();

                let actual_target = total_a.min(total_b).min(target_size);

                let fee_a = self.compute_fee(book_a.platform, ask_a_no, actual_target, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b, actual_target, book_b.fee_rate_bps);
                
                // Optimized: Reuse the previously allocated depth vectors to save CPU cycles 
                // during the hot-path match evaluation.
                match (
                    normalizer::estimate_slippage(actual_target, &depth_a),
                    normalizer::estimate_slippage(actual_target, &depth_b),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                    let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        
                        // CRITICAL FIX: Mathematical Unit Mismatch.
                        // `fee_a` and `fee_b` were computed using `actual_target`, NOT `target_size`. 
                        // Dividing by the larger `target_size` artificially underestimated the fee drag,
                        // inflating `net_spread` and causing the engine to execute structurally unprofitable arbs.
                        let per_contract_fee_a = fee_a / actual_target;
                        let per_contract_fee_b = fee_b / actual_target;
                        let per_contract_gas = gas / actual_target;

                        let net_spread = raw_spread - per_contract_fee_a - per_contract_fee_b - slippage_a - slippage_b - per_contract_gas;

                        if net_spread > self.min_threshold {
                            results.push(SpreadResult {
                                market_id: book_a.market_id,
                                leg_a_platform: book_a.platform,
                                leg_a_side: Side::No,
                                leg_a_price: ask_a_no,
                                leg_a_available: bid_a_size.min(target_size),
                                leg_a_fee: fee_a,
                                leg_b_platform: book_b.platform,
                                leg_b_side: Side::Yes,
                                leg_b_price: ask_b,
                                leg_b_available: ask_b_size.min(target_size),
                                leg_b_fee: fee_b,
                                raw_spread,
                                net_spread,
                                slippage_a,
                                slippage_b,
                                gas_cost: gas,
                            });
                        }
                    }
                    _ => {
                        tracing::warn!(
                            market_id = %book_a.market_id,
                            direction = "NO-A/YES-B",
                            "Insufficient liquidity to size opportunity — skipping"
                        );
                    }
                }
            }
        }

        results
    }

    fn compute_fee(&self, platform: Platform, price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                // Default to Taker until order type propagation is implemented
                normalizer::polymarket_fee(price, quantity, normalizer::OrderType::Taker)
            }
            Platform::Kalshi => {
                // Delegate to the centralized normalizer to ensure consistency
                // between spread estimation and execution fill accounting.
                normalizer::kalshi_fee(price, quantity, normalizer::OrderType::Taker)
            }
            Platform::Cdna => {
                let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
                rate * quantity * price.max(Decimal::ONE - price)
            }
            Platform::ForecastEx => Decimal::ZERO,
        }
    }

    fn gas_cost_if_onchain(&self, platform_a: Platform, platform_b: Platform) -> Decimal {
        let mut tx_count = Decimal::ZERO;
        if matches!(platform_a, Platform::Polymarket | Platform::PolymarketUs) {
            tx_count += Decimal::ONE;
        }
        if matches!(platform_b, Platform::Polymarket | Platform::PolymarketUs) {
            tx_count += Decimal::ONE;
        }

        if tx_count == Decimal::ZERO {
            return Decimal::ZERO;
        }

        // CRITICAL FIX (3-B): Update empirical gas limit. 
        // 200k was an overestimate; CTF exchange averages 120k-150k.
        let gas_units = Decimal::from(150_000) * tx_count;
        let gwei_to_matic = dec!(0.000000001); // 1 gwei = 10^-9 MATIC
        self.gas_price_gwei * gas_units * gwei_to_matic * self.matic_price_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal::prelude::FromPrimitive;

    #[test]
    fn test_gas_cost_offchain() {
        let engine = NetSpreadEngine::new(dec!(0.01));
        // Kalshi <-> CDNA trade uses zero gas
        assert_eq!(engine.gas_cost_if_onchain(Platform::Kalshi, Platform::Cdna), Decimal::ZERO);
    }

    proptest! {
        #[test]
        fn net_spread_never_exceeds_raw(
            price_a in 0.01f64..0.99,
            price_b_bid in 0.01f64..0.99,
            size in 10.0f64..1000.0,
        ) {
            let engine = NetSpreadEngine::new(dec!(0.001));
            let market_id = Uuid::new_v4();
            
            let mut book_a = PlatformBook::new(Platform::Kalshi, market_id);
            book_a.asks.insert(Decimal::from_f64(price_a).unwrap(), Decimal::from_f64(size).unwrap());
            
            let mut book_b = PlatformBook::new(Platform::Cdna, market_id);
            book_b.bids.insert(Decimal::from_f64(price_b_bid).unwrap(), Decimal::from_f64(size).unwrap());
            
            let spreads = engine.compute_spreads(&book_a, &book_b, dec!(100));
            for s in spreads {
                prop_assert!(s.net_spread <= s.raw_spread);
            }
        }

        #[test]
        fn zero_liquidity_produces_no_opportunities(
            price_a in 0.01f64..0.99,
            price_b_bid in 0.01f64..0.99,
        ) {
            let engine = NetSpreadEngine::new(dec!(0.001));
            let market_id = Uuid::new_v4();
            
            let mut book_a = PlatformBook::new(Platform::Kalshi, market_id);
            book_a.asks.insert(Decimal::from_f64(price_a).unwrap(), Decimal::ZERO);
            
            let mut book_b = PlatformBook::new(Platform::Cdna, market_id);
            book_b.bids.insert(Decimal::from_f64(price_b_bid).unwrap(), Decimal::from_f64(10.0).unwrap());
            
            let spreads = engine.compute_spreads(&book_a, &book_b, dec!(100));
            prop_assert!(spreads.is_empty());
        }
    }
}

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

pub struct NetSpreadEngine {
    min_threshold: Decimal,
    gas_price_gwei: Decimal,
    /// MATIC/USD spot price (Polymarket runs on Polygon; gas is paid in MATIC, not ETH).
    /// Default is $0.50 MATIC — do NOT set this to an ETH price (~$2,000+) or gas costs
    /// will be overestimated 4,000x, suppressing all Polymarket arb opportunities.
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

    pub fn update_gas_price(&mut self, gwei: Decimal) {
        self.gas_price_gwei = gwei;
    }

    pub fn update_matic_price(&mut self, price: Decimal) {
        self.matic_price_usd = price;
    }

    /// Compute spread for both directions of an arb pair
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
                let fee_a = self.compute_fee(book_a.platform, ask_a, target_size, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b_no, target_size, book_b.fee_rate_bps);
                match (
                    normalizer::estimate_slippage(target_size, &book_a.ask_depth()),
                    normalizer::estimate_slippage(target_size, &book_b.bid_depth()),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                        let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        let net_spread = raw_spread - fee_a - fee_b - slippage_a - slippage_b - gas;

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
                        tracing::warn!(
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

            if raw_spread > Decimal::ZERO {
                let fee_a = self.compute_fee(book_a.platform, ask_a_no, target_size, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b, target_size, book_b.fee_rate_bps);
                match (
                    normalizer::estimate_slippage(target_size, &book_a.bid_depth()),
                    normalizer::estimate_slippage(target_size, &book_b.ask_depth()),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                        let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        let net_spread = raw_spread - fee_a - fee_b - slippage_a - slippage_b - gas;

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
                normalizer::polymarket_fee(price, quantity, fee_rate_bps)
            }
            Platform::Kalshi => {
                normalizer::kalshi_taker_fee(price) * quantity
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

        // CRITICAL FIX: If both legs are on-chain, we pay gas twice (400,000 units).
        let gas_units = Decimal::from(200_000) * tx_count;
        let gwei_to_matic = dec!(0.000000001); // 1 gwei = 10^-9 MATIC
        self.gas_price_gwei * gas_units * gwei_to_matic * self.matic_price_usd
    }
}

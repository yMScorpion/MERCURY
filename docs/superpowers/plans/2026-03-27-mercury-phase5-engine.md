# Phase 5: Engine Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Unified Order Book, Net Spread Engine, Market Registry, and Arbitrage Detector (5-gate pipeline).

**Architecture:** The engine runs as a single tokio task receiving NormalizedTick events via broadcast channel. It maintains per-platform order books, computes net spreads with dynamic fee deduction, matches markets cross-platform, and runs the 5-gate detection pipeline to emit validated opportunities.

**Tech Stack:** rust_decimal, tokio broadcast/mpsc, uuid, chrono

**Depends on:** Phase 1 (types), Phase 4 (feeds/normalizer for fee helpers)

---

### Task 1: Unified Order Book

**Files:**
- Create: `src/engine/mod.rs`
- Create: `src/engine/order_book.rs`

- [ ] **Step 1: Write the Unified Order Book**

`src/engine/mod.rs`:
```rust
pub mod order_book;
pub mod spread;
pub mod detector;
pub mod market_registry;
```

`src/engine/order_book.rs`:
```rust
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::types::*;

/// Per-platform order book for a single market
#[derive(Debug, Clone)]
pub struct PlatformBook {
    pub platform: Platform,
    pub market_id: Uuid,
    pub bids: BTreeMap<Decimal, Decimal>,
    pub asks: BTreeMap<Decimal, Decimal>,
    pub last_update_ns: u64,
    pub fee_rate_bps: u16,
    pub sequence: u64,
}

impl PlatformBook {
    pub fn new(platform: Platform, market_id: Uuid) -> Self {
        Self {
            platform,
            market_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update_ns: 0,
            fee_rate_bps: 0,
            sequence: 0,
        }
    }

    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(p, s)| (*p, *s))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(p, s)| (*p, *s))
    }

    pub fn mid_price(&self) -> Decimal {
        match (self.best_bid(), self.best_ask()) {
            (Some((b, _)), Some((a, _))) => (b + a) / Decimal::from(2),
            (Some((b, _)), None) => b,
            (None, Some((a, _))) => a,
            _ => Decimal::ZERO,
        }
    }

    pub fn ask_depth(&self) -> Vec<PriceLevel> {
        self.asks.iter().take(10).map(|(p, s)| PriceLevel { price: *p, size: *s }).collect()
    }

    pub fn bid_depth(&self) -> Vec<PriceLevel> {
        self.bids.iter().rev().take(10).map(|(p, s)| PriceLevel { price: *p, size: *s }).collect()
    }

    pub fn total_ask_depth(&self) -> Decimal {
        self.asks.values().sum()
    }

    pub fn total_bid_depth(&self) -> Decimal {
        self.bids.values().sum()
    }

    pub fn is_stale(&self, timeout_ns: u64) -> bool {
        let now = crate::types::now_ns();
        now.saturating_sub(self.last_update_ns) > timeout_ns
    }

    /// Update from a NormalizedTick
    pub fn update_from_tick(&mut self, tick: &NormalizedTick) {
        self.bids.clear();
        self.asks.clear();

        // Reconstruct from depth
        for level in &tick.book_depth {
            // Levels below mid are bids, above are asks
            if level.price <= tick.mid_price {
                self.bids.insert(level.price, level.size);
            } else {
                self.asks.insert(level.price, level.size);
            }
        }

        // Ensure best bid/ask are set even if depth is sparse
        if tick.bid_price > Decimal::ZERO && tick.bid_size > Decimal::ZERO {
            self.bids.insert(tick.bid_price, tick.bid_size);
        }
        if tick.ask_price > Decimal::ZERO && tick.ask_size > Decimal::ZERO {
            self.asks.insert(tick.ask_price, tick.ask_size);
        }

        self.last_update_ns = tick.timestamp_ns;
        self.fee_rate_bps = tick.fee_rate_bps;
        self.sequence = tick.sequence;
    }
}

/// Unified Order Book: aggregates all platform books for all markets
pub struct UnifiedOrderBook {
    /// (market_id, platform) -> PlatformBook
    books: HashMap<(Uuid, Platform), PlatformBook>,
}

impl UnifiedOrderBook {
    pub fn new() -> Self {
        Self { books: HashMap::new() }
    }

    pub fn update(&mut self, tick: &NormalizedTick) {
        let key = (tick.market_id, tick.platform);
        let book = self.books
            .entry(key)
            .or_insert_with(|| PlatformBook::new(tick.platform, tick.market_id));
        book.update_from_tick(tick);
    }

    pub fn get_book(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformBook> {
        self.books.get(&(*market_id, *platform))
    }

    /// Get all platform books for a given market
    pub fn get_market_books(&self, market_id: &Uuid) -> Vec<&PlatformBook> {
        self.books.iter()
            .filter(|((mid, _), _)| mid == market_id)
            .map(|(_, book)| book)
            .collect()
    }

    /// Get all (market_id, platform) pairs
    pub fn all_keys(&self) -> Vec<(Uuid, Platform)> {
        self.books.keys().cloned().collect()
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/engine/
git commit -m "feat: add Unified Order Book with per-platform book tracking"
```

---

### Task 2: Market Registry

**Files:**
- Create: `src/engine/market_registry.rs`

- [ ] **Step 1: Write the Market Registry for cross-platform matching**

`src/engine/market_registry.rs`:
```rust
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::types::*;

/// Cross-platform market matching registry
pub struct MarketRegistry {
    /// unified_market_id -> Market definition
    markets: HashMap<Uuid, Market>,
    /// All pairs of (market_id, platform_a, platform_b) eligible for arbitrage
    arb_pairs: Vec<ArbPair>,
}

#[derive(Debug, Clone)]
pub struct ArbPair {
    pub market_id: Uuid,
    pub platform_a: Platform,
    pub platform_b: Platform,
    pub confidence: f64,
}

impl MarketRegistry {
    pub fn new() -> Self {
        Self {
            markets: HashMap::new(),
            arb_pairs: Vec::new(),
        }
    }

    /// Register a market (with its cross-platform mappings)
    pub fn register_market(&mut self, market: Market) {
        let market_id = market.unified_id;
        let platforms: Vec<Platform> = market.platforms.keys().cloned().collect();

        self.markets.insert(market_id, market);

        // Generate all platform pair combinations for this market
        for i in 0..platforms.len() {
            for j in (i + 1)..platforms.len() {
                let confidence = self.markets.get(&market_id)
                    .map(|m| m.confidence)
                    .unwrap_or(0.0);

                if confidence >= 0.95 {
                    self.arb_pairs.push(ArbPair {
                        market_id,
                        platform_a: platforms[i],
                        platform_b: platforms[j],
                        confidence,
                    });
                }
            }
        }
    }

    /// Get all arbitrage pairs
    pub fn get_arb_pairs(&self) -> &[ArbPair] {
        &self.arb_pairs
    }

    /// Get market by ID
    pub fn get_market(&self, id: &Uuid) -> Option<&Market> {
        self.markets.get(id)
    }

    /// Get all registered market IDs
    pub fn market_ids(&self) -> Vec<Uuid> {
        self.markets.keys().cloned().collect()
    }

    /// Get platform-specific market info
    pub fn get_platform_info(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformMarketInfo> {
        self.markets.get(market_id)?.platforms.get(platform)
    }

    pub fn market_count(&self) -> usize {
        self.markets.len()
    }

    pub fn pair_count(&self) -> usize {
        self.arb_pairs.len()
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/engine/market_registry.rs
git commit -m "feat: add Market Registry for cross-platform market matching"
```

---

### Task 3: Net Spread Engine

**Files:**
- Create: `src/engine/spread.rs`

- [ ] **Step 1: Write the Net Spread Engine**

`src/engine/spread.rs`:
```rust
use rust_decimal::Decimal;
use std::str::FromStr;
use tracing::debug;

use crate::engine::order_book::PlatformBook;
use crate::feeds::normalizer;
use crate::types::*;

/// Result of spread computation for one direction of an arb pair
#[derive(Debug, Clone)]
pub struct SpreadResult {
    pub market_id: uuid::Uuid,
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
    /// Minimum net spread to consider viable (default 0.15%)
    min_threshold: Decimal,
    /// Current Polygon gas price in gwei
    gas_price_gwei: Decimal,
    /// ETH price in USD (for gas cost conversion)
    eth_price_usd: Decimal,
}

impl NetSpreadEngine {
    pub fn new(min_threshold: Decimal) -> Self {
        Self {
            min_threshold,
            gas_price_gwei: Decimal::from(50), // Default
            eth_price_usd: Decimal::from(2000), // Default
        }
    }

    pub fn update_gas_price(&mut self, gwei: Decimal) {
        self.gas_price_gwei = gwei;
    }

    pub fn update_eth_price(&mut self, price: Decimal) {
        self.eth_price_usd = price;
    }

    pub fn set_min_threshold(&mut self, threshold: Decimal) {
        self.min_threshold = threshold;
    }

    /// Compute spread for both directions of an arb pair
    /// Returns up to 2 SpreadResults (buy YES on A + buy NO on B, and vice versa)
    pub fn compute_spreads(
        &self,
        book_a: &PlatformBook,
        book_b: &PlatformBook,
        target_size: Decimal,
    ) -> Vec<SpreadResult> {
        let mut results = Vec::new();

        // Direction 1: Buy YES on A, Buy NO on B
        // Cost = ask_A_yes + ask_B_no, profit if < 1.0
        if let (Some((ask_a, ask_a_size)), Some((bid_b, bid_b_size))) = (book_a.best_ask(), book_b.best_bid()) {
            // For binary markets: buying NO on B at price P is equivalent to
            // the complement: ask_B_no = 1 - bid_B_yes
            let ask_b_no = Decimal::ONE - bid_b;

            let raw_spread = Decimal::ONE - ask_a - ask_b_no;

            if raw_spread > Decimal::ZERO {
                let fee_a = self.compute_fee(book_a.platform, ask_a, target_size, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b_no, target_size, book_b.fee_rate_bps);

                let slippage_a = normalizer::estimate_slippage(target_size, &book_a.ask_depth(), true);
                let slippage_b = normalizer::estimate_slippage(target_size, &book_b.bid_depth(), false);

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
        }

        // Direction 2: Buy NO on A, Buy YES on B
        if let (Some((bid_a, bid_a_size)), Some((ask_b, ask_b_size))) = (book_a.best_bid(), book_b.best_ask()) {
            let ask_a_no = Decimal::ONE - bid_a;

            let raw_spread = Decimal::ONE - ask_a_no - ask_b;

            if raw_spread > Decimal::ZERO {
                let fee_a = self.compute_fee(book_a.platform, ask_a_no, target_size, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b, target_size, book_b.fee_rate_bps);

                let slippage_a = normalizer::estimate_slippage(target_size, &book_a.bid_depth(), false);
                let slippage_b = normalizer::estimate_slippage(target_size, &book_b.ask_depth(), true);

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
                // Probability-weighted, similar to Polymarket
                let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
                rate * quantity * price.max(Decimal::ONE - price)
            }
            Platform::ForecastEx => {
                // Spread-embedded, no explicit fee
                Decimal::ZERO
            }
        }
    }

    fn gas_cost_if_onchain(&self, platform_a: Platform, platform_b: Platform) -> Decimal {
        let needs_gas = matches!(platform_a, Platform::Polymarket | Platform::PolymarketUs)
            || matches!(platform_b, Platform::Polymarket | Platform::PolymarketUs);

        if !needs_gas {
            return Decimal::ZERO;
        }

        // Gas cost = gas_price_gwei * gas_units * ETH_price / 1e9
        let gas_units = Decimal::from(200_000); // ~200k gas per trade
        let gwei_to_eth = Decimal::from_str("0.000000001").unwrap();
        self.gas_price_gwei * gas_units * gwei_to_eth * self.eth_price_usd
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/engine/spread.rs
git commit -m "feat: add Net Spread Engine with dynamic fee deduction and slippage"
```

---

### Task 4: Arbitrage Detector (5-Gate Pipeline)

**Files:**
- Create: `src/engine/detector.rs`

- [ ] **Step 1: Write the Arbitrage Detector**

`src/engine/detector.rs`:
```rust
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, info, warn};
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
    /// Minimum net spread threshold
    min_spread: Decimal,
    /// Minimum order size in USD
    min_order_size: Decimal,
    /// Stale data timeout in milliseconds
    stale_timeout_ms: u64,
    /// Max concurrent arbitrage executions
    max_concurrent: usize,
    /// Currently active arbitrage count
    active_arbs: usize,
    /// Stats
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

                        let market_question = registry.get_market(&pair.market_id)
                            .map(|m| m.question.clone())
                            .unwrap_or_default();

                        // Compute score
                        let liquidity = spread.leg_a_available.min(spread.leg_b_available);
                        let log_liq = if liquidity > Decimal::ZERO {
                            Decimal::from_str(&format!("{:.4}", (liquidity.to_f64().unwrap_or(1.0)).ln())).unwrap_or(Decimal::ONE)
                        } else {
                            Decimal::ONE
                        };
                        let score = spread.net_spread * log_liq * Decimal::from_str(&format!("{:.4}", pair.confidence)).unwrap_or(Decimal::ONE);

                        opportunities.push(ArbitrageOpportunity {
                            opp_id: Uuid::new_v4(),
                            market_id: spread.market_id,
                            market_question,
                            leg_a: LegDetail {
                                platform: spread.leg_a_platform,
                                side: spread.leg_a_side,
                                price: spread.leg_a_price,
                                available_size: spread.leg_a_available,
                                fee_estimate: spread.leg_a_fee,
                            },
                            leg_b: LegDetail {
                                platform: spread.leg_b_platform,
                                side: spread.leg_b_side,
                                price: spread.leg_b_price,
                                available_size: spread.leg_b_available,
                                fee_estimate: spread.leg_b_fee,
                            },
                            raw_spread: spread.raw_spread,
                            net_spread: spread.net_spread,
                            kelly_fraction: Decimal::ZERO, // Filled by risk manager
                            recommended_size: Decimal::ZERO, // Filled by risk manager
                            score,
                            detected_at: now_ns(),
                            ttl_ms: 5000, // 5 second TTL
                        });
                    }
                    Err(reason) => {
                        debug!(?reason, market_id = %spread.market_id, "Opportunity rejected");
                    }
                }
            }
        }

        // Sort by score descending
        opportunities.sort_by(|a, b| b.score.cmp(&a.score));

        // Limit to max_concurrent
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

        // G4: Correlation Check (simplified - checked by risk manager in detail)
        // Here we just verify confidence is high enough
        if confidence < 0.95 {
            self.stats.gate4_rejected += 1;
            return Err(RejectionReason::CorrelationExposure {
                current: Decimal::ZERO,
                max: Decimal::ZERO,
            });
        }

        // G5: Risk Budget (basic check - detailed check done by risk manager)
        if self.active_arbs >= self.max_concurrent {
            self.stats.gate5_rejected += 1;
            return Err(RejectionReason::RiskBudgetExceeded {
                reason: format!("Max concurrent arbs reached: {}", self.max_concurrent),
            });
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Update main.rs**

Add to `src/main.rs`:
```rust
mod engine;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/engine/ src/main.rs
git commit -m "feat: add Arbitrage Detector with 5-gate pipeline and scoring"
```

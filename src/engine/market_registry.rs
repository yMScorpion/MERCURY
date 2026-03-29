use std::collections::HashMap;
use uuid::Uuid;

use crate::types::*;

/// Cross-platform market matching registry
pub struct MarketRegistry {
    markets: HashMap<Uuid, Market>,
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

    /// Register a market. Idempotent: re-registering the same market_id updates
    /// the market definition but does not create duplicate arb pairs.
    pub fn register_market(&mut self, market: Market) {
        let market_id = market.unified_id;
        let platforms: Vec<Platform> = market.platforms.keys().cloned().collect();
        let confidence = market.confidence;

        // Remove stale arb pairs for this market before re-adding.
        self.arb_pairs.retain(|p| p.market_id != market_id);

        self.markets.insert(market_id, market);

        if confidence >= 0.95 {
            for i in 0..platforms.len() {
                for j in (i + 1)..platforms.len() {
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

    pub fn get_arb_pairs(&self) -> &[ArbPair] {
        &self.arb_pairs
    }

    pub fn get_market(&self, id: &Uuid) -> Option<&Market> {
        self.markets.get(id)
    }

    pub fn get_platform_info(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformMarketInfo> {
        self.markets.get(market_id)?.platforms.get(platform)
    }
}

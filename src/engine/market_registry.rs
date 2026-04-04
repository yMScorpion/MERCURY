use std::collections::HashMap;
use uuid::Uuid;

use crate::types::*;

/// Cross-platform market matching registry
#[derive(Clone)]
pub struct MarketRegistry {
    markets: HashMap<Uuid, Market>,
    arb_pairs: HashMap<Uuid, Vec<ArbPair>>,
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
            arb_pairs: HashMap::new(),
        }
    }

    /// Remove markets that have expired or been resolved. Call periodically
    /// to prevent unbounded memory growth during long-running sessions.
    pub fn evict_stale_markets(&mut self) -> Vec<Uuid> {
        let now = chrono::Utc::now();
        let stale_ids: Vec<Uuid> = self.markets.iter()
            .filter(|(_, m)| {
                matches!(m.status, crate::types::MarketStatus::Resolved | crate::types::MarketStatus::Expired)
                || m.expiration < now - chrono::Duration::hours(1)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &stale_ids {
            self.markets.remove(id);
            self.arb_pairs.remove(id);
        }
        stale_ids // Return IDs so the tick router can drop the actor channels
    }

    /// Register a market. Idempotent: re-registering the same market_id updates
    /// the market definition but does not create duplicate arb pairs.
    pub fn register_market(&mut self, market: Market) {
        let market_id = market.unified_id;
        let platforms: Vec<Platform> = market.platforms.keys().cloned().collect();
        let confidence = market.confidence;

        self.markets.insert(market_id, market);

        let mut pairs = Vec::new();
        if confidence >= 0.95 {
            for i in 0..platforms.len() {
                for j in (i + 1)..platforms.len() {
                    pairs.push(ArbPair {
                        market_id,
                        platform_a: platforms[i],
                        platform_b: platforms[j],
                        confidence,
                    });
                }
            }
        }
        
        if !pairs.is_empty() {
            self.arb_pairs.insert(market_id, pairs);
        } else {
            self.arb_pairs.remove(&market_id);
        }
    }

    pub fn get_arb_pairs_for_market(&self, market_id: &Uuid) -> Option<&Vec<ArbPair>> {
        self.arb_pairs.get(market_id)
    }

    pub fn get_market(&self, id: &Uuid) -> Option<&Market> {
        self.markets.get(id)
    }

    pub fn get_platform_info(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformMarketInfo> {
        self.markets.get(market_id)?.platforms.get(platform)
    }
}

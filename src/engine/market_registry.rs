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

impl Default for MarketRegistry {
    fn default() -> Self { Self::new() }
}

impl MarketRegistry {
    pub fn new() -> Self {
        Self {
            markets: HashMap::new(),
            arb_pairs: HashMap::new(),
        }
    }

    /// Remove markets that have resolved, expired, or whose expiry passed.
    ///
    /// Markets are evicted when:
    ///   1. Status is Resolved or Expired
    ///   2. Status is Active but expiration is more than 5 minutes in the past
    ///      (safety net for markets that never received a Resolved status update)
    ///
    /// Returns IDs of evicted markets so the tick router can drop actor channels.
    pub fn evict_stale_markets(&mut self) -> Vec<Uuid> {
        let now = chrono::Utc::now();
        let stale_ids: Vec<Uuid> = self.markets.iter()
            .filter(|(_, m)| {
                matches!(m.status, MarketStatus::Resolved | MarketStatus::Expired)
                // Safety net: evict Active markets that expired > 5 minutes ago
                // (covers cases where the Resolved message was never received)
                || (m.status == MarketStatus::Active && m.expiration < now - chrono::Duration::minutes(5))
            })
            .map(|(id, _)| *id)
            .collect();

        for id in &stale_ids {
            self.markets.remove(id);
            self.arb_pairs.remove(id);
        }

        if !stale_ids.is_empty() {
            tracing::info!(count = stale_ids.len(), "Evicted stale/resolved markets from registry");
        }

        stale_ids
    }

    /// Register (or update) a market.
    ///
    /// Idempotent: re-registering the same market_id updates the market
    /// definition including platform info and status. This is critical for
    /// 15-minute markets where the token IDs change each round.
    pub fn register_market(&mut self, market: Market) {
        let market_id = market.unified_id;
        let platforms: Vec<Platform> = market.platforms.keys().cloned().collect();
        let confidence = market.confidence;
        let status = market.status;

        self.markets.insert(market_id, market);

        // Only build arb pairs for active markets with sufficient confidence
        if matches!(status, MarketStatus::Active) && confidence >= 0.2 {
            let mut pairs = Vec::new();
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
            if !pairs.is_empty() {
                self.arb_pairs.insert(market_id, pairs);
            } else {
                self.arb_pairs.remove(&market_id);
            }
        } else {
            // Resolved/Suspended/Expired markets have no arb pairs
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

    pub fn market_count(&self) -> usize {
        self.markets.len()
    }

    pub fn arb_pair_count(&self) -> usize {
        self.arb_pairs.values().map(|v| v.len()).sum()
    }

    /// Returns the number of currently active (tradeable) markets
    pub fn active_market_count(&self) -> usize {
        self.markets.values().filter(|m| m.status == MarketStatus::Active).count()
    }
}
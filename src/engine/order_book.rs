use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;

use crate::types::*;

/// Per-platform order book for a single market
#[derive(Debug, Clone)]
pub struct PlatformBook {
    pub platform: Platform,
    pub market_id: Uuid,
    pub bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub asks: std::collections::BTreeMap<Decimal, Decimal>,
    pub last_update_ns: u64,
    pub fee_rate_bps: u16,
    pub sequence: u64,
}

impl PlatformBook {
    pub fn new(platform: Platform, market_id: Uuid) -> Self {
        Self {
            platform,
            market_id,
            bids: std::collections::BTreeMap::new(),
            asks: std::collections::BTreeMap::new(),
            last_update_ns: 0,
            fee_rate_bps: 0,
            sequence: 0,
        }
    }

    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
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
        self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn bid_depth(&self) -> Vec<PriceLevel> {
        self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn is_stale(&self, timeout_ns: u64) -> bool {
        let now = crate::types::now_ns();
        now.saturating_sub(self.last_update_ns) > timeout_ns
    }

    /// Update from a NormalizedTick
    ///
    /// Ticks with a non-zero sequence that is ≤ the stored sequence are dropped
    /// to prevent out-of-order or replayed updates (e.g. after a reconnect)
    /// from overwriting a newer book with stale data.
    /// Sequence-0 ticks are treated as full snapshots and always applied.
    pub fn update_from_tick(&mut self, tick: &NormalizedTick) {
        if tick.sequence > 0 && tick.sequence <= self.sequence {
            return;
        }

        // Apply depth levels as incremental updates. Zero-size = remove.
        for level in tick.book_depth.iter() {
            if level.size == Decimal::ZERO {
                self.bids.remove(&level.price);
                self.asks.remove(&level.price);
                continue;
            }
            if level.price <= tick.bid_price {
                self.bids.insert(level.price, level.size);
            } else if level.price >= tick.ask_price {
                self.asks.insert(level.price, level.size);
            } else {
                // Mid-spread level: classify by proximity
                if tick.ask_price - level.price < level.price - tick.bid_price {
                    self.asks.insert(level.price, level.size);
                } else {
                    self.bids.insert(level.price, level.size);
                }
            }
        }

        // Apply BBO and enforce uncrossed book invariant
        if tick.bid_price > Decimal::ZERO && tick.bid_size > Decimal::ZERO {
            self.bids.insert(tick.bid_price, tick.bid_size);
            // Split off asks <= bid_price to prevent crossed book
            let invalid_asks: Vec<Decimal> = self.asks.range(..=tick.bid_price).map(|(&p, _)| p).collect();
            for p in invalid_asks { self.asks.remove(&p); }
        }
        if tick.ask_price > Decimal::ZERO && tick.ask_size > Decimal::ZERO {
            self.asks.insert(tick.ask_price, tick.ask_size);
            // Split off bids >= ask_price to prevent crossed book
            let invalid_bids: Vec<Decimal> = self.bids.range(tick.ask_price..).map(|(&p, _)| p).collect();
            for p in invalid_bids { self.bids.remove(&p); }
        }

        self.last_update_ns = tick.timestamp_ns;
        self.fee_rate_bps = tick.fee_rate_bps;
        self.sequence = tick.sequence;
    }
}

/// Unified Order Book: aggregates all platform books for all markets
pub struct UnifiedOrderBook {
    books: HashMap<(Uuid, Platform), PlatformBook>,
}

impl UnifiedOrderBook {
    pub fn new() -> Self {
        Self { books: HashMap::new() }
    }

    pub fn clear(&mut self) {
        self.books.clear();
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
}

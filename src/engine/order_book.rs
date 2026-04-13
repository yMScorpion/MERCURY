use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;
use arrayvec::ArrayVec;

use crate::types::{NormalizedTick, Platform, PriceLevel};

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

    pub fn ask_depth(&self) -> ArrayVec<PriceLevel, 20> {
        self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn bid_depth(&self) -> ArrayVec<PriceLevel, 20> {
        self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn is_stale(&self, timeout_ns: u64) -> bool {
        let now = crate::types::now_ns();
        now.saturating_sub(self.last_update_ns) > timeout_ns
    }

    /// Explicitly clear all book state after market resolution.
    ///
    /// Sets `last_update_ns` to 0 so Gate 3 in the detector treats this book as
    /// "uninitialized" rather than "stale". An empty book with a stale timestamp
    /// would silently pass Gate 3 for up to 20 minutes; resetting to 0 causes
    /// the gate to skip the age check entirely (the `last_update_ns > 0` guard).
    pub fn clear_resolved(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.last_update_ns = 0;
        self.sequence = 0;
    }

    /// Update from a NormalizedTick
    ///
    /// Ticks with a non-zero sequence that is ≤ the stored sequence are dropped
    /// to prevent out-of-order or replayed updates from overwriting newer data.
    /// Sequence-0 ticks are treated as full snapshots and always applied.
    pub fn update_from_tick(&mut self, tick: &NormalizedTick) {
        if tick.sequence > 0 && tick.sequence <= self.sequence {
            return;
        }

        // Reject ticks with prices outside valid prediction market range
        if tick.bid_price < Decimal::ZERO || tick.bid_price > Decimal::ONE
            || tick.ask_price < Decimal::ZERO || tick.ask_price > Decimal::ONE
        {
            tracing::warn!(
                platform = ?self.platform,
                market_id = %self.market_id,
                bid = %tick.bid_price,
                ask = %tick.ask_price,
                "Rejecting tick with out-of-range prices"
            );
            return;
        }

        // Since `tick.book_depth` is a snapshot of the top N levels,
        // we completely clear local state to prevent phantom stale liquidity.
        self.bids.clear();
        self.asks.clear();

        // Apply BBO
        if tick.bid_price > Decimal::ZERO && tick.bid_size > Decimal::ZERO {
            self.bids.insert(tick.bid_price, tick.bid_size);
        }
        if tick.ask_price > Decimal::ZERO && tick.ask_size > Decimal::ZERO {
            self.asks.insert(tick.ask_price, tick.ask_size);
        }

        // Apply depth levels
        for level in tick.book_depth.iter() {
            if level.size > Decimal::ZERO {
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
        }

        if let (Some((bb, _)), Some((ba, _))) = (self.best_bid(), self.best_ask()) {
            if bb >= ba {
                tracing::warn!("POST-CONDITION WARNING: crossed book after tick");
            }
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

impl Default for UnifiedOrderBook {
    fn default() -> Self {
        Self::new()
    }
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

    /// Remove all platform books for a resolved/expired market.
    ///
    /// Called by the main loop and MarketActor when a market transitions to
    /// Resolved or Expired status. This prevents the detector's Gate 3 from
    /// operating on stale book data for a market that no longer exists.
    pub fn remove_market(&mut self, market_id: &Uuid) {
        let keys_to_remove: Vec<(Uuid, Platform)> = self.books.keys()
            .filter(|(mid, _)| mid == market_id)
            .cloned()
            .collect();
        let removed = keys_to_remove.len();
        for key in keys_to_remove {
            self.books.remove(&key);
        }
        if removed > 0 {
            tracing::debug!(
                market_id = %market_id,
                removed_books = removed,
                "Removed platform books for resolved market from UnifiedOrderBook"
            );
        }
    }

    /// Clear all book levels for a market (without removing the book entries).
    /// Sets last_update_ns to 0 so Gate 3 skips the staleness check for these books.
    /// Use this when you want the books to be re-bootstrapped on the next tick rather
    /// than removed entirely.
    pub fn clear_market(&mut self, market_id: &Uuid) {
        for ((mid, _), book) in self.books.iter_mut() {
            if mid == market_id {
                book.clear_resolved();
            }
        }
    }
}
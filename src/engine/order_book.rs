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

        // Apply BBO and enforce uncrossed book invariant using O(log N) split_off
        // HIGH-4 FIX: BTreeMap::split_off is O(log N) vs the previous O(N) collect+remove.
        if tick.bid_price > Decimal::ZERO && tick.bid_size > Decimal::ZERO {
            self.bids.insert(tick.bid_price, tick.bid_size);
            // Remove all asks at or below bid_price.
            // split_off(key) returns everything >= key, leaving everything < key in self.
            // We need to remove asks where price <= bid_price.
            // Increment by smallest possible to get "strictly greater than bid_price".
            // BTreeMap split trick: split at bid_price + epsilon isn't clean with Decimal.
            // Instead, we split at bid_price and check if bid_price itself is in asks.
            let kept = self.asks.split_off(&tick.bid_price);
            // `self.asks` now contains asks < bid_price (invalid). `kept` has asks >= bid_price.
            self.asks = kept;
            // Also remove the ask AT bid_price if it exists (asks must be strictly > bid)
            self.asks.remove(&tick.bid_price);
        }
        if tick.ask_price > Decimal::ZERO && tick.ask_size > Decimal::ZERO {
            self.asks.insert(tick.ask_price, tick.ask_size);
            // Remove all bids at or above ask_price.
            // split_off(ask_price) gives us everything >= ask_price (invalid bids).
            let invalid_bids = self.bids.split_off(&tick.ask_price);
            // `invalid_bids` is dropped, `self.bids` retains only bids < ask_price.
            drop(invalid_bids);
        }

        if let (Some((bb, _)), Some((ba, _))) = (self.best_bid(), self.best_ask()) {
            if bb >= ba {
                tracing::error!("POST-CONDITION VIOLATION: crossed book after tick");
                self.bids.clear();
                self.asks.clear();
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
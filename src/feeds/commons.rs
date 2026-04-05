//! Shared order book operations for feed handlers.
//! Each feed maintains a local order book for computing ticks; this trait
//! standardises the interface so boilerplate is not duplicated.

use rust_decimal::Decimal;
use crate::types::PriceLevel;
use arrayvec::ArrayVec;

/// Common operations on a local feed-level order book.
pub trait LocalBookOps {
    fn bids(&self) -> &std::collections::BTreeMap<Decimal, Decimal>;
    fn asks(&self) -> &std::collections::BTreeMap<Decimal, Decimal>;

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids().iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks().iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / Decimal::from(2))
    }

    fn bid_depth(&self) -> ArrayVec<PriceLevel, 20> {
        self.bids().iter().rev().take(10)
            .map(|(&p, &s)| PriceLevel { price: p, size: s })
            .collect()
    }

    fn ask_depth(&self) -> ArrayVec<PriceLevel, 20> {
        self.asks().iter().take(10)
            .map(|(&p, &s)| PriceLevel { price: p, size: s })
            .collect()
    }

    fn depth(&self) -> ArrayVec<PriceLevel, 20> {
        let mut levels = ArrayVec::new();
        for (&p, &s) in self.bids().iter().rev().take(10) {
            levels.push(PriceLevel { price: p, size: s });
        }
        for (&p, &s) in self.asks().iter().take(10) {
            levels.push(PriceLevel { price: p, size: s });
        }
        levels
    }
}
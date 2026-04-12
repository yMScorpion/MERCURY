use rust_decimal::Decimal;
use uuid::Uuid;

use crate::types::*;

/// Compute a deterministic unified market ID from question + resolution + expiry
pub fn compute_unified_market_id(question: &str, resolution_source: &str, expiry: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(question.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(resolution_source.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(expiry.as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Calculate Polymarket fee for given price and fee_rate_bps
pub fn polymarket_fee(price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
    let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
    let max_side = price.max(Decimal::ONE - price);
    rate * quantity * max_side
}

/// Calculate VWAP slippage for a target order size against order book depth.
///
/// `depth` **must** be pre-sorted in fill-walk order by the caller:
///   - For buys against asks: ascending price (`PlatformBook::ask_depth()`)
///   - For sells into bids: descending price (`PlatformBook::bid_depth()`)
///
/// Returns `None` when the available depth is insufficient to fill `target_size`.
/// Returns `Some(slippage)` — the absolute VWAP deviation from best quoted price.
pub fn estimate_slippage(target_size: Decimal, depth: &[PriceLevel]) -> Option<Decimal> {
    // CRITICAL FIX: If target size is zero or less, we must return None.
    if depth.is_empty() || target_size <= Decimal::ZERO {
        return None;
    }
    // Debug assertion: depth must be monotonically ordered (ascending asks or descending bids)
    #[cfg(debug_assertions)]
    if depth.len() > 1 {
        // Allow either ascending (asks) or descending (bids)
        let is_ascending = depth.windows(2).all(|w| w[0].price <= w[1].price);
        let is_descending = depth.windows(2).all(|w| w[0].price >= w[1].price);
        debug_assert!(is_ascending || is_descending, "estimate_slippage: depth must be sorted");
    }

    let levels: Vec<&PriceLevel> = depth.iter().filter(|l| l.size > Decimal::ZERO).collect();
    if levels.is_empty() {
        // Return None to explicitly signal a complete lack of liquidity
        return None;
    }

    let best_price = levels[0].price;
    let mut remaining = target_size;
    let mut total_cost = Decimal::ZERO;

    for level in &levels {
        let fill_qty = remaining.min(level.size);
        total_cost += fill_qty * level.price;
        remaining -= fill_qty;
        if remaining <= Decimal::ZERO {
            break;
        }
    }

    if remaining > Decimal::ZERO {
        return None;
    }

    let vwap = total_cost / target_size;
    Some((vwap - best_price).abs())
}

pub fn kalshi_fee(price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
    if fee_rate_bps < 100 {
        // e.g. 5 bps for 15M crypto markets. Calculate as a standard percentage of the wager.
        let rate = Decimal::from(fee_rate_bps) / rust_decimal_macros::dec!(10000);
        rate * quantity * price
    } else {
        // Standard Kalshi fee: 10% of wager capped at 7c per contract
        let max_fee = rust_decimal_macros::dec!(0.07);
        let implied_fee = price * rust_decimal_macros::dec!(0.10);
        max_fee.min(implied_fee) * quantity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_polymarket_fee() {
        // Price > 0.5 (e.g. 0.6), quantity = 100, fee = 200 bps (0.02)
        // Fee = 0.02 * 100 * max(0.6, 0.4) = 2 * 0.6 = 1.2
        let fee = polymarket_fee(dec!(0.6), dec!(100), 200);
        assert_eq!(fee, dec!(1.20));

        // Price < 0.5 (e.g. 0.2), quantity = 50, fee = 100 bps (0.01)
        // Fee = 0.01 * 50 * max(0.2, 0.8) = 0.5 * 0.8 = 0.4
        let fee2 = polymarket_fee(dec!(0.2), dec!(50), 100);
        assert_eq!(fee2, dec!(0.40));
    }

    #[test]
    fn test_estimate_slippage() {
        let depth = vec![
            PriceLevel { price: dec!(0.50), size: dec!(10) },
            PriceLevel { price: dec!(0.52), size: dec!(20) },
        ];

        // Target size fully within first level
        let slip1 = estimate_slippage(dec!(5), &depth);
        assert_eq!(slip1, Some(dec!(0)));

        // Target size spans both levels:
        // 10 @ 0.50 = 5.0
        // 5 @ 0.52 = 2.6
        // Total cost = 7.6 for 15 contracts -> VWAP = 0.50666...
        // Best price = 0.50
        // Slippage = 0.006666...
        let slip2 = estimate_slippage(dec!(15), &depth).unwrap();
        assert!(slip2 > dec!(0.006));
        assert!(slip2 < dec!(0.007));

        // Insufficient depth
        let slip3 = estimate_slippage(dec!(50), &depth);
        assert_eq!(slip3, None);
    }
}
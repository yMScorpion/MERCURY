use rust_decimal::Decimal;
use rust_decimal_macros::dec;
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

/// Calculate Kalshi taker fee for a given contract price (0.01-0.99)
pub fn kalshi_taker_fee(contract_price: Decimal) -> Decimal {
    dec!(0.07) * contract_price * (Decimal::ONE - contract_price)
}

/// Calculate Kalshi maker fee (25% of taker fee)
pub fn kalshi_maker_fee(contract_price: Decimal) -> Decimal {
    kalshi_taker_fee(contract_price) * dec!(0.25)
}

/// Calculate Polymarket fee for given price and fee_rate_bps
pub fn polymarket_fee(price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
    let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
    let max_side = price.max(Decimal::ONE - price);
    rate * quantity * max_side
}

/// Calculate VWAP slippage for a target order size against order book depth.
///
/// Returns `None` when the available depth is insufficient to fill `target_size`
/// (the caller must treat this as an illiquid / unsizable opportunity).
/// Returns `Some(slippage)` where slippage is the absolute deviation of the
/// fill VWAP from the best quoted price.
pub fn estimate_slippage(target_size: Decimal, depth: &[PriceLevel], _is_buy: bool) -> Option<Decimal> {
    if depth.is_empty() || target_size == Decimal::ZERO {
        return Some(Decimal::ZERO);
    }

    let levels: Vec<&PriceLevel> = depth.iter().filter(|l| l.size > Decimal::ZERO).collect();

    if levels.is_empty() {
        return Some(Decimal::ZERO);
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

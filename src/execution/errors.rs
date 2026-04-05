use rust_decimal::Decimal;
use crate::types::Platform;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("Platform {0} client not configured")]
    PlatformNotConfigured(Platform),

    #[error("Order timeout after {0}ms")]
    OrderTimeout(u64),

    #[error("Leg fill mismatch: expected {expected}, got {actual}")]
    FillMismatch { expected: Decimal, actual: Decimal },

    #[error("Rate limited on {platform}: {consecutive} consecutive 429s")]
    RateLimited { platform: Platform, consecutive: u32 },

    #[error("Order rejected by exchange: {0}")]
    OrderRejected(String),

    #[error("Signature error: {0}")]
    SignatureError(String),

    #[error("Unwind failed on {platform} market {market_id}: {reason}")]
    UnwindFailed { platform: Platform, market_id: String, reason: String },
}
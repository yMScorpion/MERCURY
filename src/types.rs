use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Polymarket,
    PolymarketUs,
    Kalshi,
    Cdna,
    ForecastEx,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Polymarket => write!(f, "polymarket"),
            Platform::PolymarketUs => write!(f, "polymarket_us"),
            Platform::Kalshi => write!(f, "kalshi"),
            Platform::Cdna => write!(f, "cdna"),
            Platform::ForecastEx => write!(f, "forecastex"),
        }
    }
}

impl Platform {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "polymarket" => Some(Platform::Polymarket),
            "polymarket_us" => Some(Platform::PolymarketUs),
            "kalshi" => Some(Platform::Kalshi),
            "cdna" => Some(Platform::Cdna),
            "forecastex" => Some(Platform::ForecastEx),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Yes,
    No,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Yes => write!(f, "yes"),
            Side::No => write!(f, "no"),
        }
    }
}

impl Side {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "yes" => Some(Side::Yes),
            "no" => Some(Side::No),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketCategory {
    Politics,
    Sports,
    Crypto,
    Economics,
    Science,
    Entertainment,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketStatus {
    Active,
    Closed,
    Resolved,
    Halted,
}

impl fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketStatus::Active => write!(f, "active"),
            MarketStatus::Closed => write!(f, "closed"),
            MarketStatus::Resolved => write!(f, "resolved"),
            MarketStatus::Halted => write!(f, "halted"),
        }
    }
}

impl MarketStatus {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "active" => Some(MarketStatus::Active),
            "closed" => Some(MarketStatus::Closed),
            "resolved" => Some(MarketStatus::Resolved),
            "halted" => Some(MarketStatus::Halted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlatformHealth {
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionState {
    Pending,
    Executing,
    PartialFill,
    Filled,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TradeStatus {
    Success,
    PartialSuccess,
    Failed,
    Skipped,
}

impl fmt::Display for TradeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeStatus::Success => write!(f, "success"),
            TradeStatus::PartialSuccess => write!(f, "partial_success"),
            TradeStatus::Failed => write!(f, "failed"),
            TradeStatus::Skipped => write!(f, "skipped"),
        }
    }
}

impl TradeStatus {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "success" => Some(TradeStatus::Success),
            "partial_success" => Some(TradeStatus::PartialSuccess),
            "failed" => Some(TradeStatus::Failed),
            "skipped" => Some(TradeStatus::Skipped),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub size: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTick {
    pub platform: Platform,
    pub market_id: String,
    pub timestamp_ns: u128,
    pub bid_price: Decimal,
    pub bid_size: Decimal,
    pub ask_price: Decimal,
    pub ask_size: Decimal,
    pub mid_price: Decimal,
    pub last_trade_price: Option<Decimal>,
    pub last_trade_size: Option<Decimal>,
    pub book_depth: u32,
    pub fee_rate_bps: Decimal,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformMarketInfo {
    pub platform: Platform,
    pub platform_market_id: String,
    pub platform_url: Option<String>,
    pub fee_rate_bps: Decimal,
    pub min_order_size: Decimal,
    pub tick_size: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    pub category: MarketCategory,
    pub status: MarketStatus,
    pub resolution_date: Option<DateTime<Utc>>,
    pub platforms: HashMap<Platform, PlatformMarketInfo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegDetail {
    pub platform: Platform,
    pub market_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub fee: Decimal,
    pub order_id: Option<String>,
    pub state: ExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub id: Uuid,
    pub market_id: String,
    pub buy_platform: Platform,
    pub sell_platform: Platform,
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub gross_spread: Decimal,
    pub net_spread: Decimal,
    pub buy_fee_bps: Decimal,
    pub sell_fee_bps: Decimal,
    pub max_size: Decimal,
    pub expected_profit: Decimal,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedOpportunity {
    pub opportunity: ArbitrageOpportunity,
    pub kelly_fraction: Decimal,
    pub optimal_size: Decimal,
    pub risk_score: Decimal,
    pub gas_cost_estimate: Decimal,
    pub net_expected_profit: Decimal,
    pub validated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResult {
    pub id: Uuid,
    pub opportunity_id: Uuid,
    pub market_id: String,
    pub legs: Vec<LegDetail>,
    pub gross_spread: Decimal,
    pub net_spread: Decimal,
    pub total_fees: Decimal,
    pub gas_cost: Decimal,
    pub profit: Decimal,
    pub status: TradeStatus,
    pub failure_reason: Option<String>,
    pub execution_ms: u64,
    pub bankroll_after: Decimal,
    pub bankroll_change_pct: Decimal,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    pub market_id: String,
    pub platform: Platform,
    pub side: Side,
    pub size: Decimal,
    pub entry_price: Decimal,
    pub current_price: Option<Decimal>,
    pub unrealized_pnl: Decimal,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBalance {
    pub platform: Platform,
    pub balance: Decimal,
    pub reserved: Decimal,
    pub available: Decimal,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySnapshot {
    pub date: String,
    pub total_bankroll: Decimal,
    pub total_pnl: Decimal,
    pub trade_count: u32,
    pub win_count: u32,
    pub loss_count: u32,
    pub best_trade_pnl: Decimal,
    pub worst_trade_pnl: Decimal,
    pub avg_spread_captured: Decimal,
    pub max_drawdown_pct: Decimal,
    pub platform_balances: HashMap<Platform, Decimal>,
    pub report_sent: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertMessage {
    TradeComplete(TradeResult),
    CircuitBreaker {
        reason: String,
        threshold: Decimal,
        actual: Decimal,
    },
    SystemAlert {
        level: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformDayStats {
    pub platform: Platform,
    pub trades: u32,
    pub volume: Decimal,
    pub pnl: Decimal,
    pub fees: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: String,
    pub snapshot: DailySnapshot,
    pub platform_stats: Vec<PlatformDayStats>,
    pub top_trades: Vec<TradeResult>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the current time in nanoseconds since UNIX epoch.
pub fn now_ns() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_nanos()
}

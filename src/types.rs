use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

// ─── Financial Type Safety (Newtypes) ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct BasisPoints(pub u32);

impl From<u16> for BasisPoints {
    fn from(v: u16) -> Self { Self(v as u32) }
}

impl fmt::Display for BasisPoints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Contracts(pub Decimal);

impl fmt::Display for Contracts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Usd(pub Decimal);

impl fmt::Display for Usd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── Platform ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    #[default]
    Polymarket,
    PolymarketUs,
    Kalshi,
    Cdna,
    ForecastEx,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Polymarket => write!(f, "Polymarket"),
            Platform::PolymarketUs => write!(f, "Polymarket US"),
            Platform::Kalshi => write!(f, "Kalshi"),
            Platform::Cdna => write!(f, "CDNA"),
            Platform::ForecastEx => write!(f, "ForecastEx"),
        }
    }
}

// ─── Side ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    #[default]
    Yes,
    No,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Yes => write!(f, "YES"),
            Side::No => write!(f, "NO"),
        }
    }
}

// ─── Market Category ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketCategory {
    Sports,
    Politics,
    Finance,
    Crypto,
    Weather,
    Culture,
    Other,
}

// ─── Market Status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Active,
    Suspended,
    Resolved,
    Expired,
}

impl fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketStatus::Active => write!(f, "active"),
            MarketStatus::Suspended => write!(f, "suspended"),
            MarketStatus::Resolved => write!(f, "resolved"),
            MarketStatus::Expired => write!(f, "expired"),
        }
    }
}

// ─── Platform Health ───
// Reserved for future per-platform health tracking in the monitoring subsystem.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum PlatformHealth {
    Healthy,
    Degraded,
    Down,
}

// ─── Price Level ───

use arrayvec::ArrayVec;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub size: Decimal,
}

// ─── Normalized Tick ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTick {
    pub platform: Platform,
    pub market_id: Uuid,
    pub timestamp_ns: u64,
    pub bid_price: Decimal,
    pub bid_size: Decimal,
    pub ask_price: Decimal,
    pub ask_size: Decimal,
    pub mid_price: Decimal,
    pub last_trade_price: Decimal,
    pub last_trade_size: Decimal,
    // HFT FIX: Stack-allocated depth (Max 10 bids + 10 asks = 20)
    // Eliminates thousands of heap allocations per second during tick storms
    pub book_depth: ArrayVec<PriceLevel, 20>, 
    pub fee_rate_bps: u16,
    pub sequence: u64,
}

// ─── Platform Market Info ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformMarketInfo {
    pub platform: Platform,
    pub platform_market_id: String,
    pub fee_rate_bps: u16,
    pub min_order_size: Decimal,
    pub tick_size: Decimal,
}

// ─── Market ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub unified_id: Uuid,
    pub question: String,
    pub resolution_source: String,
    pub expiration: DateTime<Utc>,
    pub platforms: HashMap<Platform, PlatformMarketInfo>,
    pub category: MarketCategory,
    pub confidence: f64,
    pub status: MarketStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Leg Detail ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegDetail {
    pub platform: Platform,
    /// Platform-native market/token identifier (e.g. Polymarket token ID, Kalshi ticker).
    /// Used by execution clients; distinct from the unified UUID `market_id` on the opportunity.
    pub platform_market_id: String,
    pub fee_rate_bps: u32,
    pub side: Side,
    pub price: Decimal,
    pub available_size: Decimal,
    pub fee_estimate: Decimal,
}

// ─── Arbitrage Opportunity ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub opp_id: Uuid,
    pub market_id: Uuid,
    pub market_question: String,
    pub leg_a: LegDetail,
    pub leg_b: LegDetail,
    pub raw_spread: Decimal,
    pub net_spread: Decimal,
    pub kelly_fraction: Decimal,
    pub recommended_size: Decimal,
    pub score: Decimal,
    pub detected_at: u64,
    pub ttl_ms: u32,
}

// ─── Validated Opportunity ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedOpportunity {
    pub opportunity: ArbitrageOpportunity,
    pub approved_size: Decimal,
    pub risk_score: Decimal,
}

// ─── Trade Status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TradeStatus {
    #[default]
    Success,
    Fail,
    Partial,
}

impl fmt::Display for TradeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeStatus::Success => write!(f, "success"),
            TradeStatus::Fail => write!(f, "fail"),
            TradeStatus::Partial => write!(f, "partial"),
        }
    }
}

// ─── Execution State ───
// Reserved for future stateful execution tracking in the executor.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ExecutionState {
    Pending,
    PartialFill,
    Filled,
    Failed,
    Unwinding,
}

// ─── Trade Result ───

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeResult {
    pub trade_id: i64,
    pub opp_id: Uuid,
    pub market_id: Uuid,
    pub market_question: String,
    pub leg_a_platform: Platform,
    pub leg_a_side: Side,
    pub leg_a_price: Decimal,
    pub leg_a_size: Decimal,
    pub leg_a_fill_price: Decimal,
    pub leg_a_fee: Decimal,
    pub leg_b_platform: Platform,
    pub leg_b_side: Side,
    pub leg_b_price: Decimal,
    pub leg_b_size: Decimal,
    pub leg_b_fill_price: Decimal,
    pub leg_b_fee: Decimal,
    pub raw_spread: Decimal,
    pub net_spread: Decimal,
    pub profit: Decimal,
    pub status: TradeStatus,
    pub failure_reason: Option<String>,
    pub execution_ms: u64,
    pub executed_at: DateTime<Utc>,
    pub bankroll_after: Decimal,
    pub bankroll_change_pct: Decimal,
    /// Size that was reserved in in_flight_notional when this trade was dispatched.
    /// Used by the event loop to release the reservation on settlement.
    /// Not persisted to the database.
    #[serde(default)]
    pub approved_size: Decimal,
}

// ─── Settlement Result ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    pub realized_pnl: Decimal,
    pub platform: Platform,
    pub market_id: Uuid,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
}

// ─── Position ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: i64,
    pub market_id: Uuid,
    pub platform: Platform,
    pub side: Side,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Platform Balance ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBalance {
    pub platform: Platform,
    pub available: Decimal,
    pub reserved: Decimal,
    pub pending_settlement: Decimal,
    pub total: Decimal,
    pub updated_at: DateTime<Utc>,
}

// ─── Daily Snapshot ───

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailySnapshot {
    pub date: NaiveDate,
    pub bankroll: Decimal,
    pub gross_pnl: Decimal,
    pub fees_paid: Decimal,
    pub net_pnl: Decimal,
    pub trades_count: i32,
    pub success_count: i32,
    pub fail_count: i32,
    pub success_rate: Decimal,
    pub peak_bankroll: Decimal,
    pub drawdown_pct: Decimal,
    pub kelly_utilization: Decimal,
    pub report_sent: bool,
}

// ─── Audit Entry ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp_ns: u64,
    pub module: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

// ─── Alert Message (for Telegram) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertMessage {
    TradeComplete(Box<TradeResult>),
    CircuitBreaker {
        breaker_type: String,
        details: String,
        action: String,
        resume_at: Option<DateTime<Utc>>,
    },
    SystemAlert {
        severity: String,
        message: String,
    },
}

// ─── Daily Report ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub snapshot: DailySnapshot,
    pub platform_breakdown: HashMap<Platform, PlatformDayStats>,
    pub top_trades: Vec<TradeResult>,
    pub worst_trades: Vec<TradeResult>,
    pub uptime_secs: u64,
    pub ws_reconnects: u32,
    pub api_errors: u32,
    pub db_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformDayStats {
    pub platform: Platform,
    pub exposure: Decimal,
    pub trade_count: i32,
    pub pnl: Decimal,
}

// ─── Timestamp helper ───

/// Returns nanoseconds since epoch. Cast to u64 wraps in 2554, which is acceptable.
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_nanos() as u64
}

// ─── System Commands (Telegram Control) ───

#[derive(Debug, Clone)]
pub enum SystemCommand {
    StartTrading,
    StopTrading,
    EnablePlatform(Platform),
    DisablePlatform(Platform),
    RequestDailyReport,
    ActivateMarket(Uuid),
}

#[derive(serde::Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    pub callback_query: Option<TelegramCallbackQuery>,
}

#[derive(serde::Deserialize)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub message: Option<TelegramMessage>,
    pub data: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct TelegramMessage {
    pub text: Option<String>,
    pub chat: TelegramChat,
    pub from: Option<TelegramUser>,
}

#[derive(serde::Deserialize)]
pub struct TelegramUser {
    pub id: i64,
}

#[derive(serde::Deserialize)]
pub struct TelegramChat {
    pub id: i64,
}

#[derive(serde::Deserialize)]
pub struct TelegramUpdatesResponse {
    pub ok: bool,
    pub result: Vec<TelegramUpdate>,
}

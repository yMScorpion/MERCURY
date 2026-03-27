# Phase 1: Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Rust project with core types, configuration system, database layer, and build infrastructure.

**Architecture:** Single Cargo binary crate with module hierarchy. All monetary values use `rust_decimal::Decimal`. Config loaded from YAML with hot-reload via `notify`. SQLite via `rusqlite` behind a `Database` trait for future Postgres swap.

**Tech Stack:** Rust, tokio, serde, rusqlite, r2d2, rust_decimal, clap, tracing, anyhow/thiserror, notify, chrono, uuid

---

### Task 1: Cargo Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `config/default.yaml`
- Create: `.gitignore`

- [ ] **Step 1: Create Cargo.toml with all dependencies**

```toml
[package]
name = "mercury"
version = "0.1.0"
edition = "2021"
description = "MERCURY - Cross-Market Prediction Arbitrage Engine"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }

# HTTP
reqwest = { version = "0.12", features = ["json", "native-tls"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# Database
rusqlite = { version = "0.31", features = ["bundled", "serde_json"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"

# Crypto
ethers = { version = "2", features = ["legacy"] }
jsonwebtoken = "9"
rsa = { version = "0.9", features = ["pem"] }
ring = "0.17"
aes-gcm = "0.10"

# Precision math
rust_decimal = { version = "1", features = ["serde-with-str"] }
rust_decimal_macros = "1"

# Time & IDs
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }

# Config hot-reload
notify = "6"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Error handling
anyhow = "1"
thiserror = "2"

# CLI
clap = { version = "4", features = ["derive"] }

# Misc
async-trait = "0.1"
futures-util = "0.3"
url = "2"
hex = "0.4"
base64 = "0.22"
rand = "0.8"
sha2 = "0.10"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

- [ ] **Step 2: Create minimal main.rs**

```rust
use anyhow::Result;
use clap::Parser;
use tracing::info;

mod config;
mod types;
mod db;

#[derive(Parser)]
#[command(name = "mercury", about = "MERCURY - Cross-Market Prediction Arbitrage Engine")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mercury=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    info!("MERCURY starting...");
    info!(config_path = %cli.config, "Loading configuration");

    let _config = config::MercuryConfig::load(&cli.config)?;
    info!("Configuration loaded successfully");

    Ok(())
}
```

- [ ] **Step 3: Create default.yaml config stub**

```yaml
# MERCURY Configuration
# All values can be overridden via environment variables prefixed with MERCURY_

trading:
  kelly_fraction_multiplier: 0.25
  min_net_spread_threshold: "0.0015"
  max_single_trade_pct: "0.02"
  max_daily_loss_pct: "0.05"
  max_drawdown_pct: "0.15"
  max_platform_exposure_pct: "0.40"
  stale_data_timeout_ms: 5000
  max_concurrent_arbs: 3
  gas_price_max_gwei: 500
  rebalance_threshold_pct: "0.10"
  max_open_positions: 50
  initial_bankroll: "1000.00"

platforms:
  polymarket:
    enabled: true
    ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market"
    rest_url: "https://clob.polymarket.com"
    sports_ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/sports"
  kalshi:
    enabled: true
    ws_url: "wss://trading-api.kalshi.com/trade-api/ws/v2"
    rest_url: "https://trading-api.kalshi.com/trade-api/v2"
  cdna:
    enabled: true
    ws_url: "wss://deriv-api.crypto.com/v1/ws"
    rest_url: "https://deriv-api.crypto.com/v1"
  forecastex:
    enabled: true
    fix_host: "fix.interactivebrokers.com"
    fix_port: 4001

database:
  path: "data/mercury.db"
  pool_size: 4
  busy_timeout_ms: 5000

telegram:
  enabled: true
  daily_report_hour_utc: 0

polygon_rpc:
  url: "https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY"
  gas_poll_interval_secs: 12

logging:
  level: "info"
  file: "logs/mercury.log"

health:
  port: 8080
```

- [ ] **Step 4: Create .gitignore**

```
/target
*.db
*.db-wal
*.db-shm
*.enc
.env
data/
logs/
keys/
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compilation succeeds (warnings about unused modules OK at this stage)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/main.rs config/default.yaml .gitignore
git commit -m "feat: scaffold Cargo project with dependencies and CLI entry point"
```

---

### Task 2: Core Types

**Files:**
- Create: `src/types.rs`

- [ ] **Step 1: Write core type definitions**

```rust
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

// ─── Platform ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
            Platform::Polymarket => write!(f, "Polymarket"),
            Platform::PolymarketUs => write!(f, "Polymarket US"),
            Platform::Kalshi => write!(f, "Kalshi"),
            Platform::Cdna => write!(f, "CDNA"),
            Platform::ForecastEx => write!(f, "ForecastEx"),
        }
    }
}

// ─── Market Side ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
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

// ─── Platform Health ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformHealth {
    Healthy,
    Degraded,
    Down,
}

// ─── Price Level ───

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub book_depth: Vec<PriceLevel>,
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

// ─── Validated Opportunity (passed risk checks) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedOpportunity {
    pub opportunity: ArbitrageOpportunity,
    pub approved_size: Decimal,
    pub risk_score: Decimal,
}

// ─── Trade Status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Pending,
    PartialFill,
    Filled,
    Failed,
    Unwinding,
}

// ─── Trade Result ───

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    TradeComplete(TradeResult),
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

pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (unused warnings OK)

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "feat: add core type definitions for all domain objects"
```

---

### Task 3: Configuration System

**Files:**
- Create: `src/config.rs`

- [ ] **Step 1: Write config structs and loader**

```rust
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MercuryConfig {
    pub trading: TradingConfig,
    pub platforms: PlatformsConfig,
    pub database: DatabaseConfig,
    pub telegram: TelegramConfig,
    pub polygon_rpc: PolygonRpcConfig,
    pub logging: LoggingConfig,
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub kelly_fraction_multiplier: f64,
    pub min_net_spread_threshold: Decimal,
    pub max_single_trade_pct: Decimal,
    pub max_daily_loss_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_platform_exposure_pct: Decimal,
    pub stale_data_timeout_ms: u64,
    pub max_concurrent_arbs: usize,
    pub gas_price_max_gwei: u64,
    pub rebalance_threshold_pct: Decimal,
    pub max_open_positions: usize,
    pub initial_bankroll: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformsConfig {
    pub polymarket: PolymarketConfig,
    pub kalshi: KalshiConfig,
    pub cdna: CdnaConfig,
    pub forecastex: ForecastExConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketConfig {
    pub enabled: bool,
    pub ws_url: String,
    pub rest_url: String,
    pub sports_ws_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalshiConfig {
    pub enabled: bool,
    pub ws_url: String,
    pub rest_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnaConfig {
    pub enabled: bool,
    pub ws_url: String,
    pub rest_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastExConfig {
    pub enabled: bool,
    pub fix_host: String,
    pub fix_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    pub pool_size: u32,
    pub busy_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub daily_report_hour_utc: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolygonRpcConfig {
    pub url: String,
    pub gas_poll_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub port: u16,
}

impl MercuryConfig {
    pub fn load(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let config: MercuryConfig = serde_yaml::from_str(&contents)
            .with_context(|| "Failed to parse config YAML")?;
        info!("Configuration loaded from {}", path);
        Ok(config)
    }
}

/// Thread-safe config holder with hot-reload support
#[derive(Clone)]
pub struct ConfigManager {
    inner: Arc<RwLock<MercuryConfig>>,
    path: String,
}

impl ConfigManager {
    pub fn new(config: MercuryConfig, path: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(config)),
            path,
        }
    }

    pub fn get(&self) -> MercuryConfig {
        self.inner.read().unwrap().clone()
    }

    pub fn reload(&self) -> Result<()> {
        let new_config = MercuryConfig::load(&self.path)?;
        let mut guard = self.inner.write().unwrap();
        *guard = new_config;
        info!("Configuration reloaded from {}", self.path);
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}
```

- [ ] **Step 2: Verify config loads from default.yaml**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: add configuration system with YAML loader and hot-reload support"
```

---

### Task 4: Database Trait Abstraction

**Files:**
- Create: `src/db/mod.rs`
- Create: `src/db/traits.rs`

- [ ] **Step 1: Write the Database trait**

`src/db/traits.rs`:
```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::types::*;

#[async_trait]
pub trait Database: Send + Sync + 'static {
    // Markets
    async fn upsert_market(&self, market: &Market) -> Result<()>;
    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>>;
    async fn get_active_markets(&self) -> Result<Vec<Market>>;

    // Trades
    async fn insert_trade(&self, result: &TradeResult) -> Result<i64>;
    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>>;
    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>>;
    async fn get_trade_count(&self) -> Result<i64>;

    // Positions
    async fn upsert_position(&self, position: &Position) -> Result<()>;
    async fn get_open_positions(&self) -> Result<Vec<Position>>;
    async fn close_position(&self, id: i64) -> Result<()>;

    // Balances
    async fn update_balance(&self, balance: &PlatformBalance) -> Result<()>;
    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>>;
    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>>;

    // Daily snapshots
    async fn insert_daily_snapshot(&self, snapshot: &DailySnapshot) -> Result<()>;
    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>>;
    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()>;

    // Audit log
    async fn append_audit(&self, entry: &AuditEntry) -> Result<()>;

    // Config history
    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()>;

    // Utility
    async fn db_size_bytes(&self) -> Result<u64>;
}
```

`src/db/mod.rs`:
```rust
pub mod traits;
pub mod sqlite;
pub mod migrations;

pub use traits::Database;
pub use sqlite::SqliteDb;
```

- [ ] **Step 2: Commit**

```bash
git add src/db/
git commit -m "feat: add Database trait abstraction for persistence layer"
```

---

### Task 5: SQLite Implementation

**Files:**
- Create: `src/db/sqlite.rs`
- Create: `src/db/migrations.rs`

- [ ] **Step 1: Write the migrations module**

`src/db/migrations.rs`:
```rust
use anyhow::Result;
use rusqlite::Connection;
use tracing::info;

const MIGRATIONS: &[&str] = &[
    // Migration 001: Initial schema
    r#"
    CREATE TABLE IF NOT EXISTS markets (
        unified_id TEXT PRIMARY KEY,
        question TEXT NOT NULL,
        resolution_source TEXT NOT NULL,
        expiration TEXT NOT NULL,
        platforms TEXT NOT NULL,  -- JSON
        category TEXT NOT NULL,
        confidence REAL NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS trades (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        opp_id TEXT NOT NULL,
        market_id TEXT NOT NULL,
        market_question TEXT NOT NULL DEFAULT '',
        leg_a_platform TEXT NOT NULL,
        leg_a_side TEXT NOT NULL,
        leg_a_price TEXT NOT NULL,
        leg_a_size TEXT NOT NULL,
        leg_a_fill_price TEXT NOT NULL,
        leg_a_fee TEXT NOT NULL,
        leg_b_platform TEXT NOT NULL,
        leg_b_side TEXT NOT NULL,
        leg_b_price TEXT NOT NULL,
        leg_b_size TEXT NOT NULL,
        leg_b_fill_price TEXT NOT NULL,
        leg_b_fee TEXT NOT NULL,
        raw_spread TEXT NOT NULL,
        net_spread TEXT NOT NULL,
        profit TEXT NOT NULL,
        status TEXT NOT NULL,
        failure_reason TEXT,
        execution_ms INTEGER NOT NULL,
        executed_at TEXT NOT NULL,
        bankroll_after TEXT NOT NULL DEFAULT '0',
        bankroll_change_pct TEXT NOT NULL DEFAULT '0',
        FOREIGN KEY (market_id) REFERENCES markets(unified_id)
    );

    CREATE TABLE IF NOT EXISTS positions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        market_id TEXT NOT NULL,
        platform TEXT NOT NULL,
        side TEXT NOT NULL,
        quantity TEXT NOT NULL,
        avg_entry_price TEXT NOT NULL,
        unrealized_pnl TEXT NOT NULL DEFAULT '0',
        opened_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        closed INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (market_id) REFERENCES markets(unified_id)
    );

    CREATE TABLE IF NOT EXISTS balances (
        platform TEXT PRIMARY KEY,
        available TEXT NOT NULL,
        reserved TEXT NOT NULL,
        pending_settlement TEXT NOT NULL,
        total TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS daily_snapshots (
        date TEXT PRIMARY KEY,
        bankroll TEXT NOT NULL,
        gross_pnl TEXT NOT NULL,
        fees_paid TEXT NOT NULL,
        net_pnl TEXT NOT NULL,
        trades_count INTEGER NOT NULL,
        success_count INTEGER NOT NULL,
        fail_count INTEGER NOT NULL,
        success_rate TEXT NOT NULL,
        peak_bankroll TEXT NOT NULL,
        drawdown_pct TEXT NOT NULL,
        kelly_utilization TEXT NOT NULL,
        report_sent INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp_ns INTEGER NOT NULL,
        module TEXT NOT NULL,
        event_type TEXT NOT NULL,
        data TEXT NOT NULL  -- JSON
    );

    CREATE TABLE IF NOT EXISTS config_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        changed_at TEXT NOT NULL,
        key TEXT NOT NULL,
        old_value TEXT NOT NULL,
        new_value TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_trades_executed_at ON trades(executed_at);
    CREATE INDEX IF NOT EXISTS idx_trades_market_id ON trades(market_id);
    CREATE INDEX IF NOT EXISTS idx_positions_market_id ON positions(market_id);
    CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(closed) WHERE closed = 0;
    CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp_ns);
    CREATE INDEX IF NOT EXISTS idx_audit_module ON audit_log(module);
    "#,
];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );"
    )?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for (i, migration) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current_version {
            info!(version, "Running migration");
            conn.execute_batch(migration)?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                [version],
            )?;
        }
    }

    info!(version = MIGRATIONS.len(), "Database schema up to date");
    Ok(())
}
```

- [ ] **Step 2: Write the SqliteDb implementation**

`src/db/sqlite.rs`:
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use super::migrations;
use super::traits::Database;
use crate::config::DatabaseConfig;
use crate::types::*;

pub struct SqliteDb {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteDb {
    pub fn new(config: &DatabaseConfig) -> Result<Self> {
        // Ensure data directory exists
        if let Some(parent) = std::path::Path::new(&config.path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let manager = SqliteConnectionManager::file(&config.path);
        let pool = Pool::builder()
            .max_size(config.pool_size)
            .build(manager)
            .context("Failed to create SQLite connection pool")?;

        // Configure SQLite pragmas and run migrations
        {
            let conn = pool.get()?;
            conn.execute_batch(&format!(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = {};
                 PRAGMA foreign_keys = ON;",
                config.busy_timeout_ms
            ))?;
            migrations::run_migrations(&conn)?;
        }

        Ok(Self { pool })
    }

    fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get().context("Failed to get DB connection from pool")
    }
}

// Helper to parse Decimal from SQLite text
fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap_or_default()
}

fn platform_from_str(s: &str) -> Platform {
    match s {
        "Polymarket" => Platform::Polymarket,
        "Polymarket US" | "PolymarketUs" => Platform::PolymarketUs,
        "Kalshi" => Platform::Kalshi,
        "CDNA" | "Cdna" => Platform::Cdna,
        "ForecastEx" => Platform::ForecastEx,
        _ => Platform::Polymarket,
    }
}

fn side_from_str(s: &str) -> Side {
    match s {
        "YES" | "Yes" => Side::Yes,
        _ => Side::No,
    }
}

fn trade_status_from_str(s: &str) -> TradeStatus {
    match s {
        "success" => TradeStatus::Success,
        "fail" => TradeStatus::Fail,
        _ => TradeStatus::Partial,
    }
}

#[async_trait]
impl Database for SqliteDb {
    async fn upsert_market(&self, market: &Market) -> Result<()> {
        let conn = self.conn()?;
        let platforms_json = serde_json::to_string(&market.platforms)?;
        conn.execute(
            "INSERT INTO markets (unified_id, question, resolution_source, expiration,
             platforms, category, confidence, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(unified_id) DO UPDATE SET
             question=?2, resolution_source=?3, expiration=?4, platforms=?5,
             category=?6, confidence=?7, status=?8, updated_at=?10",
            rusqlite::params![
                market.unified_id.to_string(),
                market.question,
                market.resolution_source,
                market.expiration.to_rfc3339(),
                platforms_json,
                serde_json::to_string(&market.category)?.trim_matches('"'),
                market.confidence,
                serde_json::to_string(&market.status)?.trim_matches('"'),
                market.created_at.to_rfc3339(),
                market.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT unified_id, question, resolution_source, expiration,
             platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE unified_id = ?1"
        )?;
        let mut rows = stmt.query(rusqlite::params![id.to_string()])?;
        match rows.next()? {
            Some(row) => {
                let platforms_str: String = row.get(4)?;
                let category_str: String = row.get(5)?;
                let status_str: String = row.get(7)?;
                Ok(Some(Market {
                    unified_id: Uuid::parse_str(&row.get::<_, String>(0)?)?,
                    question: row.get(1)?,
                    resolution_source: row.get(2)?,
                    expiration: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)?
                        .with_timezone(&Utc),
                    platforms: serde_json::from_str(&platforms_str)?,
                    category: serde_json::from_str(&format!("\"{}\"", category_str))?,
                    confidence: row.get(6)?,
                    status: serde_json::from_str(&format!("\"{}\"", status_str))?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)?
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)?
                        .with_timezone(&Utc),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_active_markets(&self) -> Result<Vec<Market>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT unified_id, question, resolution_source, expiration,
             platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE status = 'active'"
        )?;
        let mut markets = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let platforms_str: String = row.get(4)?;
            let category_str: String = row.get(5)?;
            let status_str: String = row.get(7)?;
            markets.push(Market {
                unified_id: Uuid::parse_str(&row.get::<_, String>(0)?)?,
                question: row.get(1)?,
                resolution_source: row.get(2)?,
                expiration: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)?
                    .with_timezone(&Utc),
                platforms: serde_json::from_str(&platforms_str)?,
                category: serde_json::from_str(&format!("\"{}\"", category_str))?,
                confidence: row.get(6)?,
                status: serde_json::from_str(&format!("\"{}\"", status_str))?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)?
                    .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)?
                    .with_timezone(&Utc),
            });
        }
        Ok(markets)
    }

    async fn insert_trade(&self, result: &TradeResult) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO trades (opp_id, market_id, market_question,
             leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee,
             leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee,
             raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at,
             bankroll_after, bankroll_change_pct)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
            rusqlite::params![
                result.opp_id.to_string(),
                result.market_id.to_string(),
                result.market_question,
                result.leg_a_platform.to_string(),
                result.leg_a_side.to_string(),
                result.leg_a_price.to_string(),
                result.leg_a_size.to_string(),
                result.leg_a_fill_price.to_string(),
                result.leg_a_fee.to_string(),
                result.leg_b_platform.to_string(),
                result.leg_b_side.to_string(),
                result.leg_b_price.to_string(),
                result.leg_b_size.to_string(),
                result.leg_b_fill_price.to_string(),
                result.leg_b_fee.to_string(),
                result.raw_spread.to_string(),
                result.net_spread.to_string(),
                result.profit.to_string(),
                result.status.to_string(),
                result.failure_reason,
                result.execution_ms as i64,
                result.executed_at.to_rfc3339(),
                result.bankroll_after.to_string(),
                result.bankroll_change_pct.to_string(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, opp_id, market_id, market_question,
             leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee,
             leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee,
             raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at,
             bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= ?1 ORDER BY executed_at ASC"
        )?;
        let mut trades = Vec::new();
        let mut rows = stmt.query(rusqlite::params![since.to_rfc3339()])?;
        while let Some(row) = rows.next()? {
            trades.push(TradeResult {
                trade_id: row.get(0)?,
                opp_id: Uuid::parse_str(&row.get::<_, String>(1)?)?,
                market_id: Uuid::parse_str(&row.get::<_, String>(2)?)?,
                market_question: row.get(3)?,
                leg_a_platform: platform_from_str(&row.get::<_, String>(4)?),
                leg_a_side: side_from_str(&row.get::<_, String>(5)?),
                leg_a_price: dec(&row.get::<_, String>(6)?),
                leg_a_size: dec(&row.get::<_, String>(7)?),
                leg_a_fill_price: dec(&row.get::<_, String>(8)?),
                leg_a_fee: dec(&row.get::<_, String>(9)?),
                leg_b_platform: platform_from_str(&row.get::<_, String>(10)?),
                leg_b_side: side_from_str(&row.get::<_, String>(11)?),
                leg_b_price: dec(&row.get::<_, String>(12)?),
                leg_b_size: dec(&row.get::<_, String>(13)?),
                leg_b_fill_price: dec(&row.get::<_, String>(14)?),
                leg_b_fee: dec(&row.get::<_, String>(15)?),
                raw_spread: dec(&row.get::<_, String>(16)?),
                net_spread: dec(&row.get::<_, String>(17)?),
                profit: dec(&row.get::<_, String>(18)?),
                status: trade_status_from_str(&row.get::<_, String>(19)?),
                failure_reason: row.get(20)?,
                execution_ms: row.get::<_, i64>(21)? as u64,
                executed_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(22)?)?
                    .with_timezone(&Utc),
                bankroll_after: dec(&row.get::<_, String>(23)?),
                bankroll_change_pct: dec(&row.get::<_, String>(24)?),
            });
        }
        Ok(trades)
    }

    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>> {
        let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end = date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, opp_id, market_id, market_question,
             leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee,
             leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee,
             raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at,
             bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= ?1 AND executed_at < ?2 ORDER BY executed_at ASC"
        )?;
        let mut trades = Vec::new();
        let mut rows = stmt.query(rusqlite::params![start.to_rfc3339(), end.to_rfc3339()])?;
        while let Some(row) = rows.next()? {
            trades.push(TradeResult {
                trade_id: row.get(0)?,
                opp_id: Uuid::parse_str(&row.get::<_, String>(1)?)?,
                market_id: Uuid::parse_str(&row.get::<_, String>(2)?)?,
                market_question: row.get(3)?,
                leg_a_platform: platform_from_str(&row.get::<_, String>(4)?),
                leg_a_side: side_from_str(&row.get::<_, String>(5)?),
                leg_a_price: dec(&row.get::<_, String>(6)?),
                leg_a_size: dec(&row.get::<_, String>(7)?),
                leg_a_fill_price: dec(&row.get::<_, String>(8)?),
                leg_a_fee: dec(&row.get::<_, String>(9)?),
                leg_b_platform: platform_from_str(&row.get::<_, String>(10)?),
                leg_b_side: side_from_str(&row.get::<_, String>(11)?),
                leg_b_price: dec(&row.get::<_, String>(12)?),
                leg_b_size: dec(&row.get::<_, String>(13)?),
                leg_b_fill_price: dec(&row.get::<_, String>(14)?),
                leg_b_fee: dec(&row.get::<_, String>(15)?),
                raw_spread: dec(&row.get::<_, String>(16)?),
                net_spread: dec(&row.get::<_, String>(17)?),
                profit: dec(&row.get::<_, String>(18)?),
                status: trade_status_from_str(&row.get::<_, String>(19)?),
                failure_reason: row.get(20)?,
                execution_ms: row.get::<_, i64>(21)? as u64,
                executed_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(22)?)?
                    .with_timezone(&Utc),
                bankroll_after: dec(&row.get::<_, String>(23)?),
                bankroll_change_pct: dec(&row.get::<_, String>(24)?),
            });
        }
        Ok(trades)
    }

    async fn get_trade_count(&self) -> Result<i64> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM trades", [], |r| r.get(0))?;
        Ok(count)
    }

    async fn upsert_position(&self, position: &Position) -> Result<()> {
        let conn = self.conn()?;
        if position.id > 0 {
            conn.execute(
                "UPDATE positions SET quantity=?1, avg_entry_price=?2, unrealized_pnl=?3, updated_at=?4
                 WHERE id=?5",
                rusqlite::params![
                    position.quantity.to_string(),
                    position.avg_entry_price.to_string(),
                    position.unrealized_pnl.to_string(),
                    position.updated_at.to_rfc3339(),
                    position.id,
                ],
            )?;
        } else {
            conn.execute(
                "INSERT INTO positions (market_id, platform, side, quantity, avg_entry_price,
                 unrealized_pnl, opened_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![
                    position.market_id.to_string(),
                    position.platform.to_string(),
                    position.side.to_string(),
                    position.quantity.to_string(),
                    position.avg_entry_price.to_string(),
                    position.unrealized_pnl.to_string(),
                    position.opened_at.to_rfc3339(),
                    position.updated_at.to_rfc3339(),
                ],
            )?;
        }
        Ok(())
    }

    async fn get_open_positions(&self) -> Result<Vec<Position>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, market_id, platform, side, quantity, avg_entry_price,
             unrealized_pnl, opened_at, updated_at
             FROM positions WHERE closed = 0"
        )?;
        let mut positions = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            positions.push(Position {
                id: row.get(0)?,
                market_id: Uuid::parse_str(&row.get::<_, String>(1)?)?,
                platform: platform_from_str(&row.get::<_, String>(2)?),
                side: side_from_str(&row.get::<_, String>(3)?),
                quantity: dec(&row.get::<_, String>(4)?),
                avg_entry_price: dec(&row.get::<_, String>(5)?),
                unrealized_pnl: dec(&row.get::<_, String>(6)?),
                opened_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)?
                    .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)?
                    .with_timezone(&Utc),
            });
        }
        Ok(positions)
    }

    async fn close_position(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("UPDATE positions SET closed = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    async fn update_balance(&self, balance: &PlatformBalance) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO balances (platform, available, reserved, pending_settlement, total, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(platform) DO UPDATE SET
             available=?2, reserved=?3, pending_settlement=?4, total=?5, updated_at=?6",
            rusqlite::params![
                balance.platform.to_string(),
                balance.available.to_string(),
                balance.reserved.to_string(),
                balance.pending_settlement.to_string(),
                balance.total.to_string(),
                balance.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM balances WHERE platform = ?1"
        )?;
        let mut rows = stmt.query(rusqlite::params![platform.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(PlatformBalance {
                platform: platform_from_str(&row.get::<_, String>(0)?),
                available: dec(&row.get::<_, String>(1)?),
                reserved: dec(&row.get::<_, String>(2)?),
                pending_settlement: dec(&row.get::<_, String>(3)?),
                total: dec(&row.get::<_, String>(4)?),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)?
                    .with_timezone(&Utc),
            })),
            None => Ok(None),
        }
    }

    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM balances"
        )?;
        let mut balances = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            balances.push(PlatformBalance {
                platform: platform_from_str(&row.get::<_, String>(0)?),
                available: dec(&row.get::<_, String>(1)?),
                reserved: dec(&row.get::<_, String>(2)?),
                pending_settlement: dec(&row.get::<_, String>(3)?),
                total: dec(&row.get::<_, String>(4)?),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)?
                    .with_timezone(&Utc),
            });
        }
        Ok(balances)
    }

    async fn insert_daily_snapshot(&self, snapshot: &DailySnapshot) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO daily_snapshots (date, bankroll, gross_pnl, fees_paid, net_pnl,
             trades_count, success_count, fail_count, success_rate, peak_bankroll,
             drawdown_pct, kelly_utilization, report_sent)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(date) DO UPDATE SET
             bankroll=?2, gross_pnl=?3, fees_paid=?4, net_pnl=?5, trades_count=?6,
             success_count=?7, fail_count=?8, success_rate=?9, peak_bankroll=?10,
             drawdown_pct=?11, kelly_utilization=?12",
            rusqlite::params![
                snapshot.date.to_string(),
                snapshot.bankroll.to_string(),
                snapshot.gross_pnl.to_string(),
                snapshot.fees_paid.to_string(),
                snapshot.net_pnl.to_string(),
                snapshot.trades_count,
                snapshot.success_count,
                snapshot.fail_count,
                snapshot.success_rate.to_string(),
                snapshot.peak_bankroll.to_string(),
                snapshot.drawdown_pct.to_string(),
                snapshot.kelly_utilization.to_string(),
                snapshot.report_sent as i32,
            ],
        )?;
        Ok(())
    }

    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count,
             success_count, fail_count, success_rate, peak_bankroll, drawdown_pct,
             kelly_utilization, report_sent
             FROM daily_snapshots WHERE date = ?1"
        )?;
        let mut rows = stmt.query(rusqlite::params![date.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(DailySnapshot {
                date: NaiveDate::parse_from_str(&row.get::<_, String>(0)?, "%Y-%m-%d")?,
                bankroll: dec(&row.get::<_, String>(1)?),
                gross_pnl: dec(&row.get::<_, String>(2)?),
                fees_paid: dec(&row.get::<_, String>(3)?),
                net_pnl: dec(&row.get::<_, String>(4)?),
                trades_count: row.get(5)?,
                success_count: row.get(6)?,
                fail_count: row.get(7)?,
                success_rate: dec(&row.get::<_, String>(8)?),
                peak_bankroll: dec(&row.get::<_, String>(9)?),
                drawdown_pct: dec(&row.get::<_, String>(10)?),
                kelly_utilization: dec(&row.get::<_, String>(11)?),
                report_sent: row.get::<_, i32>(12)? != 0,
            })),
            None => Ok(None),
        }
    }

    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE daily_snapshots SET report_sent = 1 WHERE date = ?1",
            rusqlite::params![date.to_string()],
        )?;
        Ok(())
    }

    async fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO audit_log (timestamp_ns, module, event_type, data)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                entry.timestamp_ns as i64,
                entry.module,
                entry.event_type,
                entry.data.to_string(),
            ],
        )?;
        Ok(())
    }

    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO config_history (changed_at, key, old_value, new_value)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![Utc::now().to_rfc3339(), key, old_val, new_val],
        )?;
        Ok(())
    }

    async fn db_size_bytes(&self) -> Result<u64> {
        let conn = self.conn()?;
        let page_count: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: u64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(page_count * page_size)
    }
}
```

- [ ] **Step 3: Update main.rs to wire up DB**

Update `src/main.rs` to add after config loading:

```rust
    let config = config::MercuryConfig::load(&cli.config)?;
    info!("Configuration loaded successfully");

    let database = db::SqliteDb::new(&config.database)?;
    info!("Database initialized");
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add src/db/ src/main.rs
git commit -m "feat: add SQLite database layer with migrations and full CRUD implementation"
```

---

### Task 6: Build Verification

- [ ] **Step 1: Full cargo build**

Run: `cargo build`
Expected: Compiles with no errors (warnings about unused OK)

- [ ] **Step 2: Run the binary with default config**

Run: `cargo run -- --config config/default.yaml`
Expected: Prints "MERCURY starting...", "Configuration loaded successfully", "Database initialized", then exits cleanly

- [ ] **Step 3: Verify SQLite DB was created**

Run: `ls -la data/mercury.db`
Expected: File exists

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: phase 1 foundation complete - types, config, database"
```

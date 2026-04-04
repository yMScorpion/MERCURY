use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tracing::info;

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
    pub kelly_fraction_multiplier: Decimal,
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
        config.validate()?;
        info!("Configuration loaded from {}", path);
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        let t = &self.trading;
        
        // HIGH-5 FIX: Prevent zero-timeout configurations that would permanently halt trading
        anyhow::ensure!(t.stale_data_timeout_ms >= 2000,
            "stale_data_timeout_ms must be at least 2000ms (WebSocket latency + processing), got {}", t.stale_data_timeout_ms);
        anyhow::ensure!(t.min_net_spread_threshold > Decimal::ZERO,
            "min_net_spread_threshold must be positive, got {}", t.min_net_spread_threshold);
            
        anyhow::ensure!(t.initial_bankroll > Decimal::ZERO,
            "initial_bankroll must be positive, got {}", t.initial_bankroll);
        anyhow::ensure!(t.kelly_fraction_multiplier > Decimal::ZERO && t.kelly_fraction_multiplier <= Decimal::ONE,
            "kelly_fraction_multiplier must be in (0, 1], got {}", t.kelly_fraction_multiplier);
        anyhow::ensure!(t.max_daily_loss_pct > Decimal::ZERO && t.max_daily_loss_pct <= Decimal::ONE,
            "max_daily_loss_pct must be in (0, 1] (fraction scale), got {}", t.max_daily_loss_pct);
        anyhow::ensure!(t.max_drawdown_pct > Decimal::ZERO && t.max_drawdown_pct <= Decimal::ONE,
            "max_drawdown_pct must be in (0, 1] (fraction scale), got {}", t.max_drawdown_pct);
        anyhow::ensure!(t.max_platform_exposure_pct > Decimal::ZERO && t.max_platform_exposure_pct <= Decimal::ONE,
            "max_platform_exposure_pct must be in (0, 1] (fraction scale), got {}", t.max_platform_exposure_pct);
        // Hard ceiling: 10% per trade. This cannot be overridden by config YAML.
        // At 100% a single bug or bad fill wipes the entire bankroll in one trade.
        // This limit is intentionally conservative for a live trading system.
        anyhow::ensure!(
            t.max_single_trade_pct > Decimal::ZERO && t.max_single_trade_pct <= rust_decimal_macros::dec!(0.10),
            "max_single_trade_pct must be in (0, 0.10] — absolute safety ceiling is 10%; got {}",
            t.max_single_trade_pct
        );
        anyhow::ensure!(t.max_open_positions >= 1,
            "max_open_positions must be at least 1, got {}", t.max_open_positions);
        anyhow::ensure!(t.max_concurrent_arbs >= 1,
            "max_concurrent_arbs must be at least 1, got {}", t.max_concurrent_arbs);
        anyhow::ensure!(t.rebalance_threshold_pct > Decimal::ZERO && t.rebalance_threshold_pct <= Decimal::ONE,
            "rebalance_threshold_pct must be in (0, 1], got {}", t.rebalance_threshold_pct);
            
        // Validate platform URLs
        let p = &self.platforms;
        if p.polymarket.enabled {
            anyhow::ensure!(!p.polymarket.ws_url.is_empty(),
                "Polymarket enabled but ws_url is empty");
            anyhow::ensure!(!p.polymarket.rest_url.is_empty(),
                "Polymarket enabled but rest_url is empty");
            anyhow::ensure!(p.polymarket.ws_url.starts_with("wss://") || p.polymarket.ws_url.starts_with("ws://"),
                "Polymarket ws_url must start with ws:// or wss://, got: {}", p.polymarket.ws_url);
        }
        if p.kalshi.enabled {
            anyhow::ensure!(!p.kalshi.ws_url.is_empty(),
                "Kalshi enabled but ws_url is empty");
            anyhow::ensure!(!p.kalshi.rest_url.is_empty(),
                "Kalshi enabled but rest_url is empty");
        }
        if p.cdna.enabled {
            anyhow::ensure!(!p.cdna.ws_url.is_empty(),
                "CDNA enabled but ws_url is empty");
            anyhow::ensure!(!p.cdna.rest_url.is_empty(),
                "CDNA enabled but rest_url is empty");
        }
        if p.forecastex.enabled {
            anyhow::ensure!(!p.forecastex.fix_host.is_empty(),
                "ForecastEx enabled but fix_host is empty");
            anyhow::ensure!(p.forecastex.fix_port > 0,
                "ForecastEx enabled but fix_port is 0");
        }
        Ok(())
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
        match self.inner.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                tracing::error!("Config RwLock was poisoned — a thread panicked while holding the lock. Using last known config.");
                poisoned.into_inner().clone()
            }
        }
    }

    pub fn reload(&self) -> Result<()> {
        let new_config = MercuryConfig::load(&self.path)?;
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = new_config;
        info!("Configuration reloaded from {}", self.path);
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub kelly_fraction_multiplier: f64,
    pub min_net_spread_threshold: f64,
    pub max_single_trade_pct: f64,
    pub max_daily_loss_pct: f64,
    pub max_drawdown_pct: f64,
    pub max_platform_exposure_pct: f64,
    pub stale_data_timeout_ms: u64,
    pub max_concurrent_arbs: u32,
    pub gas_price_max_gwei: u64,
    pub rebalance_threshold_pct: f64,
    pub max_open_positions: u32,
    pub initial_bankroll: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub enabled: bool,
    pub rest_url: String,
    pub ws_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformsConfig {
    pub polymarket: PlatformConfig,
    pub kalshi: PlatformConfig,
    pub cdna: PlatformConfig,
    pub forecastex: PlatformConfig,
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

impl MercuryConfig {
    /// Load configuration from a YAML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let config: MercuryConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
        Ok(config)
    }
}

// ---------------------------------------------------------------------------
// ConfigManager – supports hot-reload
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<MercuryConfig>>,
    path: PathBuf,
}

impl ConfigManager {
    /// Create a new ConfigManager by loading the config from the given path.
    pub fn new(path: &Path) -> Result<Self> {
        let config = MercuryConfig::load(path)?;
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            path: path.to_path_buf(),
        })
    }

    /// Get a snapshot of the current config.
    pub async fn get(&self) -> MercuryConfig {
        self.config.read().await.clone()
    }

    /// Reload the config from disk.
    pub async fn reload(&self) -> Result<()> {
        let new_config = MercuryConfig::load(&self.path)?;
        let mut w = self.config.write().await;
        *w = new_config;
        Ok(())
    }

    /// Return the config file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

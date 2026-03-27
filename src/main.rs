mod config;
mod db;
mod types;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "mercury", about = "Cross-market prediction arbitrage engine")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("Mercury starting up");

    // Load configuration
    let cfg_manager = config::ConfigManager::new(&cli.config)?;
    let cfg = cfg_manager.get().await;

    info!(
        config_path = %cli.config.display(),
        db_path = %cfg.database.path,
        "configuration loaded"
    );

    // Initialize database
    let db = db::SqliteDb::new(
        &cfg.database.path,
        cfg.database.pool_size,
        cfg.database.busy_timeout_ms,
    )?;

    let size = db::Database::db_size_bytes(&db).await?;
    info!(db_size_bytes = size, "database initialized");

    // Startup summary
    let enabled_platforms: Vec<&str> = [
        cfg.platforms.polymarket.enabled.then_some("polymarket"),
        cfg.platforms.kalshi.enabled.then_some("kalshi"),
        cfg.platforms.cdna.enabled.then_some("cdna"),
        cfg.platforms.forecastex.enabled.then_some("forecastex"),
    ]
    .into_iter()
    .flatten()
    .collect();

    info!(
        platforms = ?enabled_platforms,
        initial_bankroll = %cfg.trading.initial_bankroll,
        max_concurrent_arbs = cfg.trading.max_concurrent_arbs,
        "Mercury Phase 1 foundation ready"
    );

    if enabled_platforms.is_empty() {
        warn!("no platforms enabled -- nothing to do");
    }

    info!("Phase 1 init complete. Exiting.");
    Ok(())
}

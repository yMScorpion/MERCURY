mod config;
mod db;
mod types;

use anyhow::Result;
use clap::Parser;
use tracing::info;

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

    let cfg = config::MercuryConfig::load(&cli.config)?;

    // Initialize database
    let db = db::SqliteDb::new(
        &cfg.database.path,
        cfg.database.pool_size,
        cfg.database.busy_timeout_ms,
    )?;

    let size = db::traits::Database::db_size_bytes(&db).await?;
    info!(db_size_bytes = size, "Database initialized");

    info!("MERCURY Phase 1 foundation ready");
    Ok(())
}

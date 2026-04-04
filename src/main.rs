use anyhow::Result;
use clap::Parser;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

mod config;
mod crypto;
mod db;
mod engine;
mod execution;
mod feeds;
mod inventory;
mod monitoring;
mod risk;
mod telegram;
mod types;

use config::MercuryConfig;
use db::SqliteDb;
use types::*;

#[derive(Parser)]
#[command(name = "mercury", about = "MERCURY - Cross-Market Prediction Arbitrage Engine")]
struct Cli {
    #[arg(short, long, default_value = "config/default.yaml")]
    config: String,
}

// C-4 FIX: Helper to manually load env vars without exposing them to the process environment
fn load_env_vars() -> std::collections::HashMap<String, String> {
    let mut env_vars = std::collections::HashMap::new();
    if let Ok(contents) = std::fs::read_to_string("/opt/mercury/.env") {
        for line in contents.lines() {
            if let Some((k, v)) = line.split_once('=') {
                env_vars.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    env_vars
}

// H-5 FIX: Extracted duplicated report generation logic
async fn build_daily_report(
    db_clone: Arc<dyn db::Database>,
    snapshot: DailySnapshot,
    uptime_secs: u64,
    ws_reconnects: u32,
    api_errors: u32,
) -> DailyReport {
    let trades = db_clone.get_trades_for_date(chrono::Utc::now().date_naive()).await.unwrap_or_default();

    let mut platform_breakdown = std::collections::HashMap::new();
    for trade in &trades {
        let total_cost = (trade.leg_a_size * trade.leg_a_fill_price) + (trade.leg_b_size * trade.leg_b_fill_price);
        let (ratio_a, ratio_b) = if total_cost > rust_decimal::Decimal::ZERO {
            ((trade.leg_a_size * trade.leg_a_fill_price) / total_cost, (trade.leg_b_size * trade.leg_b_fill_price) / total_cost)
        } else {
            (rust_decimal_macros::dec!(0.5), rust_decimal_macros::dec!(0.5))
        };

        let entry_a = platform_breakdown.entry(trade.leg_a_platform)
            .or_insert_with(|| PlatformDayStats {
                platform: trade.leg_a_platform,
                exposure: rust_decimal::Decimal::ZERO,
                trade_count: 0,
                pnl: rust_decimal::Decimal::ZERO,
            });
        entry_a.trade_count += 1;
        entry_a.pnl += trade.profit * ratio_a;

        let entry_b = platform_breakdown.entry(trade.leg_b_platform)
            .or_insert_with(|| PlatformDayStats {
                platform: trade.leg_b_platform,
                exposure: rust_decimal::Decimal::ZERO,
                trade_count: 0,
                pnl: rust_decimal::Decimal::ZERO,
            });
        entry_b.trade_count += 1;
        entry_b.pnl += trade.profit * ratio_b;
    }

    let mut sorted_trades = trades.clone();
    sorted_trades.sort_by(|a, b| b.profit.cmp(&a.profit));
    let top_trades: Vec<TradeResult> = sorted_trades.iter().take(3).cloned().collect();
    sorted_trades.sort_by(|a, b| a.profit.cmp(&b.profit));
    let worst_trades: Vec<TradeResult> = sorted_trades.iter().take(3).cloned().collect();

    let db_size_bytes = db_clone.db_size_bytes().await.unwrap_or(0);

    DailyReport {
        snapshot,
        platform_breakdown,
        top_trades,
        worst_trades,
        uptime_secs,
        ws_reconnects,
        api_errors,
        db_size_bytes,
    }
}

/// Calculate seconds from now until the next occurrence of `hour_utc`:00 UTC.
fn seconds_until_report_hour(hour_utc: u32) -> u64 {
    let now = chrono::Utc::now();
    let today_target = now.date_naive()
        .and_hms_opt(hour_utc, 0, 0)
        .unwrap()
        .and_utc();
    let target = if today_target > now {
        today_target
    } else {
        today_target + chrono::Duration::days(1)
    };
    (target - now).num_seconds().max(1) as u64
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use rust_decimal_macros::dec;
    use crate::engine::order_book::UnifiedOrderBook;
    use crate::engine::spread::NetSpreadEngine;
    use crate::engine::detector::ArbitrageDetector;
    use crate::engine::market_registry::MarketRegistry;
    use crate::types::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_main_event_loop_discovery_and_execution() {
        // L-4 FIX: Basic integration test scaffolding for the engine pipeline
        let mut registry = MarketRegistry::new();
        let market_id = Uuid::new_v4();
        let mut platforms = std::collections::HashMap::new();
        platforms.insert(Platform::Polymarket, PlatformMarketInfo {
            platform: Platform::Polymarket,
            platform_market_id: "poly_token".into(),
            fee_rate_bps: 200,
            min_order_size: dec!(1.0),
            tick_size: dec!(0.01),
        });
        platforms.insert(Platform::Kalshi, PlatformMarketInfo {
            platform: Platform::Kalshi,
            platform_market_id: "kalshi_ticker".into(),
            fee_rate_bps: 175,
            min_order_size: dec!(1.0),
            tick_size: dec!(0.01),
        });

        registry.register_market(Market {
            unified_id: market_id,
            question: "Test Market".into(),
            resolution_source: "Test".into(),
            expiration: chrono::Utc::now() + chrono::Duration::hours(1),
            platforms,
            category: MarketCategory::Other,
            confidence: 0.99,
            status: MarketStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });

        let mut uob = UnifiedOrderBook::new();
        let mut spread_engine = NetSpreadEngine::new(dec!(0.01));
        let mut detector = ArbitrageDetector::new(dec!(0.01), dec!(1.0), 5000, 3);

        // Simulate Polymarket Tick (Ask YES at 0.40)
        let poly_tick = NormalizedTick {
            platform: Platform::Polymarket,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: dec!(0.38),
            bid_size: dec!(100),
            ask_price: dec!(0.40),
            ask_size: dec!(100),
            mid_price: dec!(0.39),
            last_trade_price: dec!(0.39),
            last_trade_size: dec!(10),
            book_depth: std::sync::Arc::new(vec![
                PriceLevel { price: dec!(0.40), size: dec!(100) },
                PriceLevel { price: dec!(0.38), size: dec!(100) },
            ]),
            fee_rate_bps: 200,
            sequence: 1,
        };
        uob.update(&poly_tick);

        // Simulate Kalshi Tick (Ask NO at 0.50 -> Implies YES Bid at 0.50)
        let kalshi_tick = NormalizedTick {
            platform: Platform::Kalshi,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: dec!(0.50), // Someone bidding YES at 0.50
            bid_size: dec!(100),
            ask_price: dec!(0.52),
            ask_size: dec!(100),
            mid_price: dec!(0.51),
            last_trade_price: dec!(0.51),
            last_trade_size: dec!(10),
            book_depth: std::sync::Arc::new(vec![
                PriceLevel { price: dec!(0.52), size: dec!(100) },
                PriceLevel { price: dec!(0.50), size: dec!(100) },
            ]),
            fee_rate_bps: 175,
            sequence: 1,
        };
        uob.update(&kalshi_tick);

        let opps = detector.detect_for_market(
            &market_id,
            &registry,
            &uob,
            &spread_engine,
            dec!(10.0),
        );

        assert_eq!(opps.len(), 1, "Should detect 1 arb opportunity");
        let opp = &opps[0];
        assert_eq!(opp.leg_a.platform, Platform::Polymarket);
        assert_eq!(opp.leg_b.platform, Platform::Kalshi);
        assert!(opp.net_spread > dec!(0.05), "Net spread should be positive");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mercury_config = MercuryConfig::load(&cli.config)?;
    
    // L-2 FIX: Structured JSON logging to file for production
    let log_file_path = std::path::Path::new(&mercury_config.logging.file);
    let log_dir = log_file_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let log_name = log_file_path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("mercury.log"));
    let file_appender = tracing_appender::rolling::daily(log_dir, log_name);
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(mercury_config.logging.level.parse().unwrap_or_else(|_| "mercury=info".parse().unwrap())),
        )
        .with_writer(non_blocking_writer)
        .init();

    info!("MERCURY v{} starting...", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded");

    // HARD PANIC FOR FORECAST EX
    if mercury_config.platforms.forecastex.enabled {
        drop(_guard); // LOW-5 FIX: Ensure logs are completely flushed before process aborts via panic
        panic!("CRITICAL: ForecastEx FIX execution is not fully implemented. Do not run with forecastex.enabled = true to prevent unhedged dual-leg exposure.");
    }

    let db: Arc<dyn db::Database> = Arc::new(SqliteDb::new(
        &mercury_config.database.path,
        mercury_config.database.pool_size,
        mercury_config.database.busy_timeout_ms,
    ).await?);
    info!("Database initialized via SQLx");

    let metrics = monitoring::metrics::Metrics::new();

    // ─── Channels ───
    let (tick_tx, _) = broadcast::channel::<NormalizedTick>(10_000); // 10k buffer = ~10s at 1000 ticks/s
    let (opportunity_tx, opportunity_rx) = mpsc::channel::<ValidatedOpportunity>(100);
    let (trade_result_tx, trade_result_rx) = mpsc::channel::<TradeResult>(100);
    let (trade_result_tx2, trade_result_rx2) = mpsc::channel::<TradeResult>(1_000);
    let (alert_tx, alert_rx) = mpsc::channel::<AlertMessage>(500);
    let (daily_report_tx, daily_report_rx) = mpsc::channel::<DailyReport>(10);
    let (gas_update_tx, mut gas_update_rx) = mpsc::channel::<monitoring::gas_oracle::GasUpdate>(16);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<SystemCommand>(10); // Command routing from Telegram
    let (settlement_tx, mut settlement_rx) = mpsc::channel::<SettlementResult>(100);

    let env_vars = load_env_vars();

    // ─── Telegram ───
    let tg_notification_token = std::env::var("TELEGRAM_NOTIFICATION_TOKEN").unwrap_or_else(|_| env_vars.get("TELEGRAM_NOTIFICATION_TOKEN").cloned().unwrap_or_default());
    let tg_daily_token = std::env::var("TELEGRAM_DAILY_TOKEN").unwrap_or_else(|_| env_vars.get("TELEGRAM_DAILY_TOKEN").cloned().unwrap_or_default());
    let tg_alerts_chat = std::env::var("TELEGRAM_ALERTS_CHAT_ID").unwrap_or_else(|_| env_vars.get("TELEGRAM_ALERTS_CHAT_ID").cloned().unwrap_or_default());
    let tg_report_chat = std::env::var("TELEGRAM_REPORT_CHAT_ID").unwrap_or_else(|_| env_vars.get("TELEGRAM_REPORT_CHAT_ID").cloned().unwrap_or_default());

    let tg_alerts_enabled = mercury_config.telegram.enabled
        && !tg_notification_token.is_empty()
        && !tg_alerts_chat.is_empty()
        && tg_alerts_chat != "YOUR_CHAT_ID_HERE";
    let tg_reports_enabled = mercury_config.telegram.enabled
        && !tg_daily_token.is_empty()
        && !tg_report_chat.is_empty()
        && tg_report_chat != "YOUR_CHAT_ID_HERE";
    let _tg_enabled = tg_alerts_enabled || tg_reports_enabled;

    // ─── Shared cancellation token & task tracker ───
    let cancel_token = CancellationToken::new();
    let mut join_set: JoinSet<()> = JoinSet::new();

    if tg_alerts_enabled {
        let bot = telegram::bot::TelegramBot::new(tg_notification_token.clone());
        let alert_service = telegram::alerts::AlertService::new(bot, tg_alerts_chat.clone(), alert_rx);
        join_set.spawn(alert_service.run());
        info!("Telegram alerts enabled (MERCURY_NOTIFICATION bot)");
    } else {
        warn!("Telegram alerts disabled — set TELEGRAM_NOTIFICATION_TOKEN and TELEGRAM_ALERTS_CHAT_ID");
        drop(alert_rx);
    }

    if tg_reports_enabled {
        let bot = telegram::bot::TelegramBot::new(tg_daily_token.clone());
        // Pass the command transmitter so the bot can route commands to the core
        let report_service = telegram::reports::ReportService::new(bot, tg_report_chat.clone(), daily_report_rx, cmd_tx, db.clone());
        join_set.spawn(report_service.run());
        info!("Telegram daily reports & command polling enabled (MERCURY_DAILYBOT)");
    } else {
        warn!("Telegram reports disabled — set TELEGRAM_DAILY_TOKEN and TELEGRAM_REPORT_CHAT_ID");
        drop(daily_report_rx);
    }

    // ─── Risk Manager ───
    let initial_bankroll = mercury_config.trading.initial_bankroll;
    let mut bankroll_manager = risk::bankroll::BankrollManager::new(initial_bankroll);
    let mut kelly = risk::kelly::KellyCalculator::new(
        mercury_config.trading.kelly_fraction_multiplier,
        mercury_config.trading.max_single_trade_pct,
    );
    let mut circuit_breakers = risk::circuit_breaker::CircuitBreakers::new(
        mercury_config.trading.max_single_trade_pct,
        mercury_config.trading.max_daily_loss_pct,
        mercury_config.trading.max_drawdown_pct,
        mercury_config.trading.max_platform_exposure_pct,
        mercury_config.trading.gas_price_max_gwei,
        mercury_config.trading.stale_data_timeout_ms / 1000,
        mercury_config.trading.max_open_positions,
    );

    // M-2 FIX: Recover persistent Circuit Breaker state from recent DB trades
    if let Ok(recent_trades) = db.get_trades_since(chrono::Utc::now() - chrono::Duration::hours(1)).await {
        for trade in &recent_trades {
            circuit_breakers.record_execution(trade.status == crate::types::TradeStatus::Success);
        }
        tracing::info!("Recovered circuit breaker state from {} recent trades", recent_trades.len());
    }

    // ─── Engine ───
    let uob = Arc::new(tokio::sync::RwLock::new(engine::order_book::UnifiedOrderBook::new()));
    let mut spread_engine = engine::spread::NetSpreadEngine::new(
        mercury_config.trading.min_net_spread_threshold,
    );
    spread_engine.update_gas_price(Decimal::from(50));
    spread_engine.update_matic_price(dec!(0.50));

    let mut detector = engine::detector::ArbitrageDetector::new(
        mercury_config.trading.min_net_spread_threshold,
        Decimal::from(5),
        mercury_config.trading.stale_data_timeout_ms,
        mercury_config.trading.max_concurrent_arbs,
    );
    let mut registry = engine::market_registry::MarketRegistry::new();

    let initial_trade_count = db.get_trade_count().await.unwrap_or(0);

    if let Ok(cum_profit) = db.get_cumulative_profit().await {
        bankroll_manager.restore_state(cum_profit);
    }

    // ─── Initialize Platform Clients from Environment ───
    let polymarket_client = (|| -> Option<execution::polymarket_client::PolymarketClient> {
        let api_key = std::env::var("POLYMARKET_API_KEY").unwrap_or_else(|_| env_vars.get("POLYMARKET_API_KEY").cloned().unwrap_or_default());
        let api_secret = std::env::var("POLYMARKET_API_SECRET").unwrap_or_else(|_| env_vars.get("POLYMARKET_API_SECRET").cloned().unwrap_or_default());
        let api_passphrase = std::env::var("POLYMARKET_API_PASSPHRASE").unwrap_or_else(|_| env_vars.get("POLYMARKET_API_PASSPHRASE").cloned().unwrap_or_default());
        let wallet_key = std::env::var("POLYMARKET_WALLET_KEY").unwrap_or_else(|_| env_vars.get("POLYMARKET_WALLET_KEY").cloned().unwrap_or_default());
        
        // LOW-3: Fail explicitly if credentials exist but are empty
        if api_key.is_empty() || api_secret.is_empty() || api_passphrase.is_empty() || wallet_key.is_empty() {
            return None;
        }
        
        // Clean any accidental env injection
        std::env::remove_var("POLYMARKET_API_KEY");
        std::env::remove_var("POLYMARKET_API_SECRET");
        std::env::remove_var("POLYMARKET_API_PASSPHRASE");
        std::env::remove_var("POLYMARKET_WALLET_KEY");
        let chain_id: u64 = std::env::var("POLYMARKET_CHAIN_ID")
            .or_else(|_| env_vars.get("POLYMARKET_CHAIN_ID").cloned().ok_or(std::env::VarError::NotPresent))
            .unwrap_or_else(|_| "137".to_string())
            .parse()
            .unwrap_or(137);
        let signer = match crypto::eip712::PolymarketSigner::from_hex_default(&wallet_key, chain_id) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Failed to initialize Polymarket signer");
                return None;
            }
        };
        info!(address = %signer.address(), "Polymarket client initialized");
        Some(execution::polymarket_client::PolymarketClient::new(
            mercury_config.platforms.polymarket.rest_url.clone(),
            signer,
            api_key,
            api_secret,
            api_passphrase,
        ))
    })();

    let kalshi_client = (|| -> Option<execution::kalshi_client::KalshiClient> {
        let api_key_id = std::env::var("KALSHI_API_KEY_ID").unwrap_or_else(|_| env_vars.get("KALSHI_API_KEY_ID").cloned().unwrap_or_default());
        let rsa_pem_path = std::env::var("KALSHI_RSA_PEM_PATH").unwrap_or_else(|_| env_vars.get("KALSHI_RSA_PEM_PATH").cloned().unwrap_or_default());
        if api_key_id.is_empty() || rsa_pem_path.is_empty() {
            return None;
        }
        let pem_bytes = match std::fs::read(&rsa_pem_path) {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, path = %rsa_pem_path, "Failed to read Kalshi RSA PEM");
                return None;
            }
        };
        let auth = match crypto::jwt::KalshiAuth::new(api_key_id, &pem_bytes) {
            Ok(a) => a,
            Err(e) => {
                warn!(error = %e, "Failed to initialize Kalshi auth");
                return None;
            }
        };
        info!("Kalshi client initialized");
        Some(execution::kalshi_client::KalshiClient::new(
            mercury_config.platforms.kalshi.rest_url.clone(),
            auth,
        ))
    })();

    let cdna_client = (|| -> Option<execution::cdna_client::CdnaClient> {
        if !mercury_config.platforms.cdna.enabled {
            return None;
        }
        let api_key = std::env::var("CDNA_API_KEY").ok().or_else(|| env_vars.get("CDNA_API_KEY").cloned())?;
        let api_secret = std::env::var("CDNA_API_SECRET").ok().or_else(|| env_vars.get("CDNA_API_SECRET").cloned())?;
        info!("CDNA client initialized");
        Some(execution::cdna_client::CdnaClient::new(
            mercury_config.platforms.cdna.rest_url.clone(),
            api_key,
            api_secret,
        ))
    })();

    let forecastex_client = if mercury_config.platforms.forecastex.enabled {
        Some(execution::forecastex_client::ForecastExClient::new(
            mercury_config.platforms.forecastex.fix_host.clone(),
            mercury_config.platforms.forecastex.fix_port,
        ))
    } else {
        None
    };

    if polymarket_client.is_some() {
        info!("Polymarket execution: ENABLED");
    } else {
        warn!("Polymarket execution: DISABLED (set POLYMARKET_API_KEY, POLYMARKET_API_SECRET, POLYMARKET_API_PASSPHRASE, POLYMARKET_WALLET_KEY)");
    }
    if kalshi_client.is_some() {
        info!("Kalshi execution: ENABLED");
    } else {
        warn!("Kalshi execution: DISABLED (set KALSHI_API_KEY_ID, KALSHI_RSA_PEM_PATH)");
    }

    // CRITICAL FIX: Clone the client before moving it into the executor
    let kalshi_for_settlement = kalshi_client.clone();
    let kalshi_for_reconciler = kalshi_client.clone();
    let polymarket_for_reconciler = polymarket_client.clone();
    let poly_for_unwind = polymarket_client.clone();
    let kalshi_for_unwind = kalshi_client.clone();
    let cdna_for_unwind = cdna_client.clone();
    let forex_for_unwind = forecastex_client.clone();

    let executor = execution::executor::ExecutionEngine::new(
        opportunity_rx,
        trade_result_tx.clone(),
        alert_tx.clone(),
        db.clone(),
        uob.clone(),
        polymarket_client,
        kalshi_client,
        cdna_client,
        forecastex_client,
        initial_trade_count,
    );
    join_set.spawn(executor.run());

    // ─── Position Tracker ───
    let position_tracker = inventory::positions::PositionTracker::new(db.clone(), trade_result_rx2);
    join_set.spawn(position_tracker.run());

    // ─── Reconciler ───
    // M-4 FIX: Increased reconciler threshold from $0.10 to $5.00 to prevent false positive fee alerts
    let reconciler = inventory::reconciler::Reconciler::new(
        db.clone(), alert_tx.clone(), kalshi_for_reconciler, polymarket_for_reconciler, 60, dec!(5.00),
    );
    join_set.spawn(reconciler.run());

    // ─── Settlement Monitor ───
    let settlement = inventory::settlement::SettlementMonitor::new(
        db.clone(), alert_tx.clone(), settlement_tx, 60, kalshi_for_settlement // HIGH-2 FIX: Reduced from 300s to 60s
    );
    join_set.spawn(settlement.run());

   // ─── Unwind Watchdog ───
    let unwind_watchdog = inventory::unwind_watchdog::UnwindWatchdog::new(
        db.clone(), alert_tx.clone(), 30,
        poly_for_unwind,
        kalshi_for_unwind,
        cdna_for_unwind,
        forex_for_unwind,
    );
    join_set.spawn(unwind_watchdog.run());

    // ─── Health Server ───
    let health_metrics = metrics.clone();
    join_set.spawn(monitoring::health::run_health_server(
        mercury_config.health.port,
        health_metrics,
        mercury_config.trading.stale_data_timeout_ms,
    ));

    // ─── Gas Oracle ───
    let gas_oracle = monitoring::gas_oracle::GasOracle::new(
        mercury_config.polygon_rpc.url.clone(),
        mercury_config.polygon_rpc.gas_poll_interval_secs,
        gas_update_tx,
    );
    join_set.spawn(gas_oracle.run());

    // ─── DB Backup (every 6 hours) ───
    // MED-1 FIX: Derive the backup directory directly from the configured database path
    let db_path = std::path::Path::new(&mercury_config.database.path);
    let backup_dir = db_path.parent().unwrap_or(std::path::Path::new(".")).join("backups");
    let backup_task = monitoring::backup::BackupTask::new(
        db.clone(),
        backup_dir.to_string_lossy().to_string(),
        6 * 3600,
    );
    join_set.spawn(backup_task.run());

    // ─── Config Hot-Reload Watcher ───
    let (config_reload_tx, mut config_reload_rx) = mpsc::channel::<MercuryConfig>(5);
    {
        let config_path = cli.config.clone();
        let cancel_reload = cancel_token.clone();
        join_set.spawn(async move {
            use notify::{Watcher, RecursiveMode, Event, EventKind};
            let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
            let mut watcher = match notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        let _ = tx.try_send(());
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    warn!(error = %e, "Failed to create config file watcher");
                    return;
                }
            };
            if let Err(e) = watcher.watch(std::path::Path::new(&config_path), RecursiveMode::NonRecursive) {
                warn!(error = %e, "Failed to watch config file");
                return;
            }
            info!("Config hot-reload watcher active on {}", config_path);
            let _kept_alive = watcher; // MED-2: Keep the watcher alive so the channel actually fires
            loop {
                tokio::select! {
                    _ = cancel_reload.cancelled() => break,
                    Some(()) = rx.recv() => {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        match MercuryConfig::load(&config_path) {
                            Ok(new_config) => {
                                // MED-1: Dynamic hot-reloading without systemd downtime.
                                tracing::info!("Config file updated. Applying new risk limits dynamically.");
                                let _ = config_reload_tx.try_send(new_config);
                            }
                            Err(e) => {
                                warn!(error = %e, "Config reload failed — keeping current config");
                            }
                        }
                    }
                }
            }
        });
    }

    // ─── Market Discovery ───
    let (matched_market_tx, mut matched_market_rx) = mpsc::channel::<feeds::discovery::MatchedMarket>(2000);
    
    let kalshi_auth_for_discovery = std::env::var("KALSHI_API_KEY_ID").ok().and_then(|key_id| {
        let pem_path = std::env::var("KALSHI_RSA_PEM_PATH").ok()?;
        let pem = std::fs::read(&pem_path).ok()?;
        crypto::jwt::KalshiAuth::new(key_id, &pem).ok()
    });
    
    let discovery = feeds::discovery::MarketDiscovery::new(
        mercury_config.platforms.clone(),
        60, // CRITICAL FIX: Poll every 60s to immediately catch new 15-minute crypto candles
        kalshi_auth_for_discovery,
    );
    join_set.spawn(discovery.run(matched_market_tx));

    // Load active markets from DB to seed feed handlers with initial subscriptions
    let active_markets = db.get_active_markets().await.unwrap_or_default();
    let mut pm_subs = Vec::new();
    let mut kalshi_subs = Vec::new();
    let mut cdna_subs = Vec::new();
    for m in active_markets {
        if let Some(info) = m.platforms.get(&Platform::Polymarket) {
            pm_subs.push((info.platform_market_id.clone(), m.unified_id, info.fee_rate_bps));
        }
        if let Some(info) = m.platforms.get(&Platform::Kalshi) {
            kalshi_subs.push((info.platform_market_id.clone(), m.unified_id));
        }
        if let Some(info) = m.platforms.get(&Platform::Cdna) {
            cdna_subs.push((info.platform_market_id.clone(), m.unified_id, info.fee_rate_bps));
        }
    }

    // ─── Feed Handlers ───
    if mercury_config.platforms.polymarket.enabled {
        let pm_feed = feeds::polymarket::PolymarketFeed::new(
            mercury_config.platforms.polymarket.clone(),
            db.clone(), // CRITICAL FIX: Pass the DB connection to the feed
            pm_subs,
        );
        let pm_tick_tx = tick_tx.clone();
        let pm_alert_tx = alert_tx.clone();
        let pm_cancel = cancel_token.clone();
        let pm_metrics = metrics.clone();
        join_set.spawn(feeds::base::run_with_reconnect(
            Box::new(pm_feed),
            pm_tick_tx,
            pm_alert_tx,
            pm_metrics,
            pm_cancel,
        ));
    }

    if mercury_config.platforms.kalshi.enabled {
        let kalshi_auth_for_feed = std::env::var("KALSHI_API_KEY_ID").ok().and_then(|key_id| {
            let pem_path = std::env::var("KALSHI_RSA_PEM_PATH").ok()?;
            let pem = std::fs::read(&pem_path).ok()?;
            crypto::jwt::KalshiAuth::new(key_id, &pem).ok()
        });
        let k_feed = feeds::kalshi::KalshiFeed::new(
            mercury_config.platforms.kalshi.clone(),
            kalshi_auth_for_feed,
            db.clone(), // CRITICAL FIX: Pass the DB connection so Kalshi can dynamically poll for updates
            kalshi_subs, 
        );
        let k_tick_tx = tick_tx.clone();
        let k_alert_tx = alert_tx.clone();
        let k_cancel = cancel_token.clone();
        let k_metrics = metrics.clone();
        join_set.spawn(feeds::base::run_with_reconnect(
            Box::new(k_feed),
            k_tick_tx,
            k_alert_tx,
            k_metrics,
            k_cancel,
        ));
    }

    if mercury_config.platforms.cdna.enabled {
        let c_feed = feeds::cdna::CdnaFeed::new(
            mercury_config.platforms.cdna.clone(),
            cdna_subs,
        );
        let c_tick_tx = tick_tx.clone();
        let c_alert_tx = alert_tx.clone();
        let c_cancel = cancel_token.clone();
        let c_metrics = metrics.clone();
        join_set.spawn(feeds::base::run_with_reconnect(
            Box::new(c_feed),
            c_tick_tx,
            c_alert_tx,
            c_metrics,
            c_cancel,
        ));
    }

    if mercury_config.platforms.forecastex.enabled {
        let f_feed = feeds::forecastex::ForecastExFeed::new(
            mercury_config.platforms.forecastex.clone(),
            vec![],
        );
        let f_tick_tx = tick_tx.clone();
        let f_alert_tx = alert_tx.clone();
        let f_cancel = cancel_token.clone();
        let f_metrics = metrics.clone();
        join_set.spawn(feeds::base::run_with_reconnect(
            Box::new(f_feed),
            f_tick_tx,
            f_alert_tx,
            f_metrics,
            f_cancel,
        ));
    }

    info!("All subsystems initialized. MERCURY engine running. Press Ctrl+C to shutdown.");

    // ─── Main Event Loop ───
    let mut tick_rx = tick_tx.subscribe();
    let mut trade_result_rx = trade_result_rx;

    use chrono::Timelike;
    let today = chrono::Utc::now().date_naive();
    let report_sent_today = db.get_daily_snapshot(today).await.ok().flatten().map(|s| s.report_sent).unwrap_or(false);
    let now_hour = chrono::Utc::now().hour();
    let target_hour = mercury_config.telegram.daily_report_hour_utc;
    
    // MED-2: If we missed the report hour today, schedule it to run in 5 seconds
    let delay = if !report_sent_today && now_hour >= target_hour {
        5
    } else {
        seconds_until_report_hour(target_hour)
    };

    info!(delay_secs = delay, hour_utc = target_hour, "Daily report scheduled");
    let mut next_report_time = tokio::time::Instant::now() + std::time::Duration::from_secs(delay);
    let mut report_sleep = Box::pin(tokio::time::sleep_until(next_report_time));

    // ADDED: Sync interval to prevent state drift
    let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));

    let mut cached_open_positions: usize = db.get_open_arb_count().await
        .unwrap_or_else(|e| {
            warn!("Could not read open arb count from DB on startup: {e}");
            0
        });

    let mut in_flight_notional = Decimal::ZERO;
    let mut in_flight_trades: usize = 0;
    
    // Explicitly enforce which brokers are allowed. CDNA and ForecastEx disabled per requirements.
    let mut active_platforms = std::collections::HashSet::new();
    active_platforms.insert(Platform::Polymarket);
    active_platforms.insert(Platform::Kalshi);

    let mut active_config_str = serde_json::to_string(&mercury_config).unwrap_or_default();

loop {
        tokio::select! {
            // ── Telegram Control Commands ──
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    SystemCommand::StartTrading => circuit_breakers.manual_halt(false),
                    SystemCommand::StopTrading => circuit_breakers.manual_halt(true),
                    SystemCommand::EnablePlatform(p) => { active_platforms.insert(p); },
                    SystemCommand::DisablePlatform(p) => { active_platforms.remove(&p); },
                    SystemCommand::RequestDailyReport => {
                        let snapshot = bankroll_manager.daily_snapshot(kelly.fraction());
                        let uptime_secs = metrics.uptime_secs();
                        let ws_reconnects = metrics.ws_reconnects.load(std::sync::atomic::Ordering::Relaxed);
                        let api_errors = metrics.api_errors.load(std::sync::atomic::Ordering::Relaxed);
                        let db_clone = db.clone();
                        let report_tx_clone = daily_report_tx.clone();
                        let reports_enabled = tg_reports_enabled;

                        tokio::spawn(async move {
                            let report = build_daily_report(db_clone, snapshot, uptime_secs, ws_reconnects, api_errors).await;
                            if reports_enabled {
                                let _ = report_tx_clone.try_send(report);
                            }
                        });
                    }
                    SystemCommand::ActivateMarket(id) => {
                        let db_clone = db.clone();
                        tokio::spawn(async move {
                            if let Err(e) = db_clone.update_market_status(&id, MarketStatus::Active).await {
                                tracing::error!("Failed to activate market {}: {}", id, e);
                            }
                        });
                        if let Ok(Some(mut market)) = db.get_market(&id).await {
                            market.status = MarketStatus::Active;
                            registry.register_market(market);
                            info!("Market {} manually activated via Telegram", id);
                        }
                    }
                }
            }

            tick_result = tick_rx.recv() => {
                let tick = match tick_result {
                    Ok(t) => t,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("HFT Engine lagging! Missed {} feed ticks. Books may be stale.", n);
                        // H-4 FIX: DO NOT clear all books globally. 
                        // The stale feed detector (CB9) and individual tick sequence numbers 
                        // will natively handle staleness and overwrite with fresh data.
                        let _ = alert_tx.try_send(AlertMessage::SystemAlert {
                            severity: "warning".into(),
                            message: format!("HFT Engine lagging! Missed {} ticks. Cooling down detection.", n),
                        });
                        // Cooldown detection for 5 seconds to prevent single-sided executions
                        detector.pause_detection_until(crate::types::now_ns() + 5_000_000_000);
                        continue;
                    }
                    Err(_) => break, // Channel closed
                };

                // O(1) filter blocks deactivated brokers with zero latency overhead
                if !active_platforms.contains(&tick.platform) {
                    continue;
                }

                metrics.inc_ticks();
                uob.write().await.update(&tick);

                if circuit_breakers.is_trading_halted() {
                    continue;
                }

                detector.set_active_arbs(cached_open_positions);

                // Replaced O(N) detect with O(1) detect_for_market
                let uob_guard = uob.read().await;
                let opps = detector.detect_for_market(
                    &tick.market_id,
                    &registry,
                    &uob_guard,
                    &spread_engine,
                    rust_decimal_macros::dec!(10.0), // fallback target size
                );
                drop(uob_guard);
                metrics.inc_spreads(); 

                // Process opportunities
                for opp in &opps {
                    metrics.inc_detected();
                }
                for opp in opps {
                // Fix CB5: Calculate the true exposure allocated to this specific market question
                let market_exposure_pct = bankroll_manager.market_exposure_pct(&opp.market_id);

                let trips = circuit_breakers.check_all(
                        opp.recommended_size, 
                        bankroll_manager.total_bankroll(), 
                        bankroll_manager.daily_loss_pct(), 
                        bankroll_manager.drawdown_pct(), 
                        bankroll_manager.platform_exposure_pct(&opp.leg_a.platform).max(bankroll_manager.platform_exposure_pct(&opp.leg_b.platform)), 
                        cached_open_positions, 
                        opp.leg_a.platform == Platform::Polymarket || opp.leg_b.platform == Platform::Polymarket,
                        metrics.ms_since_last_tick(), 
                        market_exposure_pct // Authoritative market correlation tracking
                    );

                    if trips.is_empty() {
                        let win_prob = bankroll_manager.exec_success_rate().max(rust_decimal_macros::dec!(0.5));
                        let kelly_frac = kelly.optimal_fraction(win_prob, opp.net_spread);
                        
                        let kelly_ideal_usd = kelly.position_size(
                            bankroll_manager.total_bankroll(), 
                            win_prob, 
                            opp.net_spread, 
                            mercury_config.trading.max_single_trade_pct
                        );
                        
                        // FIX: Dimensional Analysis Bug.
                        // We must convert the dollar budget into contracts by dividing by the combined price of both legs.
                        let combined_contract_price = opp.leg_a.price + opp.leg_b.price;
                        let kelly_ideal_contracts = if combined_contract_price > rust_decimal::Decimal::ZERO {
                            kelly_ideal_usd / combined_contract_price
                        } else {
                            rust_decimal::Decimal::ZERO
                        };

                        // Fix Sizing: Kalshi demands whole integers, but Polymarket supports decimals.
                        let approved_size = if opp.leg_a.platform == Platform::Kalshi || opp.leg_b.platform == Platform::Kalshi {
                            kelly_ideal_contracts.min(opp.recommended_size).floor()
                        } else {
                            kelly_ideal_contracts.min(opp.recommended_size).round_dp(2)
                        };
                        
                        // CRITICAL FIX: Targeted Minimum Notional Guard.
                        // Polymarket strictly rejects orders < $5.00, but Kalshi's minimum is just 1 contract.
                        // We must only check the $5.00 limit against the Polymarket leg. Applying it to Kalshi
                        // will erroneously reject highly profitable arbs where the Kalshi leg is cheap (e.g. $2.00).
                        let mut pm_size_too_small = false;
                        if matches!(opp.leg_a.platform, Platform::Polymarket | Platform::PolymarketUs) && (approved_size * opp.leg_a.price) < rust_decimal_macros::dec!(5.0) {
                            pm_size_too_small = true;
                        }
                        if matches!(opp.leg_b.platform, Platform::Polymarket | Platform::PolymarketUs) && (approved_size * opp.leg_b.price) < rust_decimal_macros::dec!(5.0) {
                            pm_size_too_small = true;
                        }
                        
                        if pm_size_too_small {
                            tracing::debug!("Opportunity rejected: Size too small to meet Polymarket $5.00 minimum");
                            continue;
                        }
                        
                        if approved_size > Decimal::ZERO {
                            // FIX: Prevent over-allocation during rapid successes by subtracting active exposure from the bankroll check.
                            let effective_bankroll = (bankroll_manager.total_bankroll() - in_flight_notional - bankroll_manager.total_exposure()).max(Decimal::ZERO);
                            if effective_bankroll >= approved_size {
                                // FIX: Use exact nominal pricing for exposure rather than assuming a 50/50 split.
                                let leg_a_exposure = approved_size * opp.leg_a.price;
                                let leg_b_exposure = approved_size * opp.leg_b.price;
                                
                                // Extract the Enums and UUIDs BEFORE moving the opportunity into the channel (Borrow Checker Fix)
                                let platform_a = opp.leg_a.platform;
                                let platform_b = opp.leg_b.platform;
                                let opp_market_id = opp.market_id; 
                                
                                let validated = ValidatedOpportunity { opportunity: opp, approved_size, risk_score: kelly_frac };
                                match opportunity_tx.try_send(validated) {
                                    Ok(_) => {
                                        metrics.inc_executed();
                                        cached_open_positions = cached_open_positions.saturating_add(1);
                                        in_flight_notional += approved_size;
                                        in_flight_trades += 1;
                                        bankroll_manager.add_exposure(platform_a, leg_a_exposure);
                                        bankroll_manager.add_exposure(platform_b, leg_b_exposure);
                                        
                                        // Track correlated market exposure for CB5.
                                        bankroll_manager.add_market_exposure(opp_market_id, approved_size);
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Execution channel full, dropping opportunity to maintain latency");
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Market Discovery Results ──
            Some(matched) = matched_market_rx.recv() => {
                let market_id = matched.market.unified_id;
                if registry.get_market(&market_id).is_none() {
                    info!(
                        market_id = %market_id,
                        question = %matched.market.question,
                        platforms = matched.market.platforms.len(),
                        "New cross-platform market registered"
                    );
                    
                    // CRITICAL FIX: Spawn DB write to a background task.
                    // Awaiting SQLite I/O directly in the main select loop blocks 
                    // the tick processor, causing catastrophic latency spikes.
                    let db_clone = db.clone();
                    let market_clone = matched.market.clone();
                    tokio::spawn(async move {
                        if let Err(e) = db_clone.upsert_market(&market_clone).await {
                            tracing::error!(error = %e, "Failed to persist new market");
                        }
                    });
                    
                    registry.register_market(matched.market);
                }
            }

            // ── Config Hot Reload ──
            Some(new_config) = config_reload_rx.recv() => {
                let new_config_str = serde_json::to_string(&new_config).unwrap_or_default();
                let _ = db.log_config_change("hot_reload", &active_config_str, &new_config_str).await;
                active_config_str = new_config_str;
                
                spread_engine.update_threshold(new_config.trading.min_net_spread_threshold);
                detector.update_thresholds(new_config.trading.min_net_spread_threshold, new_config.trading.stale_data_timeout_ms, new_config.trading.max_concurrent_arbs);
                circuit_breakers.update_limits(
                    new_config.trading.max_single_trade_pct,
                    new_config.trading.max_daily_loss_pct,
                    new_config.trading.max_drawdown_pct,
                    new_config.trading.max_platform_exposure_pct,
                    new_config.trading.gas_price_max_gwei,
                    new_config.trading.stale_data_timeout_ms / 1000,
                    new_config.trading.max_open_positions,
                );
                kelly.update_fraction(new_config.trading.kelly_fraction_multiplier, new_config.trading.max_single_trade_pct);
                
                let _ = alert_tx.try_send(AlertMessage::SystemAlert {
                    severity: "info".into(),
                    message: "⚙️ Configuration hot-reloaded successfully. Risk limits updated.".into(),
                });
            }

            // ── Trade Results ──
            Some(mut result) = trade_result_rx.recv() => {
                bankroll_manager.record_trade(&result);
                
                // CRITICAL FIX: Populate accurate bankroll data BEFORE DB insert & alerting
                let pre_trade = bankroll_manager.total_bankroll() - result.profit;
                result.bankroll_after = bankroll_manager.total_bankroll();
                result.bankroll_change_pct = if pre_trade > rust_decimal::Decimal::ZERO {
                    (result.profit / pre_trade) * rust_decimal_macros::dec!(100.0)
                } else {
                    rust_decimal::Decimal::ZERO
                };

                // NON-BLOCKING SQL DB INSERT
                let db_clone = db.clone();
                let res_for_db = result.clone();
                tokio::spawn(async move {
                    if let Err(e) = db_clone.insert_trade(&res_for_db).await {
                        tracing::error!(error = %e, "Failed to persist trade result to database");
                    }
                });

                // TELEGRAM ALERT DISPATCH
                if tg_alerts_enabled {
                    let _ = alert_tx.try_send(AlertMessage::TradeComplete(result.clone()));
                }

                // CRITICAL FIX (1-B): Only release the exposure if the trade FAILED or was PARTIAL.
                // Successful trades keep their capital locked on the platform until settlement.
                if result.status != TradeStatus::Success {
                    let leg_a_exposure = result.approved_size * result.leg_a_price;
                    let leg_b_exposure = result.approved_size * result.leg_b_price;
                    bankroll_manager.remove_exposure(result.leg_a_platform, leg_a_exposure);
                    bankroll_manager.remove_exposure(result.leg_b_platform, leg_b_exposure);
                    bankroll_manager.remove_market_exposure(result.market_id, result.approved_size);
                } else {
                    // Fix: True-up exposure for Successful trades to match exact fill cost, preventing progressive drift
                    let reserved_a = result.approved_size * result.leg_a_price;
                    let actual_a = result.leg_a_size * result.leg_a_fill_price;
                    if reserved_a > actual_a {
                        bankroll_manager.remove_exposure(result.leg_a_platform, reserved_a - actual_a);
                    } else if actual_a > reserved_a {
                        bankroll_manager.add_exposure(result.leg_a_platform, actual_a - reserved_a);
                    }

                    let reserved_b = result.approved_size * result.leg_b_price;
                    let actual_b = result.leg_b_size * result.leg_b_fill_price;
                    if reserved_b > actual_b {
                        bankroll_manager.remove_exposure(result.leg_b_platform, reserved_b - actual_b);
                    } else if actual_b > reserved_b {
                        bankroll_manager.add_exposure(result.leg_b_platform, actual_b - reserved_b);
                    }
                }
                
                circuit_breakers.record_execution(result.status == TradeStatus::Success);
                kelly.adjust_for_drawdown(bankroll_manager.drawdown_pct());

                // CRITICAL FIX: rust_decimal does not implement saturating_sub. 
                // Manual clamp prevents arithmetic panics and compilation errors.
                in_flight_notional = (in_flight_notional - result.approved_size).max(Decimal::ZERO);
                in_flight_trades = in_flight_trades.saturating_sub(1);
                if result.status == TradeStatus::Fail {
                    cached_open_positions = cached_open_positions.saturating_sub(1);
                }

                // CRITICAL FIX: Use try_send. If SQLite I/O lags, the position tracker blocks.
                // Awaiting here would halt the main tick-processing event loop.
                if let Err(e) = trade_result_tx2.try_send(result.clone()) {
                    error!(error = %e, trade_id = result.trade_id,
                        "CRITICAL: Position tracker channel full/closed — trade result lost, \
                         open position will not be closed in DB. Manual intervention required.");
                }

                match result.status {
                    TradeStatus::Success => metrics.inc_success(),
                    _ => metrics.inc_failed(),
                }
            }

            // ── Gas Oracle Updates ──
            Some(gas) = gas_update_rx.recv() => {
                spread_engine.update_gas_price(Decimal::from(gas.gas_gwei));
                spread_engine.update_matic_price(gas.matic_usd);
                circuit_breakers.update_gas_price(gas.gas_gwei);
                debug!(gwei = gas.gas_gwei, matic_usd = %gas.matic_usd, "Gas parameters updated");
            }

            // ── Settlement PnL Sink ──
            Some(settlement) = settlement_rx.recv() => {
                bankroll_manager.record_settlement(settlement.realized_pnl);
                // Free the exposure that was locked during the trade lifecycle
                let removed_exposure = settlement.quantity * settlement.avg_entry_price;
                bankroll_manager.remove_exposure(settlement.platform, removed_exposure);
                // CRIT-4 FIX: Market exposure was reserved using the FULL approved_size. 
                // Because we enforce FOK, settlement.quantity is equivalent to the approved_size lock.
                bankroll_manager.remove_market_exposure(settlement.market_id, settlement.quantity);
                
                // CRITICAL FIX (2-B): Free up the position capacity immediately so the 
                // engine doesn't artificially halt trading waiting for the 60s DB sync.
                cached_open_positions = cached_open_positions.saturating_sub(1);
            }

            // ── Periodic State Sync ──
            _ = sync_interval.tick() => {
                if let Ok(count) = db.get_open_arb_count().await {
                    // DB count includes all persisted open positions.
                    // in_flight_trades are dispatched but not yet persisted.
                    // Don't double-count: only add truly in-flight (not yet DB-persisted) trades.
                    cached_open_positions = count.max(cached_open_positions.saturating_sub(in_flight_trades)) + in_flight_trades;
                }
                
                // L-5 FIX: Expose DetectorStats to the logs
                tracing::info!(
                    detected = detector.stats.opportunities_detected,
                    passed = detector.stats.opportunities_passed,
                    gate1_spread = detector.stats.gate1_rejected,
                    gate2_liquidity = detector.stats.gate2_rejected,
                    gate3_stale = detector.stats.gate3_rejected,
                    gate4_correlation = detector.stats.gate4_rejected,
                    gate5_capacity = detector.stats.gate5_rejected,
                    "Detector pipeline statistics"
                );
            }

            // ── Daily Report ──
            _ = &mut report_sleep => {
                next_report_time = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds_until_report_hour(target_hour));
                report_sleep.as_mut().reset(next_report_time);
                
                let snapshot = bankroll_manager.daily_snapshot(kelly.fraction());
                let uptime_secs = metrics.uptime_secs();
                let ws_reconnects = metrics.ws_reconnects.load(Ordering::Relaxed);
                let api_errors = metrics.api_errors.load(Ordering::Relaxed);
                
                let db_clone = db.clone();
                let report_tx_clone = daily_report_tx.clone();
                let reports_enabled = tg_reports_enabled;

                tokio::spawn(async move {
                    let report = build_daily_report(db_clone.clone(), snapshot, uptime_secs, ws_reconnects, api_errors).await;
                    let _ = db_clone.insert_daily_snapshot(&report.snapshot).await;

                    if reports_enabled {
                        let _ = report_tx_clone.try_send(report);
                    }
                });

                bankroll_manager.reset_daily();
            }
            

            // ── Background Task Monitor ──
            Some(task_result) = join_set.join_next(), if !join_set.is_empty() => {
                let msg = match task_result {
                    Ok(()) => "A critical background task exited cleanly but unexpectedly. Shutting down to prevent unhedged exposure.",
                    Err(e) => {
                        tracing::error!(error = %e, "A critical background task panicked!");
                        "A critical background task panicked! Shutting down."
                    }
                };
                
                let _ = alert_tx.try_send(AlertMessage::SystemAlert {
                    severity: "critical".into(),
                    message: msg.into(),
                });
                
                // MED-7 FIX: Treat executor or other core task failure as a critical event that halts the engine
                tracing::error!("CRITICAL: Background task failure. Initiating emergency shutdown.");
                cancel_token.cancel();
                break;
            }

            // ── Graceful Shutdown ──
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                cancel_token.cancel();

                if tg_alerts_enabled {
                    let bot = telegram::bot::TelegramBot::new(tg_notification_token.clone());
                    let _ = bot.send_message(
                        &tg_alerts_chat,
                        "MERCURY SHUTTING DOWN - Graceful shutdown initiated.",
                    ).await;
                }

                info!("MERCURY shutdown complete");
                break;
            }
        }
    }

    // Drain in-flight trades before killing subsystems.
    if cached_open_positions > 0 {
        info!(positions = cached_open_positions, "Draining in-flight positions (up to 30s)");
        let drain_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        while cached_open_positions > 0 {
            let remaining = drain_deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                warn!(
                    positions = cached_open_positions,
                    "Shutdown drain timeout — {} position(s) may remain open",
                    cached_open_positions
                );
                break;
            }
            match tokio::time::timeout(remaining, trade_result_rx.recv()).await {
                Ok(Some(result)) => {
                    bankroll_manager.record_trade(&result);
                    
                    // Fix: Use the originally reserved size and accurate price
                    let leg_a_exposure = result.approved_size * result.leg_a_price;
                    let leg_b_exposure = result.approved_size * result.leg_b_price;
                    bankroll_manager.remove_exposure(result.leg_a_platform, leg_a_exposure);
                    bankroll_manager.remove_exposure(result.leg_b_platform, leg_b_exposure);
                    bankroll_manager.remove_market_exposure(result.market_id, result.approved_size);
                    
                    // Fix: Decrement open positions universally during the drain
                    cached_open_positions = cached_open_positions.saturating_sub(1); 
                    
                    if let Err(e) = trade_result_tx2.try_send(result) {
                        error!(error = %e, "Position tracker channel full during drain");
                    }
                }
                _ => break,
            }
        }
    }

    // Abort all remaining tasks and wait for them to finish.
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    Ok(())
}
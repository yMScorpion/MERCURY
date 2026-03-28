use anyhow::Result;
use clap::Parser;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mercury=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    info!("MERCURY v{} starting...", env!("CARGO_PKG_VERSION"));

    let mercury_config = MercuryConfig::load(&cli.config)?;
    info!("Configuration loaded");

    let db: Arc<dyn db::Database> = Arc::new(SqliteDb::new(
        &mercury_config.database.path,
        mercury_config.database.pool_size,
        mercury_config.database.busy_timeout_ms,
    )?);
    info!("Database initialized");

    let metrics = monitoring::metrics::Metrics::new();

    // ─── Channels ───
    let (tick_tx, _) = broadcast::channel::<NormalizedTick>(10_000);
    let (opportunity_tx, opportunity_rx) = mpsc::channel::<ValidatedOpportunity>(100);
    let (trade_result_tx, trade_result_rx) = mpsc::channel::<TradeResult>(100);
    let (trade_result_tx2, trade_result_rx2) = mpsc::channel::<TradeResult>(100);
    let (alert_tx, alert_rx) = mpsc::channel::<AlertMessage>(500);
    let (daily_report_tx, daily_report_rx) = mpsc::channel::<DailyReport>(10);

    // ─── Telegram ───
    // Two separate bots: MERCURY_NOTIFICATION (real-time alerts) and MERCURY_DAILYBOT (daily reports)
    let tg_notification_token = std::env::var("TELEGRAM_NOTIFICATION_TOKEN").unwrap_or_default();
    let tg_daily_token = std::env::var("TELEGRAM_DAILY_TOKEN").unwrap_or_default();
    let tg_alerts_chat = std::env::var("TELEGRAM_ALERTS_CHAT_ID").unwrap_or_default();
    let tg_report_chat = std::env::var("TELEGRAM_REPORT_CHAT_ID").unwrap_or_default();

    let tg_alerts_enabled = mercury_config.telegram.enabled
        && !tg_notification_token.is_empty()
        && !tg_alerts_chat.is_empty()
        && tg_alerts_chat != "YOUR_CHAT_ID_HERE";
    let tg_reports_enabled = mercury_config.telegram.enabled
        && !tg_daily_token.is_empty()
        && !tg_report_chat.is_empty()
        && tg_report_chat != "YOUR_CHAT_ID_HERE";
    let tg_enabled = tg_alerts_enabled || tg_reports_enabled;

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
        let report_service = telegram::reports::ReportService::new(bot, tg_report_chat.clone(), daily_report_rx);
        join_set.spawn(report_service.run());
        info!("Telegram daily reports enabled (MERCURY_DAILYBOT)");
    } else {
        warn!("Telegram reports disabled — set TELEGRAM_DAILY_TOKEN and TELEGRAM_REPORT_CHAT_ID");
        drop(daily_report_rx);
    }

    // ─── Risk Manager ───
    let initial_bankroll = mercury_config.trading.initial_bankroll;
    let mut bankroll_manager = risk::bankroll::BankrollManager::new(initial_bankroll);
    let mut kelly = risk::kelly::KellyCalculator::new(
        Decimal::from_str_exact(&mercury_config.trading.kelly_fraction_multiplier.to_string())
            .unwrap_or(dec!(0.25))
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

    // ─── Engine ───
    let mut uob = engine::order_book::UnifiedOrderBook::new();
    let mut spread_engine = engine::spread::NetSpreadEngine::new(
        mercury_config.trading.min_net_spread_threshold,
    );

    // Polymarket runs on Polygon — gas is paid in MATIC, not ETH.
    // Default MATIC price is ~$0.50, not $2000 (ETH).  Without this fix,
    // the hardcoded eth_price_usd=$2000 inflates gas cost to ~$20/trade
    // and makes every Polymarket arb appear unprofitable.
    {
        let gas_gwei = std::env::var("POLYGON_GAS_GWEI")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Decimal::from)
            .unwrap_or(Decimal::from(50));
        let matic_price = std::env::var("MATIC_PRICE_USD")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .and_then(|f| rust_decimal::Decimal::from_f64_retain(f))
            .unwrap_or(rust_decimal_macros::dec!(0.50));
        spread_engine.update_gas_price(gas_gwei);
        spread_engine.update_eth_price(matic_price);
        info!(gas_gwei = %gas_gwei, matic_usd = %matic_price, "Gas cost parameters set (Polygon/MATIC)");
    }
    let mut detector = engine::detector::ArbitrageDetector::new(
        mercury_config.trading.min_net_spread_threshold,
        Decimal::from(5),
        mercury_config.trading.stale_data_timeout_ms,
        mercury_config.trading.max_concurrent_arbs,
    );
    let mut registry = engine::market_registry::MarketRegistry::new();

    // ─── Execution Engine ───
    let executor = execution::executor::ExecutionEngine::new(
        opportunity_rx,
        trade_result_tx.clone(),
        alert_tx.clone(),
        db.clone(),
        None, None, None, None,
        initial_bankroll,
    );
    join_set.spawn(executor.run());

    // ─── Position Tracker ───
    let position_tracker = inventory::positions::PositionTracker::new(db.clone(), trade_result_rx2);
    join_set.spawn(position_tracker.run());

    // ─── Reconciler ───
    let reconciler = inventory::reconciler::Reconciler::new(
        db.clone(), alert_tx.clone(), 60, dec!(0.10),
    );
    join_set.spawn(reconciler.run());

    // ─── Settlement Monitor ───
    let settlement = inventory::settlement::SettlementMonitor::new(
        db.clone(), alert_tx.clone(), 300,
    );
    join_set.spawn(settlement.run());

    // ─── Health Server ───
    let health_metrics = metrics.clone();
    join_set.spawn(monitoring::health::run_health_server(mercury_config.health.port, health_metrics));

    info!("All subsystems initialized. MERCURY engine running. Press Ctrl+C to shutdown.");

    // ─── Main Event Loop ───
    let mut tick_rx = tick_tx.subscribe();
    let mut trade_result_rx = trade_result_rx;
    let daily_report_interval = tokio::time::interval(std::time::Duration::from_secs(86400));
    tokio::pin!(daily_report_interval);

    // Cache open-position count; updated on each trade result to avoid a DB
    // round-trip inside the hot tick-processing path.
    let mut cached_open_positions: usize = 0;

    loop {
        tokio::select! {
            tick_result = tick_rx.recv() => {
                let tick = match tick_result {
                    Ok(t) => t,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Broadcast channel lagged by {} ticks — consider increasing buffer", n);
                        metrics.ws_reconnects.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    Err(_) => break,
                };

                metrics.inc_ticks();
                uob.update(&tick);

                // Sync concurrency cap with the cached position count so the
                // detector's slot guard actually works (BUG-3 fix).
                detector.set_active_arbs(cached_open_positions);

                // detector.detect() calls compute_spreads internally; the extra
                // loop here was a 100 % duplicate that has been removed (CRITICAL-2).
                let opps = detector.detect(&registry, &uob, &spread_engine, Decimal::from(10));
                metrics.spreads_evaluated.fetch_add(opps.len() as u64, Ordering::Relaxed);

                for opp in opps {
                    metrics.inc_detected();

                    if circuit_breakers.is_trading_halted() {
                        continue;
                    }

                    let exec_prob = bankroll_manager.exec_success_rate();
                    let kelly_frac = kelly.optimal_fraction(exec_prob, opp.net_spread);
                    let approved_size = kelly.position_size(
                        bankroll_manager.total_bankroll(),
                        exec_prob,
                        opp.net_spread,
                        mercury_config.trading.max_single_trade_pct,
                    );

                    // Use the per-tick cached count — no DB round-trip per opportunity.
                    let involves_poly = matches!(opp.leg_a.platform, Platform::Polymarket | Platform::PolymarketUs)
                        || matches!(opp.leg_b.platform, Platform::Polymarket | Platform::PolymarketUs);

                    let trips = circuit_breakers.check_all(
                        approved_size,
                        bankroll_manager.total_bankroll(),
                        bankroll_manager.daily_loss_pct(),
                        bankroll_manager.drawdown_pct(),
                        bankroll_manager.platform_exposure_pct(&opp.leg_a.platform)
                            .max(bankroll_manager.platform_exposure_pct(&opp.leg_b.platform)),
                        cached_open_positions,
                        involves_poly,
                    );

                    if !trips.is_empty() {
                        if tg_enabled {
                            for trip in &trips {
                                // Non-blocking send: the alert channel has a 500-slot
                                // buffer; dropping one alert on overflow is acceptable.
                                let _ = alert_tx.try_send(AlertMessage::CircuitBreaker {
                                    breaker_type: trip.breaker_type.clone(),
                                    details: trip.details.clone(),
                                    action: trip.action.clone(),
                                    resume_at: trip.resume_at,
                                });
                            }
                        }
                        continue;
                    }

                    if approved_size > Decimal::ZERO {
                        let validated = ValidatedOpportunity {
                            opportunity: opp,
                            approved_size,
                            risk_score: kelly_frac,
                        };
                        // Non-blocking send: if the executor queue is full we skip
                        // this opportunity rather than blocking the event loop.
                        if opportunity_tx.try_send(validated).is_ok() {
                            metrics.inc_executed();
                            cached_open_positions = cached_open_positions.saturating_add(1);
                        }
                    }
                }
            }

            Some(result) = trade_result_rx.recv() => {
                bankroll_manager.record_trade(&result);
                circuit_breakers.record_execution(result.status == TradeStatus::Success);
                kelly.adjust_for_drawdown(bankroll_manager.drawdown_pct());
                // Decrement cached counter now that the position is settled.
                cached_open_positions = cached_open_positions.saturating_sub(1);
                if let Err(e) = trade_result_tx2.try_send(result.clone()) {
                    tracing::warn!("Position tracker channel full, dropping trade result: {e}");
                }
                match result.status {
                    TradeStatus::Success => metrics.inc_success(),
                    _ => metrics.inc_failed(),
                }
            }

            _ = daily_report_interval.tick() => {
                let snapshot = bankroll_manager.daily_snapshot(kelly.fraction());
                let trades = db.get_trades_for_date(chrono::Utc::now().date_naive()).await.unwrap_or_default();

                let mut platform_breakdown = HashMap::new();
                for trade in &trades {
                    let entry = platform_breakdown.entry(trade.leg_a_platform)
                        .or_insert_with(|| PlatformDayStats {
                            platform: trade.leg_a_platform,
                            exposure: Decimal::ZERO,
                            trade_count: 0,
                            pnl: Decimal::ZERO,
                        });
                    entry.trade_count += 1;
                    entry.pnl += trade.profit;
                }

                let mut sorted_trades = trades.clone();
                sorted_trades.sort_by(|a, b| b.profit.cmp(&a.profit));
                let top_trades: Vec<TradeResult> = sorted_trades.iter().take(3).cloned().collect();
                sorted_trades.sort_by(|a, b| a.profit.cmp(&b.profit));
                let worst_trades: Vec<TradeResult> = sorted_trades.iter().take(3).cloned().collect();

                let db_size = db.db_size_bytes().await.unwrap_or(0);
                let report = DailyReport {
                    snapshot,
                    platform_breakdown,
                    top_trades,
                    worst_trades,
                    uptime_secs: metrics.uptime_secs(),
                    ws_reconnects: metrics.ws_reconnects.load(Ordering::Relaxed),
                    api_errors: metrics.api_errors.load(Ordering::Relaxed),
                    db_size_bytes: db_size,
                };

                // Persist snapshot before resetting counters (BUG-13 fix).
                let _ = db.insert_daily_snapshot(&report.snapshot).await;

                if tg_reports_enabled {
                    let _ = daily_report_tx.try_send(report);
                }
                bankroll_manager.reset_daily();
            }

            // Warn if any background task exits unexpectedly during the main loop.
            Some(task_result) = join_set.join_next() => {
                match task_result {
                    Ok(()) => warn!("A background task exited cleanly but unexpectedly"),
                    Err(e) => warn!("A background task was cancelled or panicked: {e}"),
                }
            }

            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                cancel_token.cancel();
                if tg_alerts_enabled {
                    let bot = telegram::bot::TelegramBot::new(tg_notification_token.clone());
                    let _ = bot.send_message(&tg_alerts_chat, "MERCURY SHUTTING DOWN - Graceful shutdown initiated.").await;
                }
                info!("MERCURY shutdown complete");
                break;
            }
        }
    }

    // Abort all remaining tasks and wait for them to finish.
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}

    Ok(())
}

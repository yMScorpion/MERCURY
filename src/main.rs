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
use tracing::{debug, info, warn};

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
    // 1 000-slot buffer: each TradeResult is ~200 B; 200 KB headroom vs the
    // 100-slot original that could drop results under burst load, leaving
    // positions open in the DB and diverging cached_open_positions on restart.
    let (trade_result_tx2, trade_result_rx2) = mpsc::channel::<TradeResult>(1_000);
    let (alert_tx, alert_rx) = mpsc::channel::<AlertMessage>(500);
    let (daily_report_tx, daily_report_rx) = mpsc::channel::<DailyReport>(10);
    let (gas_update_tx, mut gas_update_rx) = mpsc::channel::<monitoring::gas_oracle::GasUpdate>(16);

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
    // Set safe startup defaults; the GasOracle will overwrite these on its first
    // fetch (which fires immediately — see gas_oracle.rs).
    spread_engine.update_gas_price(Decimal::from(50));
    spread_engine.update_matic_price(dec!(0.50));
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

    // ─── Gas Oracle ───
    // Polls live Polygon gas price (eth_gasPrice) and MATIC/USD spot price.
    // Updates are delivered via gas_update_rx into the main event loop so
    // spread_engine and circuit_breakers always reflect current network costs.
    let gas_oracle = monitoring::gas_oracle::GasOracle::new(
        mercury_config.polygon_rpc.url.clone(),
        mercury_config.polygon_rpc.gas_poll_interval_secs,
        gas_update_tx,
    );
    join_set.spawn(gas_oracle.run());

    info!("All subsystems initialized. MERCURY engine running. Press Ctrl+C to shutdown.");

    // ─── Main Event Loop ───
    let mut tick_rx = tick_tx.subscribe();
    let mut trade_result_rx = trade_result_rx;
    let daily_report_interval = tokio::time::interval(std::time::Duration::from_secs(86400));
    tokio::pin!(daily_report_interval);

    // Seed the open-position counter from the DB so that after a crash/restart
    // the concurrency limit is accurate from the first tick.
    let mut cached_open_positions: usize = db.get_open_positions().await
        .map(|v| v.len())
        .unwrap_or_else(|e| {
            warn!("Could not read open positions from DB on startup: {e}");
            0
        });

    // In-flight notional: capital reserved for opportunities that have been sent
    // to the executor but not yet settled. Deducted from available capital when
    // sizing new trades to prevent over-leveraging while orders are pending.
    let mut in_flight_notional = Decimal::ZERO;

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
                        // Reserve in-flight capital: reduce available bankroll by the
                        // size of this opportunity so concurrent rapid detections don't
                        // all size themselves against the full bankroll. Without this,
                        // max_open_positions trades can each claim max_single_trade_pct
                        // of the full bankroll, over-leveraging by a factor of N.
                        let effective_bankroll = (bankroll_manager.total_bankroll()
                            - in_flight_notional)
                            .max(Decimal::ZERO);
                        if effective_bankroll < approved_size {
                            // Not enough un-reserved capital — skip until in-flight trades settle.
                            continue;
                        }

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
                            in_flight_notional += approved_size;
                        }
                    }
                }
            }

            Some(result) = trade_result_rx.recv() => {
                bankroll_manager.record_trade(&result);
                // Release platform exposure for both legs now that the trade is
                // settled; without this, failed-trade exposure accumulates and
                // the platform exposure circuit breaker trips prematurely.
                bankroll_manager.remove_exposure(result.leg_a_platform, result.leg_a_size);
                bankroll_manager.remove_exposure(result.leg_b_platform, result.leg_b_size);
                circuit_breakers.record_execution(result.status == TradeStatus::Success);
                kelly.adjust_for_drawdown(bankroll_manager.drawdown_pct());
                // Release the in-flight capital reservation. approved_size is the exact
                // amount that was reserved when the opportunity was dispatched, so this
                // correctly frees capital whether the trade succeeded, partially filled, or failed.
                in_flight_notional = in_flight_notional.saturating_sub(result.approved_size);
                // Decrement cached counter now that the position is settled.
                cached_open_positions = cached_open_positions.saturating_sub(1);
                if let Err(e) = trade_result_tx2.try_send(result.clone()) {
                    // ERROR not warn: a dropped result means the position tracker
                    // never closes the DB record, leaving a ghost open position that
                    // inflates exposure and causes cached_open_positions to diverge
                    // from the database after a restart.
                    tracing::error!(error = %e, "Position tracker channel full — trade result dropped, open position may not be closed in DB");
                }
                match result.status {
                    TradeStatus::Success => metrics.inc_success(),
                    _ => metrics.inc_failed(),
                }
            }

            Some(gas) = gas_update_rx.recv() => {
                spread_engine.update_gas_price(Decimal::from(gas.gas_gwei));
                spread_engine.update_matic_price(gas.matic_usd);
                circuit_breakers.update_gas_price(gas.gas_gwei);
                debug!(gwei = gas.gas_gwei, matic_usd = %gas.matic_usd, "Gas parameters updated live");
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

    // Drain in-flight trades before killing subsystems. The executor task is
    // still alive (not yet aborted); give it up to 30 s to settle open legs.
    if cached_open_positions > 0 {
        info!(positions = cached_open_positions, "Draining in-flight positions (up to 30s)");
        let drain_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        while cached_open_positions > 0 {
            let remaining = drain_deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                warn!(positions = cached_open_positions, "Shutdown drain timeout — {} position(s) may remain open", cached_open_positions);
                break;
            }
            match tokio::time::timeout(remaining, trade_result_rx.recv()).await {
                Ok(Some(result)) => {
                    bankroll_manager.record_trade(&result);
                    bankroll_manager.remove_exposure(result.leg_a_platform, result.leg_a_size);
                    bankroll_manager.remove_exposure(result.leg_b_platform, result.leg_b_size);
                    cached_open_positions = cached_open_positions.saturating_sub(1);
                    if let Err(e) = trade_result_tx2.try_send(result) {
                        tracing::error!(error = %e, "Position tracker channel full — trade result dropped on second send path");
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

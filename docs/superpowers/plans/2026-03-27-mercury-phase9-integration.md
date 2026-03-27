# Phase 9: Main Supervisor & Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire all modules together in main.rs with supervisor pattern, health endpoint, monitoring metrics, config hot-reload, daily report scheduler, and deployment files.

**Architecture:** main.rs creates all channels, initializes all subsystems, spawns them as tokio tasks, and supervises with JoinHandle tracking. A daily scheduler triggers report generation. Config hot-reload via file watcher. Graceful shutdown on SIGINT/SIGTERM.

**Tech Stack:** tokio (spawn, signal, select), notify (file watcher), reqwest (health), serde_yaml

**Depends on:** All previous phases (1-8)

---

### Task 1: Monitoring & Health

**Files:**
- Create: `src/monitoring/mod.rs`
- Create: `src/monitoring/metrics.rs`
- Create: `src/monitoring/health.rs`

- [ ] **Step 1: Write metrics collector**

`src/monitoring/mod.rs`:
```rust
pub mod metrics;
pub mod health;
```

`src/monitoring/metrics.rs`:
```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Global metrics counter (lock-free atomics)
#[derive(Debug)]
pub struct Metrics {
    pub ticks_received: AtomicU64,
    pub spreads_evaluated: AtomicU64,
    pub opportunities_detected: AtomicU64,
    pub opportunities_executed: AtomicU64,
    pub trades_success: AtomicU64,
    pub trades_failed: AtomicU64,
    pub ws_reconnects: AtomicU32,
    pub api_errors: AtomicU32,
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ticks_received: AtomicU64::new(0),
            spreads_evaluated: AtomicU64::new(0),
            opportunities_detected: AtomicU64::new(0),
            opportunities_executed: AtomicU64::new(0),
            trades_success: AtomicU64::new(0),
            trades_failed: AtomicU64::new(0),
            ws_reconnects: AtomicU32::new(0),
            api_errors: AtomicU32::new(0),
            start_time: Instant::now(),
        })
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn inc_ticks(&self) { self.ticks_received.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_spreads(&self) { self.spreads_evaluated.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_detected(&self) { self.opportunities_detected.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_executed(&self) { self.opportunities_executed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_success(&self) { self.trades_success.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_failed(&self) { self.trades_failed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_reconnects(&self) { self.ws_reconnects.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_api_errors(&self) { self.api_errors.fetch_add(1, Ordering::Relaxed); }
}
```

- [ ] **Step 2: Write health endpoint**

`src/monitoring/health.rs`:
```rust
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tracing::{error, info};

use super::metrics::Metrics;

/// Simple HTTP health check endpoint
pub async fn run_health_server(port: u16, metrics: Arc<Metrics>) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, addr, "Failed to bind health server");
            return;
        }
    };

    info!(addr, "Health server listening");

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let body = format!(
                    r#"{{"status":"ok","uptime_secs":{},"ticks":{},"spreads_evaluated":{},"opportunities_detected":{},"executed":{},"success":{},"failed":{},"ws_reconnects":{},"api_errors":{}}}"#,
                    metrics.uptime_secs(),
                    metrics.ticks_received.load(Ordering::Relaxed),
                    metrics.spreads_evaluated.load(Ordering::Relaxed),
                    metrics.opportunities_detected.load(Ordering::Relaxed),
                    metrics.opportunities_executed.load(Ordering::Relaxed),
                    metrics.trades_success.load(Ordering::Relaxed),
                    metrics.trades_failed.load(Ordering::Relaxed),
                    metrics.ws_reconnects.load(Ordering::Relaxed),
                    metrics.api_errors.load(Ordering::Relaxed),
                );

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );

                let _ = stream.write_all(response.as_bytes()).await;
            }
            Err(e) => {
                error!(error = %e, "Health server accept error");
            }
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add src/monitoring/
git commit -m "feat: add metrics collector and health check HTTP endpoint"
```

---

### Task 2: Full Main.rs Supervisor

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Rewrite main.rs with full supervisor wiring**

`src/main.rs`:
```rust
use anyhow::Result;
use clap::Parser;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

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

use config::{ConfigManager, MercuryConfig};
use db::SqliteDb;
use types::*;

#[derive(Parser)]
#[command(name = "mercury", about = "MERCURY - Cross-Market Prediction Arbitrage Engine")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mercury=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    info!("MERCURY v{} starting...", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let mercury_config = MercuryConfig::load(&cli.config)?;
    let config_manager = ConfigManager::new(mercury_config.clone(), cli.config.clone());
    info!("Configuration loaded");

    // Initialize database
    let db = Arc::new(SqliteDb::new(&mercury_config.database)?) as Arc<dyn db::Database>;
    info!("Database initialized");

    // Initialize metrics
    let metrics = monitoring::metrics::Metrics::new();

    // ─── Create Channels ───
    let (tick_tx, _) = broadcast::channel::<NormalizedTick>(10_000);
    let (opportunity_tx, opportunity_rx) = mpsc::channel::<ValidatedOpportunity>(100);
    let (trade_result_tx, trade_result_rx) = mpsc::channel::<TradeResult>(100);
    let (alert_tx, alert_rx) = mpsc::channel::<AlertMessage>(500);
    let (daily_report_tx, daily_report_rx) = mpsc::channel::<DailyReport>(10);

    // Clone for position tracker
    let (trade_result_tx2, trade_result_rx2) = mpsc::channel::<TradeResult>(100);

    // ─── Initialize Telegram ───
    let tg_bot_token = std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
    let tg_alerts_chat = std::env::var("TELEGRAM_ALERTS_CHAT_ID").unwrap_or_default();
    let tg_report_chat = std::env::var("TELEGRAM_REPORT_CHAT_ID").unwrap_or_default();

    let tg_enabled = mercury_config.telegram.enabled && !tg_bot_token.is_empty();

    if tg_enabled {
        let bot = telegram::bot::TelegramBot::new(tg_bot_token.clone());

        // Alert service (Channel 1)
        let alert_service = telegram::alerts::AlertService::new(
            bot.clone(), tg_alerts_chat.clone(), alert_rx,
        );
        tokio::spawn(alert_service.run());

        // Report service (Channel 2)
        let report_service = telegram::reports::ReportService::new(
            bot, tg_report_chat.clone(), daily_report_rx,
        );
        tokio::spawn(report_service.run());

        info!("Telegram notifications enabled");
    } else {
        warn!("Telegram notifications disabled (set TELEGRAM_BOT_TOKEN env var to enable)");
        // Drop receivers to prevent channel backup
        drop(alert_rx);
        drop(daily_report_rx);
    }

    // ─── Initialize Risk Manager ───
    let initial_bankroll = mercury_config.trading.initial_bankroll;
    let mut bankroll_manager = risk::bankroll::BankrollManager::new(initial_bankroll);
    let mut kelly = risk::kelly::KellyCalculator::new(
        Decimal::try_from(mercury_config.trading.kelly_fraction_multiplier).unwrap_or(Decimal::from(25) / Decimal::from(100))
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

    // ─── Initialize Engine ───
    let mut uob = engine::order_book::UnifiedOrderBook::new();
    let mut spread_engine = engine::spread::NetSpreadEngine::new(
        mercury_config.trading.min_net_spread_threshold,
    );
    let mut detector = engine::detector::ArbitrageDetector::new(
        mercury_config.trading.min_net_spread_threshold,
        Decimal::from(5), // min order size $5
        mercury_config.trading.stale_data_timeout_ms,
        mercury_config.trading.max_concurrent_arbs,
    );
    let mut registry = engine::market_registry::MarketRegistry::new();

    // ─── Initialize Execution Engine ───
    let executor = execution::executor::ExecutionEngine::new(
        opportunity_rx,
        trade_result_tx.clone(),
        alert_tx.clone(),
        db.clone(),
        None, // Polymarket client - initialize when credentials available
        None, // Kalshi client
        None, // CDNA client
        None, // ForecastEx client
        initial_bankroll,
    );
    tokio::spawn(executor.run());

    // ─── Initialize Position Tracker ───
    let position_tracker = inventory::positions::PositionTracker::new(
        db.clone(), trade_result_rx2,
    );
    tokio::spawn(position_tracker.run());

    // ─── Initialize Reconciler ───
    let reconciler = inventory::reconciler::Reconciler::new(
        db.clone(), alert_tx.clone(), 60, Decimal::from_str_exact("0.10").unwrap(),
    );
    tokio::spawn(reconciler.run());

    // ─── Initialize Settlement Monitor ───
    let settlement = inventory::settlement::SettlementMonitor::new(
        db.clone(), alert_tx.clone(), 300, // Check every 5 minutes
    );
    tokio::spawn(settlement.run());

    // ─── Health Server ───
    let health_metrics = metrics.clone();
    tokio::spawn(monitoring::health::run_health_server(
        mercury_config.health.port,
        health_metrics,
    ));

    // ─── Feed Handlers ───
    // Note: Feed handlers would be spawned here with actual subscriptions
    // For now, we start with no subscriptions until markets are registered
    info!("All subsystems initialized");

    // ─── Main Event Loop ───
    let mut tick_rx = tick_tx.subscribe();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    info!("MERCURY engine running. Press Ctrl+C to shutdown.");

    // Daily report timer
    let daily_report_interval = tokio::time::interval(std::time::Duration::from_secs(86400));
    tokio::pin!(daily_report_interval);

    loop {
        tokio::select! {
            // Process incoming ticks
            Ok(tick) = tick_rx.recv() => {
                metrics.inc_ticks();

                // Update unified order book
                uob.update(&tick);

                // Run spread engine on all arb pairs
                for pair in registry.get_arb_pairs() {
                    if let (Some(book_a), Some(book_b)) = (
                        uob.get_book(&pair.market_id, &pair.platform_a),
                        uob.get_book(&pair.market_id, &pair.platform_b),
                    ) {
                        let target_size = Decimal::from(10); // Default target
                        let spreads = spread_engine.compute_spreads(book_a, book_b, target_size);
                        metrics.spreads_evaluated.fetch_add(spreads.len() as u64, std::sync::atomic::Ordering::Relaxed);
                    }
                }

                // Run detector
                let opps = detector.detect(
                    &registry, &uob, &spread_engine,
                    Decimal::from(10),
                );

                for opp in opps {
                    metrics.inc_detected();

                    // Kelly sizing
                    let exec_prob = bankroll_manager.exec_success_rate();
                    let kelly_frac = kelly.optimal_fraction(exec_prob, opp.net_spread);
                    let approved_size = kelly.position_size(
                        bankroll_manager.total_bankroll(),
                        exec_prob,
                        opp.net_spread,
                        mercury_config.trading.max_single_trade_pct,
                    );

                    // Circuit breaker check
                    let open_positions = db.get_open_positions().await.map(|p| p.len()).unwrap_or(0);
                    let involves_poly = matches!(opp.leg_a.platform, Platform::Polymarket | Platform::PolymarketUs)
                        || matches!(opp.leg_b.platform, Platform::Polymarket | Platform::PolymarketUs);

                    let trips = circuit_breakers.check_all(
                        approved_size,
                        bankroll_manager.total_bankroll(),
                        bankroll_manager.daily_loss_pct(),
                        bankroll_manager.drawdown_pct(),
                        bankroll_manager.platform_exposure_pct(&opp.leg_a.platform)
                            .max(bankroll_manager.platform_exposure_pct(&opp.leg_b.platform)),
                        open_positions,
                        involves_poly,
                    );

                    if !trips.is_empty() {
                        for trip in &trips {
                            let _ = alert_tx.send(AlertMessage::CircuitBreaker {
                                breaker_type: trip.breaker_type.clone(),
                                details: trip.details.clone(),
                                action: trip.action.clone(),
                                resume_at: trip.resume_at,
                            }).await;
                        }
                        continue;
                    }

                    if approved_size > Decimal::ZERO {
                        let validated = ValidatedOpportunity {
                            opportunity: opp,
                            approved_size,
                            risk_score: kelly_frac,
                        };
                        let _ = opportunity_tx.send(validated).await;
                        metrics.inc_executed();
                    }
                }
            }

            // Process trade results for bankroll tracking
            Ok(result) = async { trade_result_rx.recv().await.ok_or(()) } => {
                bankroll_manager.record_trade(&result);
                circuit_breakers.record_execution(result.status == TradeStatus::Success);
                kelly.adjust_for_drawdown(bankroll_manager.drawdown_pct());

                // Forward to position tracker
                let _ = trade_result_tx2.send(result).await;

                match result.status {
                    TradeStatus::Success => metrics.inc_success(),
                    _ => metrics.inc_failed(),
                }
            }

            // Daily report timer
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
                let top_trades = sorted_trades.iter().take(3).cloned().collect();
                sorted_trades.sort_by(|a, b| a.profit.cmp(&b.profit));
                let worst_trades = sorted_trades.iter().take(3).cloned().collect();

                let db_size = db.db_size_bytes().await.unwrap_or(0);

                let report = DailyReport {
                    snapshot,
                    platform_breakdown,
                    top_trades,
                    worst_trades,
                    uptime_secs: metrics.uptime_secs(),
                    ws_reconnects: metrics.ws_reconnects.load(std::sync::atomic::Ordering::Relaxed),
                    api_errors: metrics.api_errors.load(std::sync::atomic::Ordering::Relaxed),
                    db_size_bytes: db_size,
                };

                let _ = daily_report_tx.send(report).await;
                bankroll_manager.reset_daily();
            }

            // Graceful shutdown
            _ = &mut shutdown => {
                info!("Shutdown signal received");

                // Send shutdown notification
                if tg_enabled {
                    let bot = telegram::bot::TelegramBot::new(tg_bot_token.clone());
                    let _ = bot.send_message(
                        &tg_alerts_chat,
                        "🔴 <b>MERCURY SHUTTING DOWN</b>\n\nGraceful shutdown initiated.",
                    ).await;
                }

                info!("MERCURY shutdown complete");
                break;
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (may need minor fixes for borrow checker issues)

- [ ] **Step 3: Commit**

```bash
git add src/main.rs src/monitoring/
git commit -m "feat: wire up full supervisor with all subsystems, channels, and shutdown"
```

---

### Task 3: Deployment Files

**Files:**
- Create: `deploy/mercury.service`
- Create: `deploy/setup.sh`
- Create: `deploy/Dockerfile`

- [ ] **Step 1: Write systemd service file**

`deploy/mercury.service`:
```ini
[Unit]
Description=MERCURY Arbitrage Engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=mercury
Group=mercury
WorkingDirectory=/opt/mercury
ExecStart=/opt/mercury/mercury --config /opt/mercury/config/default.yaml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=mercury

# Environment
EnvironmentFile=-/opt/mercury/.env

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/opt/mercury/data /opt/mercury/logs
PrivateTmp=yes

# Resource limits
LimitNOFILE=65536
MemoryMax=3G

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 2: Write setup script**

`deploy/setup.sh`:
```bash
#!/bin/bash
set -euo pipefail

echo "=== MERCURY Instance Setup ==="

# Create user
sudo useradd -r -s /bin/false mercury 2>/dev/null || true

# Create directories
sudo mkdir -p /opt/mercury/{config,data,logs,keys}
sudo chown -R mercury:mercury /opt/mercury

# Copy binary
sudo cp target/release/mercury /opt/mercury/mercury
sudo chmod +x /opt/mercury/mercury

# Copy config
sudo cp config/default.yaml /opt/mercury/config/

# Create .env template
if [ ! -f /opt/mercury/.env ]; then
    cat <<'ENVEOF' | sudo tee /opt/mercury/.env
RUST_LOG=mercury=info
TELEGRAM_BOT_TOKEN=
TELEGRAM_ALERTS_CHAT_ID=
TELEGRAM_REPORT_CHAT_ID=
POLYMARKET_API_KEY=
POLYMARKET_API_SECRET=
POLYMARKET_API_PASSPHRASE=
KALSHI_API_KEY_ID=
WALLET_PASSPHRASE=
ENVEOF
    sudo chmod 600 /opt/mercury/.env
    sudo chown mercury:mercury /opt/mercury/.env
    echo ">>> Edit /opt/mercury/.env with your credentials"
fi

# Install systemd service
sudo cp deploy/mercury.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable mercury

echo "=== Setup complete ==="
echo "1. Edit /opt/mercury/.env with your credentials"
echo "2. Edit /opt/mercury/config/default.yaml as needed"
echo "3. Start with: sudo systemctl start mercury"
echo "4. View logs: journalctl -u mercury -f"
```

- [ ] **Step 3: Write Dockerfile**

`deploy/Dockerfile`:
```dockerfile
FROM rust:1.77-slim AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false mercury
COPY --from=builder /build/target/release/mercury /opt/mercury/mercury
COPY config/default.yaml /opt/mercury/config/default.yaml
RUN mkdir -p /opt/mercury/data /opt/mercury/logs /opt/mercury/keys && \
    chown -R mercury:mercury /opt/mercury
USER mercury
WORKDIR /opt/mercury
EXPOSE 8080
CMD ["/opt/mercury/mercury", "--config", "/opt/mercury/config/default.yaml"]
```

- [ ] **Step 4: Commit**

```bash
git add deploy/ src/monitoring/
git commit -m "feat: add deployment files - systemd, setup script, Dockerfile"
```

---

### Task 4: Final Build & Verification

- [ ] **Step 1: Full cargo build**

Run: `cargo build --release`
Expected: Compiles successfully

- [ ] **Step 2: Run with default config**

Run: `cargo run -- --config config/default.yaml`
Expected: Starts, initializes all subsystems, waits for ticks

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: MERCURY v0.1.0 - complete single-instance arbitrage engine"
```

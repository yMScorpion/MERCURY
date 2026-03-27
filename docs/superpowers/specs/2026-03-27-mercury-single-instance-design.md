# MERCURY Single-Instance Design Spec

**Date:** 2026-03-27
**Version:** 3.0 (adapted from SDD v2.1 / SAD v2.1)
**Status:** Approved

## Overview

Project MERCURY is a cross-market prediction arbitrage engine that identifies, validates, and executes arbitrage opportunities across Polymarket, Kalshi, Crypto.com Derivatives (CDNA), and ForecastEx. This spec adapts the original multi-instance architecture (SDD/SAD v2.1) to run on a single cheap AWS instance with Telegram-based profit notifications.

## Key Architectural Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust (everything) | Performance, safety, single binary deployment |
| Architecture | Monolithic async binary (tokio) | Minimal overhead on 2-vCPU instance, shared memory, simple deployment |
| Platforms | Polymarket + Kalshi + CDNA + ForecastEx | All four from day one as specified in SDD |
| Instance | AWS t3.medium (2 vCPU, 4GB RAM) | ~$30/month, sufficient for Rust binary + SQLite |
| Database | SQLite with trait abstraction | Zero overhead, swap to PostgreSQL later via `Database` trait |
| Key storage | Local AES-GCM encrypted keyfile | $0 vs $1,400/month CloudHSM |
| Notifications | Telegram (2 channels) | Real-time trade alerts + daily summary reports |
| Monetary math | `rust_decimal` | Exact decimal arithmetic, no floating-point rounding errors |
| Inter-module comms | tokio broadcast + mpsc channels | Lock-free, zero-copy within single process |
| Recovery | systemd auto-restart + daily S3 backups | RTO ~5 minutes |

## Cost Comparison

| Original SAD | Single-Instance |
|-------------|----------------|
| ~$2,510/month (6 instances + CloudHSM) | ~$43/month (1 instance + EBS + S3 + EIP) |

## Project Structure

```
mercury/
├── Cargo.toml
├── config/
│   └── default.yaml              # All runtime configuration
├── deploy/
│   ├── setup.sh                  # Single-instance provisioning
│   ├── mercury.service           # systemd unit
│   └── Dockerfile                # Optional container deploy
├── src/
│   ├── main.rs                   # Entry point, tokio runtime, supervisor
│   ├── config.rs                 # YAML config loader + hot-reload (notify)
│   ├── types.rs                  # Shared types: NormalizedTick, Platform, UnifiedMarketId, etc.
│   ├── feeds/
│   │   ├── mod.rs
│   │   ├── base.rs               # FeedHandler trait
│   │   ├── polymarket.rs         # Polymarket CLOB WebSocket
│   │   ├── kalshi.rs             # Kalshi WebSocket + REST
│   │   ├── cdna.rs               # Crypto.com CDNA adapter
│   │   ├── forecastex.rs         # ForecastEx FIX 4.4 adapter
│   │   └── normalizer.rs         # Platform-specific -> NormalizedTick
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── order_book.rs         # Unified Order Book (per-platform + aggregated)
│   │   ├── spread.rs             # Net Spread Engine (fees, slippage, gas)
│   │   ├── detector.rs           # Arbitrage Detector (5-gate pipeline)
│   │   └── market_registry.rs    # Cross-platform market matching
│   ├── execution/
│   │   ├── mod.rs
│   │   ├── executor.rs           # Dual-leg execution state machine
│   │   ├── polymarket_client.rs  # EIP-712 signing, CLOB REST
│   │   ├── kalshi_client.rs      # JWT auth, REST orders
│   │   ├── cdna_client.rs        # CDNA order submission
│   │   └── forecastex_client.rs  # FIX protocol order submission
│   ├── risk/
│   │   ├── mod.rs
│   │   ├── kelly.rs              # Kelly Criterion (fractional, multi-asset)
│   │   ├── bankroll.rs           # Bankroll manager, exposure tracking
│   │   └── circuit_breaker.rs    # All circuit breakers from SDD 7.3
│   ├── inventory/
│   │   ├── mod.rs
│   │   ├── positions.rs          # Double-entry position ledger
│   │   ├── reconciler.rs         # Periodic reconciliation
│   │   └── settlement.rs         # Settlement monitoring + redemption
│   ├── db/
│   │   ├── mod.rs
│   │   ├── traits.rs             # Database trait abstraction
│   │   ├── sqlite.rs             # SQLite implementation
│   │   └── migrations.rs         # Schema migrations
│   ├── telegram/
│   │   ├── mod.rs
│   │   ├── bot.rs                # Telegram HTTP API client
│   │   ├── alerts.rs             # Real-time trade alerts (channel 1)
│   │   └── reports.rs            # Daily summary reports (channel 2)
│   ├── crypto/
│   │   ├── mod.rs
│   │   ├── keystore.rs           # Encrypted keyfile load/decrypt
│   │   ├── eip712.rs             # EIP-712 structured data signing
│   │   └── jwt.rs                # JWT/RSA for Kalshi auth
│   └── monitoring/
│       ├── mod.rs
│       ├── metrics.rs            # Internal metrics (latency, throughput, PnL)
│       └── health.rs             # Health check endpoint (tiny HTTP server)
```

## Data Flow

```
Feed Handlers (4x WebSocket tasks)
    │
    ▼ broadcast<NormalizedTick>
    │
Unified Order Book ──► Net Spread Engine ──► Arbitrage Detector
                                                    │
                                              Risk/Kelly Check
                                                    │
                                              ▼ mpsc<ValidatedOpportunity>
                                                    │
                                            Execution Engine
                                                    │
                                              ▼ mpsc<TradeResult>
                                           ┌────────┴────────┐
                                           ▼                  ▼
                                    TG Alerts (Ch1)    Inventory/DB
                                                           │
                                                    Daily Aggregation
                                                           │
                                                    TG Reports (Ch2)
```

### Channel Definitions

| Channel | Type | Sender | Receiver(s) |
|---------|------|--------|-------------|
| `tick_bus` | `broadcast<NormalizedTick>` | 4 feed handlers | OrderBook, MarketRegistry |
| `opportunity_tx` | `mpsc<ValidatedOpportunity>` | Detector (post-risk-check) | Executor |
| `trade_result_tx` | `mpsc<TradeResult>` | Executor | Inventory, TG Alerts, Metrics |
| `alert_tx` | `mpsc<AlertMessage>` | Risk, CircuitBreaker, Reconciler | TG Alerts |
| `daily_report_tx` | `mpsc<DailyReport>` | Scheduled task (24h timer) | TG Reports |

### Supervisor Pattern

- Each subsystem spawned via `tokio::spawn` with `JoinHandle` tracking
- Critical task panic: log error, send Telegram alert, restart task
- Non-critical tasks (telegram, metrics) failing don't halt trading
- Graceful shutdown via `tokio::signal::ctrl_c()`: cancel open orders, flush DB, notify Telegram

## Telegram Notification System

### Channel 1: Real-Time Trade Alerts

Every arbitrage attempt triggers a notification:

**Successful trade format:**
```
[check] ARBITRAGE #247 -- PROFIT
Market: "Will Bitcoin hit $100k by April?"
Leg A: BUY YES @ $0.62 on Polymarket (50 contracts)
Leg B: BUY NO  @ $0.31 on Kalshi (50 contracts)
Raw Spread: 7.00% | Net Spread: 3.42%
Fees: $1.83 | Slippage: $0.12
Profit: +$17.10
Bankroll: $5,017.10 (+0.34%) | Exec: 127ms
```

**Failed trade format:**
```
[x] ARBITRAGE #248 -- LOSS
Market: "Fed rate cut in May?"
Leg A: BUY YES @ $0.45 on Kalshi (30 contracts) -- FILLED
Leg B: BUY NO  @ $0.50 on Polymarket -- SLIPPAGE
Expected: 5.00% | Actual fill: $0.53 (3c worse)
Loss: -$2.10
Bankroll: $5,015.00 (-0.04%) | Reason: Hedge leg slippage
```

**Circuit breaker format:**
```
[!] CIRCUIT BREAKER: Max Daily Loss (5%)
Daily PnL: -$250.50 | Trading halted 24h
Resume: 2026-03-28 14:30 UTC
```

### Channel 2: Daily Summary Reports

Sent daily at configurable time (default 00:00 UTC):
- P&L summary (gross, fees, net, ROI)
- Trading activity (detected/executed/success rate)
- Platform breakdown (exposure, trades, PnL per platform)
- Risk metrics (bankroll, drawdown, Kelly utilization)
- Top 3 trades (best wins and worst losses)
- System health (uptime, reconnects, errors, DB size)

### Implementation

- Direct HTTP to `api.telegram.org/bot<token>/sendMessage`
- `parse_mode: MarkdownV2` for formatting
- Rate limited to 30 msg/min (Telegram limit)
- Queue with exponential backoff retry on failure
- Configured via `TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALERTS_CHAT_ID`, `TELEGRAM_REPORT_CHAT_ID` env vars

## Database Schema

### SQLite Configuration

- `journal_mode = WAL` (concurrent reads during writes)
- `synchronous = NORMAL` (durability/speed balance)
- `busy_timeout = 5000`
- Connection pool: 2-4 connections via `r2d2`

### Tables

**markets:** `unified_id` (PK), `question`, `resolution_source`, `expiration`, `platforms` (JSON), `category`, `confidence`, `status`, `created_at`, `updated_at`

**trades:** `id` (PK), `opp_id`, `market_id` (FK), `leg_a_platform`, `leg_a_side`, `leg_a_price`, `leg_a_size`, `leg_a_fill_price`, `leg_a_fee`, `leg_b_platform`, `leg_b_side`, `leg_b_price`, `leg_b_size`, `leg_b_fill_price`, `leg_b_fee`, `raw_spread`, `net_spread`, `profit`, `status` (success/fail/partial), `failure_reason`, `execution_ms`, `executed_at`

**positions:** `id` (PK), `market_id` (FK), `platform`, `side`, `quantity`, `avg_entry_price`, `unrealized_pnl`, `opened_at`, `updated_at`

**balances:** `platform` (PK), `available`, `reserved`, `pending_settlement`, `total`, `updated_at`

**daily_snapshots:** `date` (PK), `bankroll`, `gross_pnl`, `fees_paid`, `net_pnl`, `trades_count`, `success_count`, `fail_count`, `success_rate`, `peak_bankroll`, `drawdown_pct`, `kelly_utilization`, `report_sent`

**audit_log:** `id` (PK), `timestamp_ns`, `module`, `event_type`, `data` (JSON)

**config_history:** `id` (PK), `changed_at`, `key`, `old_value`, `new_value`

### Trait Abstraction

`db::traits::Database` trait with async methods for all CRUD operations. `SqliteDb` implements it. Future `PostgresDb` can be swapped in without changing any calling code.

## Algorithms (Carried Over from SDD v2.1)

All core algorithms are unchanged from the SDD:

### Net Spread Calculation
```
net_spread = raw_spread
  - fee_platform_A(size, price_A)
  - fee_platform_B(size, price_B)
  - slippage_A(size, depth_A)
  - slippage_B(size, depth_B)
  - gas_cost_if_onchain
  - settlement_risk_premium
```

### 5-Gate Detection Pipeline
G1: Spread threshold | G2: Liquidity check | G3: Staleness filter | G4: Correlation check | G5: Risk budget

### Kelly Criterion
`f* = (p * b - q) / b` with fractional multiplier (default 0.25x), dynamically adjusted.

### Dual-Leg Execution
Aggressive-first strategy: risky leg first, hedge leg second. State machine: PENDING -> PARTIAL_FILL -> FILLED -> FAILED -> UNWINDING.

### Circuit Breakers
All 10 circuit breakers from SDD Section 7.3 (max single trade 2%, max daily loss 5%, max drawdown 15%, etc.)

## Platform Fee Models

| Platform | Taker Fee Formula |
|----------|------------------|
| Polymarket (Global) | `fee_rate_bps/10000 * C * max(C, 1-C)` (dynamic, up to 1.80%) |
| Polymarket US (DCM) | Fixed 30bps taker / 20bps maker rebate |
| Kalshi | `$0.07 * C * (1-C)` per contract (max $0.0175 at 50c) |
| CDNA | Probability-weighted, similar to Polymarket |
| ForecastEx | Spread-embedded (half-spread cost from order book) |

## Infrastructure

### AWS Setup
- Region: us-east-1a
- Instance: t3.medium (2 vCPU, 4GB RAM), Reserved 1yr
- Storage: 20GB gp3 root + 20GB gp3 data
- Static IP: 1 Elastic IP
- Security: SSH from operator IP only, outbound HTTPS only

### Deployment
- Single binary at `/opt/mercury/mercury`
- Config at `/opt/mercury/config/default.yaml`
- DB at `/opt/mercury/data/mercury.db`
- Keys at `/opt/mercury/keys/wallet.enc`
- Managed by systemd with `Restart=always`

### Backups
- SQLite: daily `.backup` to S3
- Audit logs: daily rotation + S3 archival
- Config: git-versioned

### Monthly Cost: ~$43

## Crate Dependencies

| Crate | Purpose |
|-------|---------|
| tokio | Async runtime, channels, timers, signals |
| tokio-tungstenite | WebSocket client |
| reqwest | HTTP client (REST APIs + Telegram) |
| serde / serde_json / serde_yaml | Serialization |
| rusqlite | SQLite |
| r2d2 | Connection pooling |
| ethers | EIP-712 signing |
| jsonwebtoken | Kalshi JWT auth |
| rsa / ring | RSA keys, crypto primitives |
| aes-gcm | Keyfile decryption |
| chrono | Timestamps, scheduling |
| uuid | Unique identifiers |
| notify | Config hot-reload |
| tracing / tracing-subscriber | Structured logging |
| anyhow / thiserror | Error handling |
| rust_decimal | Exact decimal arithmetic |
| clap | CLI arguments |

## What Was Removed vs Original SDD/SAD

| Removed | Replacement |
|---------|-------------|
| CloudHSM ($1,400/mo) | Local encrypted keyfile ($0) |
| Hot standby instance | systemd auto-restart |
| Dedicated Polygon full node | Public RPC (Alchemy free tier) |
| Cluster Placement Group | Single instance |
| ECS/Fargate | Single binary |
| Prometheus/Grafana | Telegram reports + SQLite metrics |
| NAT Gateway | Direct public IP with security groups |
| Multi-AZ deployment | Single AZ with S3 backups |
| Aeron state replication | N/A (single instance) |
| mTLS inter-service | N/A (single process) |

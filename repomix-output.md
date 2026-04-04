This file is a merged representation of a subset of the codebase, containing files not matching ignore patterns, combined into a single document by Repomix.

# File Summary

## Purpose
This file contains a packed representation of a subset of the repository's contents that is considered the most important context.
It is designed to be easily consumable by AI systems for analysis, code review,
or other automated processes.

## File Format
The content is organized as follows:
1. This summary section
2. Repository information
3. Directory structure
4. Repository files (if enabled)
5. Multiple file entries, each consisting of:
  a. A header with the file path (## File: path/to/file)
  b. The full contents of the file in a code block

## Usage Guidelines
- This file should be treated as read-only. Any changes should be made to the
  original repository files, not this packed version.
- When processing this file, use the file path to distinguish
  between different files in the repository.
- Be aware that this file may contain sensitive information. Handle it with
  the same level of security as you would the original repository.

## Notes
- Some files may have been excluded based on .gitignore rules and Repomix's configuration
- Binary files are not included in this packed representation. Please refer to the Repository Structure section for a complete list of file paths, including binary files
- Files matching these patterns are excluded: .claude/, .claude-flow/, .cargo/, .serena/, docs/, CLAUDE.md
- Files matching patterns in .gitignore are excluded
- Files matching default ignore patterns are excluded
- Files are sorted by Git change count (files with more changes are at the bottom)

# Directory Structure
```
.gitignore
.mcp.json
Cargo.toml
config/default.yaml
deploy/Dockerfile
deploy/mercury.service
deploy/setup.sh
src/config.rs
src/crypto/eip712.rs
src/crypto/jwt.rs
src/crypto/mod.rs
src/db/migrations.rs
src/db/mod.rs
src/db/sqlite.rs
src/db/traits.rs
src/engine/detector.rs
src/engine/market_registry.rs
src/engine/mod.rs
src/engine/order_book.rs
src/engine/spread.rs
src/execution/cdna_client.rs
src/execution/executor.rs
src/execution/forecastex_client.rs
src/execution/kalshi_client.rs
src/execution/mod.rs
src/execution/polymarket_client.rs
src/feeds/base.rs
src/feeds/cdna.rs
src/feeds/discovery.rs
src/feeds/forecastex.rs
src/feeds/kalshi.rs
src/feeds/mod.rs
src/feeds/normalizer.rs
src/feeds/polymarket.rs
src/inventory/mod.rs
src/inventory/positions.rs
src/inventory/reconciler.rs
src/inventory/settlement.rs
src/inventory/unwind_watchdog.rs
src/main.rs
src/monitoring/backup.rs
src/monitoring/gas_oracle.rs
src/monitoring/health.rs
src/monitoring/metrics.rs
src/monitoring/mod.rs
src/risk/bankroll.rs
src/risk/circuit_breaker.rs
src/risk/kelly.rs
src/risk/mod.rs
src/telegram/alerts.rs
src/telegram/bot.rs
src/telegram/mod.rs
src/telegram/reports.rs
src/types.rs
```

# Files

## File: .gitignore
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

## File: .mcp.json
```json
{
  "mcpServers": {
    "claude-flow": {
      "command": "cmd",
      "args": [
        "/c",
        "npx",
        "-y",
        "@claude-flow/cli@latest",
        "mcp",
        "start"
      ],
      "env": {
        "npm_config_update_notifier": "false",
        "CLAUDE_FLOW_MODE": "v3",
        "CLAUDE_FLOW_HOOKS_ENABLED": "true",
        "CLAUDE_FLOW_TOPOLOGY": "hierarchical-mesh",
        "CLAUDE_FLOW_MAX_AGENTS": "15",
        "CLAUDE_FLOW_MEMORY_BACKEND": "hybrid"
      },
      "autoStart": false
    },
    "serena": {
      "command": "uvx",
      "args": [
        "--from",
        "git+https://github.com/oraios/serena",
        "serena",
        "start-mcp-server",
        "--project-from-cwd",
        "--context",
        "claude-code",
        "--mode",
        "interactive",
        "--mode",
        "editing",
        "--enable-web-dashboard",
        "false",
        "--open-web-dashboard",
        "false",
        "--transport",
        "stdio"
      ]
    }
  }
}
```

## File: deploy/mercury.service
```
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
# C-4 FIX: Removed EnvironmentFile to prevent leaking secrets into process memory. Parsed natively by Rust instead.
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/opt/mercury/data /opt/mercury/logs
PrivateTmp=yes
LimitNOFILE=65536
MemoryMax=3G

[Install]
WantedBy=multi-user.target
```

## File: deploy/setup.sh
```bash
#!/bin/bash
set -euo pipefail

echo "=== MERCURY Instance Setup ==="

sudo useradd -r -s /bin/false mercury 2>/dev/null || true
sudo mkdir -p /opt/mercury/{config,data,logs,keys}
sudo chown -R mercury:mercury /opt/mercury
sudo cp target/release/mercury /opt/mercury/mercury
sudo chmod +x /opt/mercury/mercury
sudo cp config/default.yaml /opt/mercury/config/

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

sudo cp deploy/mercury.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable mercury

echo "=== Setup complete ==="
echo "1. Edit /opt/mercury/.env with your credentials"
echo "2. Start with: sudo systemctl start mercury"
echo "3. View logs: journalctl -u mercury -f"
```

## File: src/db/mod.rs
```rust
pub mod migrations;
pub mod sqlite;
pub mod traits;

pub use sqlite::SqliteDb;
pub use traits::Database;
```

## File: src/engine/mod.rs
```rust
pub mod order_book;
pub mod spread;
pub mod detector;
pub mod market_registry;
```

## File: src/execution/mod.rs
```rust
pub mod executor;
pub mod polymarket_client;
pub mod kalshi_client;
pub mod cdna_client;
pub mod forecastex_client;
```

## File: src/monitoring/backup.rs
```rust
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::db::Database;

/// Periodically backs up the SQLite database to a timestamped file.
pub struct BackupTask {
    db: Arc<dyn Database>,
    backup_dir: String,
    interval: Duration,
}

impl BackupTask {
    pub fn new(db: Arc<dyn Database>, backup_dir: String, interval_secs: u64) -> Self {
        Self {
            db,
            backup_dir,
            interval: Duration::from_secs(interval_secs),
        }
    }

    pub async fn run(self) {
        if let Err(e) = std::fs::create_dir_all(&self.backup_dir) {
            error!(error = %e, dir = %self.backup_dir, "Failed to create backup directory");
            return;
        }

        info!(interval_secs = self.interval.as_secs(), dir = %self.backup_dir, "DB backup task started");
        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let dest = format!("{}/mercury_backup_{}.db", self.backup_dir, timestamp);

            match self.db.backup_to_file(&dest).await {
                Ok(()) => {
                    info!(path = %dest, "Database backup completed");
                    // Prune old backups: keep last 7
                    if let Err(e) = self.prune_old_backups(7) {
                        warn!(error = %e, "Failed to prune old backups");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Database backup failed");
                }
            }
        }
    }

    fn prune_old_backups(&self, keep: usize) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with("mercury_backup_") && n.ends_with(".db"))
                    .unwrap_or(false)
            })
            .collect();

        // L-9 FIX: Sort by actual file creation/modification time, not lexicographically
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));

        if entries.len() > keep {
            for entry in &entries[..entries.len() - keep] {
                std::fs::remove_file(entry.path())?;
                info!(path = ?entry.path(), "Pruned old backup");
            }
        }
        Ok(())
    }
}
```

## File: src/risk/mod.rs
```rust
pub mod kelly;
pub mod bankroll;
pub mod circuit_breaker;
```

## File: src/telegram/alerts.rs
```rust
use crate::types::*;
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AlertService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<AlertMessage>,
}

impl AlertService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<AlertMessage>) -> Self {
        Self { bot, chat_id, rx }
    }

    pub async fn run(mut self) {
        info!("Telegram alert service started");
        while let Some(msg) = self.rx.recv().await {
            let text = match &msg {
                AlertMessage::TradeComplete(trade) => format_trade_alert(trade),
                AlertMessage::CircuitBreaker { breaker_type, details, action, resume_at } => {
                    format_circuit_breaker(breaker_type, details, action, resume_at.as_ref())
                }
                AlertMessage::SystemAlert { severity, message } => {
                    format_system_alert(severity, message)
                }
            };

            if let Err(e) = self.bot.send_message(&self.chat_id, &text).await {
                error!(error = %e, "Failed to send Telegram alert");
            }
        }
        info!("Telegram alert service stopped");
    }
}

fn format_trade_alert(trade: &TradeResult) -> String {
    let icon = match trade.status {
        TradeStatus::Success => "✅",
        TradeStatus::Fail => "❌",
        TradeStatus::Partial => "⚠️",
    };

    let label = match trade.status {
        TradeStatus::Success => "PROFIT",
        TradeStatus::Fail => "LOSS",
        TradeStatus::Partial => "PARTIAL",
    };

    let profit_icon = if trade.profit >= Decimal::ZERO { "💰" } else { "💸" };
    let profit_sign = if trade.profit >= Decimal::ZERO { "+" } else { "" };
    let pct_sign = if trade.bankroll_change_pct >= Decimal::ZERO { "+" } else { "" };

    let question = TelegramBot::escape_html(&trade.market_question);

    let mut msg = format!(
        "{icon} <b>ARBITRAGE #{id} — {label}</b>\n\
         \n\
         <b>Market:</b> \"{question}\"\n\
         Leg A: BUY {side_a} @ ${price_a} on {plat_a} ({size_a} contracts)\n\
         Leg B: BUY {side_b} @ ${price_b} on {plat_b} ({size_b} contracts)\n\
         \n\
         Raw Spread: {raw}% | Net Spread: {net}%\n\
         Fees: ${fee_a} + ${fee_b} = ${fees_total}",
        id = trade.trade_id,
        side_a = trade.leg_a_side,
        price_a = trade.leg_a_fill_price,
        plat_a = trade.leg_a_platform,
        size_a = trade.leg_a_size,
        side_b = trade.leg_b_side,
        price_b = trade.leg_b_fill_price,
        plat_b = trade.leg_b_platform,
        size_b = trade.leg_b_size,
        raw = (trade.raw_spread * Decimal::from(100)).round_dp(2),
        net = (trade.net_spread * Decimal::from(100)).round_dp(2),
        fee_a = trade.leg_a_fee,
        fee_b = trade.leg_b_fee,
        fees_total = trade.leg_a_fee + trade.leg_b_fee,
    );

    msg.push_str(&format!(
        "\n\n{profit_icon} <b>Profit: {profit_sign}${profit}</b>\n\
         📊 Bankroll: ${bankroll} ({pct_sign}{pct}%)\n\
         ⏱️ Execution: {exec_ms}ms",
        profit = trade.profit.round_dp(2),
        bankroll = trade.bankroll_after.round_dp(2),
        pct = trade.bankroll_change_pct.round_dp(4),
        exec_ms = trade.execution_ms,
    ));

    if let Some(reason) = &trade.failure_reason {
        msg.push_str(&format!("\n🔍 Reason: {}", TelegramBot::escape_html(reason)));
    }

    msg
}

fn format_circuit_breaker(
    breaker_type: &str,
    details: &str,
    action: &str,
    resume_at: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
    let mut msg = format!(
        "🚨 <b>CIRCUIT BREAKER TRIGGERED</b>\n\
         \n\
         <b>Type:</b> {}\n\
         <b>Details:</b> {}\n\
         <b>Action:</b> {}",
        TelegramBot::escape_html(breaker_type),
        TelegramBot::escape_html(details),
        TelegramBot::escape_html(action),
    );

    if let Some(resume) = resume_at {
        msg.push_str(&format!("\n<b>Resume:</b> {} UTC", resume.format("%Y-%m-%d %H:%M")));
    }

    msg
}

fn format_system_alert(severity: &str, message: &str) -> String {
    let icon = match severity.to_lowercase().as_str() {
        "critical" | "p1" => "🔴",
        "warning" | "p2" => "🟡",
        "info" | "p3" => "🔵",
        _ => "⚪",
    };

    format!(
        "{} <b>SYSTEM ALERT [{}]</b>\n\n{}",
        icon,
        TelegramBot::escape_html(severity),
        TelegramBot::escape_html(message),
    )
}
```

## File: src/telegram/mod.rs
```rust
pub mod bot;
pub mod alerts;
pub mod reports;
```

## File: deploy/Dockerfile
```
FROM rust:1.82-bookworm AS builder
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
EXPOSE 9090
CMD ["/opt/mercury/mercury", "--config", "/opt/mercury/config/default.yaml"]
```

## File: src/crypto/eip712.rs
```rust
use anyhow::{Context, Result};
use alloy::primitives::{Address, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::{sol, Eip712Domain};
use std::str::FromStr;
use tracing::info;
use zeroize::Zeroizing;

sol! {
    #[derive(Debug)]
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        address taker;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint256 expiration;
        uint256 nonce;
        uint256 feeRateBps;
        uint8 side;
        uint8 signingScheme;
    }
}

/// Polymarket CLOB order signer
#[derive(Clone)]
pub struct PolymarketSigner {
    wallet: PrivateKeySigner,
    chain_id: u64,
    verifying_contract: Address,
    domain: Eip712Domain,
}

impl PolymarketSigner {
    /// Create from raw private key bytes
    pub fn new(private_key_bytes: &[u8], chain_id: u64, verifying_contract: Address) -> Result<Self> {
        let wallet = PrivateKeySigner::from_slice(private_key_bytes)
            .context("Failed to create wallet from private key")?
            .with_chain_id(Some(chain_id));

        // C-5 FIX: Polymarket CTF Exchange expects this specific salt
        let salt_bytes = hex::decode("251543d4af9c5b206ce59ec472b535d94726cd55b6bd05eb0ebdc86dfcbac0e3").unwrap_or_default();
        let salt = alloy::primitives::B256::from_slice(&salt_bytes);

        let domain = Eip712Domain::new(
            Some("Polymarket CTF Exchange".into()),
            Some("1".into()),
            Some(U256::from(chain_id)),
            Some(verifying_contract),
            Some(salt),
        );

        info!(address = %wallet.address(), chain_id, "Polymarket signer initialized");
        Ok(Self { wallet, chain_id, verifying_contract, domain })
    }

    /// Create from hex-encoded private key string
    pub fn from_hex(hex_key: &str, chain_id: u64, verifying_contract: Address) -> Result<Self> {
        let key = hex_key.strip_prefix("0x").unwrap_or(hex_key);
        // Wrap decoded bytes in Zeroizing so they are cleared from memory on drop
        let bytes = Zeroizing::new(hex::decode(key).context("Invalid hex private key")?);
        Self::new(&bytes, chain_id, verifying_contract)
    }

    /// Create with the default Polymarket CTF Exchange contract on Polygon
    pub fn from_hex_default(hex_key: &str, chain_id: u64) -> Result<Self> {
        let verifying_contract = Address::from_str("0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E")
            .context("Invalid default verifying contract address")?;
        Self::from_hex(hex_key, chain_id, verifying_contract)
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    pub fn verifying_contract(&self) -> Address {
        self.verifying_contract
    }

    /// Sign a Polymarket CLOB order
    /// Returns the signature as a hex string
    pub async fn sign_order(
        &self,
        salt: U256,
        maker: Address,
        signer: Address,
        taker: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        expiration: U256,
        nonce: U256,
        fee_rate_bps: U256,
        side: u8,
        signing_scheme: u8,
    ) -> Result<String> {
        let order = Order {
            salt,
            maker,
            signer,
            taker,
            tokenId: token_id,
            makerAmount: maker_amount,
            takerAmount: taker_amount,
            expiration,
            nonce,
            feeRateBps: fee_rate_bps,
            side,
            signingScheme: signing_scheme,
        };

        // CRITICAL FIX: Alloy typed data signing is asynchronous. 
        // We use `sign_typed_data` and `.await` it.
        let signature = self.wallet
            .sign_typed_data(&order, &self.domain)
            .await
            .context("Failed to sign EIP-712 order")?;

        Ok(format!("0x{}", hex::encode(signature.as_bytes())))
    }
}
```

## File: src/crypto/mod.rs
```rust
pub mod eip712;
pub mod jwt;
```

## File: src/engine/market_registry.rs
```rust
use std::collections::HashMap;
use uuid::Uuid;

use crate::types::*;

/// Cross-platform market matching registry
pub struct MarketRegistry {
    markets: HashMap<Uuid, Market>,
    arb_pairs: HashMap<Uuid, Vec<ArbPair>>,
}

#[derive(Debug, Clone)]
pub struct ArbPair {
    pub market_id: Uuid,
    pub platform_a: Platform,
    pub platform_b: Platform,
    pub confidence: f64,
}

impl MarketRegistry {
    pub fn new() -> Self {
        Self {
            markets: HashMap::new(),
            arb_pairs: HashMap::new(), // CRITICAL FIX: Match struct definition
        }
    }

    /// Register a market. Idempotent: re-registering the same market_id updates
    /// the market definition but does not create duplicate arb pairs.
    pub fn register_market(&mut self, market: Market) {
        let market_id = market.unified_id;
        let platforms: Vec<Platform> = market.platforms.keys().cloned().collect();
        let confidence = market.confidence;

        self.markets.insert(market_id, market);

        let mut pairs = Vec::new();
        if confidence >= 0.95 {
            for i in 0..platforms.len() {
                for j in (i + 1)..platforms.len() {
                    pairs.push(ArbPair {
                        market_id,
                        platform_a: platforms[i],
                        platform_b: platforms[j],
                        confidence,
                    });
                }
            }
        }
        
        if !pairs.is_empty() {
            self.arb_pairs.insert(market_id, pairs);
        } else {
            self.arb_pairs.remove(&market_id);
        }
    }

    pub fn get_arb_pairs_for_market(&self, market_id: &Uuid) -> Option<&Vec<ArbPair>> {
        self.arb_pairs.get(market_id)
    }

    pub fn get_market(&self, id: &Uuid) -> Option<&Market> {
        self.markets.get(id)
    }

    pub fn get_platform_info(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformMarketInfo> {
        self.markets.get(market_id)?.platforms.get(platform)
    }
}
```

## File: src/feeds/mod.rs
```rust
pub mod base;
pub mod polymarket;
pub mod kalshi;
pub mod cdna;
pub mod forecastex;
pub mod normalizer;
pub mod discovery;
```

## File: src/inventory/mod.rs
```rust
pub mod positions;
pub mod reconciler;
pub mod settlement;
pub mod unwind_watchdog;
```

## File: src/inventory/positions.rs
```rust
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::db::Database;
use crate::types::*;

pub struct PositionTracker {
    db: Arc<dyn Database>,
    rx: mpsc::Receiver<TradeResult>,
}

impl PositionTracker {
    pub fn new(db: Arc<dyn Database>, rx: mpsc::Receiver<TradeResult>) -> Self {
        Self { db, rx }
    }

    pub async fn run(mut self) {
        info!("Position tracker started");
        while let Some(trade) = self.rx.recv().await {
            if let Err(e) = self.process_trade(&trade).await {
                error!(error = %e, "Failed to process trade for position tracking");
            }
        }
        info!("Position tracker stopped");
    }

    async fn process_trade(&self, trade: &TradeResult) -> Result<()> {
        if trade.status == TradeStatus::Fail && trade.leg_a_size == Decimal::ZERO {
            return Ok(());
        }

        let now = Utc::now();

        // When both legs filled, write them in a single atomic transaction so the
        // DB never reflects a partial position pair.
        match (trade.leg_a_size > Decimal::ZERO, trade.leg_b_size > Decimal::ZERO) {
            (true, true) => {
                let pos_a = Position {
                    id: 0,
                    market_id: trade.market_id,
                    platform: trade.leg_a_platform,
                    side: trade.leg_a_side,
                    quantity: trade.leg_a_size,
                    avg_entry_price: trade.leg_a_fill_price,
                    unrealized_pnl: Decimal::ZERO,
                    opened_at: now,
                    updated_at: now,
                };
                let pos_b = Position {
                    id: 0,
                    market_id: trade.market_id,
                    platform: trade.leg_b_platform,
                    side: trade.leg_b_side,
                    quantity: trade.leg_b_size,
                    avg_entry_price: trade.leg_b_fill_price,
                    unrealized_pnl: Decimal::ZERO,
                    opened_at: now,
                    updated_at: now,
                };
                self.db.upsert_position_pair(&pos_a, &pos_b).await?;
            }
            (true, false) => {
                let pos_a = Position {
                    id: 0,
                    market_id: trade.market_id,
                    platform: trade.leg_a_platform,
                    side: trade.leg_a_side,
                    quantity: trade.leg_a_size,
                    avg_entry_price: trade.leg_a_fill_price,
                    unrealized_pnl: Decimal::ZERO,
                    opened_at: now,
                    updated_at: now,
                };
                self.db.upsert_position(&pos_a).await?;
            }
            (false, true) => {
                let pos_b = Position {
                    id: 0,
                    market_id: trade.market_id,
                    platform: trade.leg_b_platform,
                    side: trade.leg_b_side,
                    quantity: trade.leg_b_size,
                    avg_entry_price: trade.leg_b_fill_price,
                    unrealized_pnl: Decimal::ZERO,
                    opened_at: now,
                    updated_at: now,
                };
                self.db.upsert_position(&pos_b).await?;
            }
            (false, false) => {}
        }

        let leg_a_cost = trade.leg_a_fill_price * trade.leg_a_size + trade.leg_a_fee;
        let leg_b_cost = trade.leg_b_fill_price * trade.leg_b_size + trade.leg_b_fee;

        let audit = AuditEntry {
            timestamp_ns: now_ns(),
            module: "inventory".into(),
            event_type: "trade_booked".into(),
            data: serde_json::json!({
                "trade_id": trade.trade_id,
                "leg_a_debit": leg_a_cost.to_string(),
                "leg_b_debit": leg_b_cost.to_string(),
                "total_debit": (leg_a_cost + leg_b_cost).to_string(),
                "profit": trade.profit.to_string(),
            }),
        };
        self.db.append_audit(&audit).await?;

        info!(trade_id = trade.trade_id, leg_a_cost = %leg_a_cost, leg_b_cost = %leg_b_cost, "Position booked");
        Ok(())
    }
}
```

## File: src/inventory/unwind_watchdog.rs
```rust
//! Periodic watchdog that detects orphaned (one-sided) positions and escalates.
//!
//! An orphaned position occurs when one leg of an arb fills but the hedge leg
//! fails and the automatic unwind also fails. These positions bleed money as
//! the market moves. The watchdog scans every N seconds and alerts.

use anyhow::Result;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

use crate::db::Database;
use crate::types::*;
use crate::execution::executor::{PlatformOrderClient, OrderAction};
use crate::execution::polymarket_client::PolymarketClient;
use crate::execution::kalshi_client::KalshiClient;
use crate::execution::cdna_client::CdnaClient;
use crate::execution::forecastex_client::ForecastExClient;

pub struct UnwindWatchdog {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    check_interval: Duration,
    polymarket: Option<PolymarketClient>,
    kalshi: Option<KalshiClient>,
    cdna: Option<CdnaClient>,
    forecastex: Option<ForecastExClient>,
}

impl UnwindWatchdog {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        check_interval_secs: u64,
        polymarket: Option<PolymarketClient>,
        kalshi: Option<KalshiClient>,
        cdna: Option<CdnaClient>,
        forecastex: Option<ForecastExClient>,
    ) -> Self {
        Self {
            db,
            alert_tx,
            check_interval: Duration::from_secs(check_interval_secs),
            polymarket,
            kalshi,
            cdna,
            forecastex,
        }
    }

    pub async fn run(self) {
        info!("Unwind watchdog started");
        let mut interval = tokio::time::interval(self.check_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.check_orphaned_positions().await {
                error!(error = %e, "Unwind watchdog check failed");
            }
        }
    }

    async fn check_orphaned_positions(&self) -> Result<()> {
        let positions = self.db.get_open_positions().await?;

        // Group open positions by market_id.
        let mut by_market: HashMap<Uuid, Vec<Position>> = HashMap::new();
        for pos in positions {
            by_market.entry(pos.market_id).or_default().push(pos);
        }

        for (market_id, legs) in &by_market {
            // CRITICAL FIX: Accumulate net exposure across ALL positions.
            // If the bot runs multiple arbs on the same market, legs.len() could be 4, 5, or 6.
            let mut total_yes = Decimal::ZERO;
            let mut total_no = Decimal::ZERO;

            for pos in legs {
                match pos.side {
                    Side::Yes => total_yes += pos.quantity,
                    Side::No => total_no += pos.quantity,
                }
            }

            let unhedged_diff = (total_yes - total_no).abs();

            if unhedged_diff > Decimal::ZERO {
                // Find the oldest position to correctly calculate the age of the imbalance
                let oldest = legs.iter().map(|p| p.opened_at).min().unwrap_or_else(chrono::Utc::now);
                let age = chrono::Utc::now() - oldest;

                if age.num_minutes() > 5 {
                    error!(
                        market_id = %market_id,
                        unhedged_quantity = %unhedged_diff,
                        age_minutes = age.num_minutes(),
                        "ORPHANED POSITION DETECTED — {} contracts unhedged for {} minutes",
                        unhedged_diff,
                        age.num_minutes()
                    );

                    let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                        severity: "critical".into(),
                        message: format!(
                            "🚨 ORPHANED POSITION IMBALANCE: {} contracts unhedged \
                             open for {} minutes. Attempting automated liquidation. \
                             Market ID: {}",
                            unhedged_diff,
                            age.num_minutes(),
                            market_id,
                        ),
                    }).await;

                    // L-3 FIX: Automated liquidation attempt
                    if let Ok(Some(market)) = self.db.get_market(market_id).await {
                        let (target_platform, target_side) = if total_yes > total_no {
                            (legs.iter().find(|p| p.side == Side::Yes).map(|p| p.platform), Side::Yes)
                        } else {
                            (legs.iter().find(|p| p.side == Side::No).map(|p| p.platform), Side::No)
                        };

                        if let Some(plat) = target_platform {
                            if let Some(info) = market.platforms.get(&plat) {
                                let result = match plat {
                                    Platform::Polymarket | Platform::PolymarketUs => {
                                        if let Some(c) = &self.polymarket {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, rust_decimal_macros::dec!(0.01), unhedged_diff, info.fee_rate_bps as u32).await)
                                        } else { None }
                                    },
                                    Platform::Kalshi => {
                                        if let Some(c) = &self.kalshi {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, rust_decimal_macros::dec!(0.01), unhedged_diff, info.fee_rate_bps as u32).await)
                                        } else { None }
                                    },
                                    Platform::Cdna => {
                                        if let Some(c) = &self.cdna {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, rust_decimal_macros::dec!(0.01), unhedged_diff, info.fee_rate_bps as u32).await)
                                        } else { None }
                                    },
                                    Platform::ForecastEx => None,
                                };

                                if let Some(Ok(res)) = result {
                                    if res.filled {
                                        let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                                            severity: "warning".into(),
                                            message: format!("✅ Automated liquidation successful. Filled {} contracts on {}.", res.fill_size, plat),
                                        }).await;
                                        for pos in legs {
                                            let _ = self.db.close_position(pos.id).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
```

## File: src/monitoring/health.rs
```rust
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncBufReadExt};
use tokio::net::TcpListener;
use tracing::{error, info};

use super::metrics::Metrics;

/// Simple HTTP health check endpoint.
/// Returns 200 when feeds are live, 503 when data is stale.
pub async fn run_health_server(port: u16, metrics: Arc<Metrics>, stale_timeout_ms: u64) {
    let addr = format!("127.0.0.1:{}", port); // CRIT-2: Bind to localhost to prevent external info leaks
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
                // L-1 FIX: Robust HTTP header parsing using BufReader instead of fixed buffer
                let mut req_line = String::new();
                let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    let mut reader = tokio::io::BufReader::new(&mut stream);
                    let _ = reader.read_line(&mut req_line).await;
                    let mut header_line = String::new();
                    while let Ok(n) = reader.read_line(&mut header_line).await {
                        if n <= 2 { break; } // \r\n or \n
                        header_line.clear();
                    }
                }).await;

                if !req_line.starts_with("GET ") {
                    let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
                    continue;
                }

                let ms_since_tick = metrics.ms_since_last_tick();
                let is_healthy = ms_since_tick < stale_timeout_ms || (ms_since_tick == u64::MAX && metrics.uptime_secs() < 60);

                let status_text = if is_healthy { "ok" } else { "degraded" };
                let http_status = if is_healthy { "200 OK" } else { "503 Service Unavailable" };

                let body = format!(
                    r#"{{"status":"{}","uptime_secs":{},"ms_since_last_tick":{},"ticks":{},"spreads_evaluated":{},"opportunities_detected":{},"executed":{},"success":{},"failed":{},"ws_reconnects":{},"api_errors":{}}}"#,
                    status_text,
                    metrics.uptime_secs(),
                    ms_since_tick,
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
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    http_status, body.len(), body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
            Err(e) => error!(error = %e, "Health server accept error"),
        }
    }
}
```

## File: src/monitoring/metrics.rs
```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    /// Nanosecond timestamp of the most recent tick received on any feed.
    /// Used by the health endpoint to determine liveness.
    pub last_tick_ns: AtomicU64,
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
            last_tick_ns: AtomicU64::new(0),
        })
    }

    pub fn uptime_secs(&self) -> u64 { self.start_time.elapsed().as_secs() }
    pub fn inc_ticks(&self) {
        self.ticks_received.fetch_add(1, Ordering::Relaxed);
        self.last_tick_ns.store(crate::types::now_ns(), Ordering::Relaxed);
    }
    pub fn inc_spreads(&self) { self.spreads_evaluated.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_detected(&self) { self.opportunities_detected.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_executed(&self) { self.opportunities_executed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_success(&self) { self.trades_success.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_failed(&self) { self.trades_failed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_reconnects(&self) { self.ws_reconnects.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_api_errors(&self) { self.api_errors.fetch_add(1, Ordering::Relaxed); }

    /// Returns milliseconds since the last tick was received, or u64::MAX if none received yet.
    pub fn ms_since_last_tick(&self) -> u64 {
        let last = self.last_tick_ns.load(Ordering::Relaxed);
        if last == 0 {
            return u64::MAX;
        }
        let now = crate::types::now_ns();
        now.saturating_sub(last) / 1_000_000
    }
}
```

## File: src/telegram/reports.rs
```rust
use crate::types::*;
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};
use std::sync::Arc;
use crate::db::Database;

pub struct ReportService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<DailyReport>,
    cmd_tx: mpsc::Sender<SystemCommand>,
    db: Arc<dyn Database>,
}

impl ReportService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<DailyReport>, cmd_tx: mpsc::Sender<SystemCommand>, db: Arc<dyn Database>) -> Self {
        Self { bot, chat_id, rx, cmd_tx, db }
    }

    pub async fn run(mut self) {
        info!("Telegram report & command service started");
        let mut offset = 0;
        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(3)); 
        
        // M-5 FIX: Read TELEGRAM_ADMIN_ID once outside the polling loop
        let admin_id = std::env::var("TELEGRAM_ADMIN_ID").unwrap_or_default();

        loop {
            tokio::select! {
                Some(report) = self.rx.recv() => {
                    let text = format_daily_report(&report);
                    if let Err(e) = self.bot.send_message(&self.chat_id, &text).await {
                        error!(error = %e, "Failed to send daily report");
                    }
                }
                _ = poll_interval.tick() => {
                    if let Ok(updates) = self.bot.get_updates(offset).await {
                        for update in updates {
                            offset = offset.max(update.update_id + 1);
                            
                            // Handle Interactive Button Clicks
                            if let Some(cb) = update.callback_query {
                                let _ = self.bot.answer_callback_query(&cb.id, None).await;
                                
                                if admin_id.is_empty() || cb.from.id.to_string() != admin_id {
                                    continue; 
                                }
                                
                                let data = cb.data.unwrap_or_default();
                                if data == "cmd_start" {
                                    let _ = self.cmd_tx.send(SystemCommand::StartTrading).await;
                                    let _ = self.bot.send_message(&self.chat_id, "▶️ Trading Engine: <b>RESUMED</b>").await;
                                } else if data == "cmd_stop" {
                                    let _ = self.cmd_tx.send(SystemCommand::StopTrading).await;
                                    let _ = self.bot.send_message(&self.chat_id, "⏸️ Trading Engine: <b>HALTED</b>").await;
                                } else if data == "req_report" {
                                    let _ = self.cmd_tx.send(SystemCommand::RequestDailyReport).await;
                                } else if data == "req_suspended" {
                                    if let Ok(suspended) = self.db.get_suspended_markets().await {
                                        if suspended.is_empty() {
                                            let _ = self.bot.send_message(&self.chat_id, "✅ No suspended markets requiring manual review.").await;
                                        } else {
                                            for m in suspended.into_iter().take(5) {
                                                let msg = format!("<b>Suspended Market:</b>\n{}\nConfidence: {}", TelegramBot::escape_html(&m.question), m.confidence);
                                                let markup = serde_json::json!({
                                                    "inline_keyboard": [[{"text": "⚡ Activate Market", "callback_data": format!("activate_{}", m.unified_id)}]]
                                                });
                                                let _ = self.bot.send_message_with_markup(&self.chat_id, &msg, Some(markup)).await;
                                            }
                                        }
                                    }
                                } else if data == "req_trades" {
                                    let today = chrono::Utc::now().date_naive();
                                    if let Ok(mut trades) = self.db.get_trades_for_date(today).await {
                                        if trades.is_empty() {
                                            let _ = self.bot.send_message(&self.chat_id, "ℹ️ No trades executed today.").await;
                                        } else {
                                            trades.sort_by(|a, b| b.executed_at.cmp(&a.executed_at));
                                            let mut msg = format!("📜 <b>Recent Trades (Total Today: {})</b>\n\n", trades.len());
                                            for t in trades.into_iter().take(5) {
                                                let sign = if t.profit >= rust_decimal::Decimal::ZERO { "+" } else { "" };
                                                msg.push_str(&format!("<b>#{}</b>: {}${} | {}\n", t.trade_id, sign, t.profit.round_dp(2), truncate_question(&t.market_question, 30)));
                                            }
                                            let _ = self.bot.send_message(&self.chat_id, &msg).await;
                                        }
                                    }
                                } else if data == "poly_on" {
                                    let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Polymarket)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "✅ Polymarket routing enabled").await;
                                } else if data == "poly_off" {
                                    let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Polymarket)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "❌ Polymarket routing disabled").await;
                                } else if data == "kalshi_on" {
                                    let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Kalshi)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "✅ Kalshi routing enabled").await;
                                } else if data == "kalshi_off" {
                                    let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Kalshi)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "❌ Kalshi routing disabled").await;
                                } else if data.starts_with("activate_") {
                                    let id_str = &data["activate_".len()..];
                                    if let Ok(uuid) = uuid::Uuid::parse_str(id_str) {
                                        let _ = self.cmd_tx.send(SystemCommand::ActivateMarket(uuid)).await;
                                        let _ = self.bot.send_message(&self.chat_id, &format!("✅ Activation command sent for:\n<code>{}</code>", id_str)).await;
                                    }
                                }
                                continue;
                            }

                            // Handle Standard Text Commands
                            if let Some(msg) = update.message {
                                if msg.chat.id.to_string() != self.chat_id { continue; } 
                                
                                if !admin_id.is_empty() {
                                    let sender_id = msg.from.map(|u| u.id.to_string()).unwrap_or_default();
                                    if sender_id != admin_id { continue; }
                                }

                                if let Some(text) = msg.text {
                                    if text.to_lowercase() == "/menu" {
                                        self.send_main_menu().await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn send_main_menu(&self) {
        let markup = serde_json::json!({
            "inline_keyboard": [
                [
                    {"text": "▶️ Start Trading", "callback_data": "cmd_start"},
                    {"text": "⏸️ Stop Trading", "callback_data": "cmd_stop"}
                ],
                [
                    {"text": "📊 Live Financial Report", "callback_data": "req_report"}
                ],
                [
                    {"text": "🔍 Suspended Markets", "callback_data": "req_suspended"},
                    {"text": "📜 Recent Trades", "callback_data": "req_trades"}
                ],
                [
                    {"text": "🟢 Poly ON", "callback_data": "poly_on"},
                    {"text": "🔴 Poly OFF", "callback_data": "poly_off"}
                ],
                [
                    {"text": "🟢 Kalshi ON", "callback_data": "kalshi_on"},
                    {"text": "🔴 Kalshi OFF", "callback_data": "kalshi_off"}
                ]
            ]
        });

        let _ = self.bot.send_message_with_markup(
            &self.chat_id,
            "🎛️ <b>MERCURY Control Panel</b>\nSelect an operation below:",
            Some(markup)
        ).await;
    }
}

fn format_daily_report(report: &DailyReport) -> String {
    let s = &report.snapshot;
    // CRITICAL FIX: ROI must be calculated against the STARTING bankroll, not the ending bankroll.
    let starting_bankroll = s.bankroll - s.net_pnl;
    let roi = if starting_bankroll > Decimal::ZERO {
        (s.net_pnl / starting_bankroll * Decimal::from(100)).round_dp(2)
    } else {
        Decimal::ZERO
    };

    let pnl_sign = if s.net_pnl >= Decimal::ZERO { "+" } else { "" };

    let mut msg = format!(
        "📈 <b>MERCURY DAILY REPORT — {date}</b>\n\
         \n\
         \u{2550}\u{2550}\u{2550} P&amp;L Summary \u{2550}\u{2550}\u{2550}\n\
         Gross Profit:    {pnl_sign}${gross}\n\
         Total Fees:      -${fees}\n\
         <b>Net Profit:      {pnl_sign}${net}</b>\n\
         ROI Today:       {pnl_sign}{roi}%\n\
         \n\
         \u{2550}\u{2550}\u{2550} Trading Activity \u{2550}\u{2550}\u{2550}\n\
         Opportunities Executed:  {total}\n\
         Successful Trades:       {success} ({rate}%)\n\
         Failed/Loss Trades:      {fail}\n",
        date = s.date,
        gross = s.gross_pnl.round_dp(2).abs(),
        fees = s.fees_paid.round_dp(2),
        net = s.net_pnl.round_dp(2).abs(),
        total = s.trades_count,
        success = s.success_count,
        fail = s.fail_count,
        rate = (s.success_rate * Decimal::from(100)).round_dp(1),
    );

    // Platform breakdown
    msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Platform Breakdown \u{2550}\u{2550}\u{2550}\n"));
    for (platform, stats) in &report.platform_breakdown {
        let p_sign = if stats.pnl >= Decimal::ZERO { "+" } else { "" };
        msg.push_str(&format!(
            "{}: ${} exposed \u{2502} {} trades \u{2502} {}${}\n",
            platform,
            stats.exposure.round_dp(2),
            stats.trade_count,
            p_sign,
            stats.pnl.round_dp(2),
        ));
    }

    // Risk metrics
    msg.push_str(&format!(
        "\n\u{2550}\u{2550}\u{2550} Risk Metrics \u{2550}\u{2550}\u{2550}\n\
         Bankroll:         ${bankroll}\n\
         Peak Bankroll:    ${peak}\n\
         Drawdown:         {dd}%\n\
         Kelly Utilization: {kelly}\n",
        bankroll = s.bankroll.round_dp(2),
        peak = s.peak_bankroll.round_dp(2),
        dd = s.drawdown_pct.round_dp(2),
        kelly = s.kelly_utilization.round_dp(2),
    ));

    // Top trades
    if !report.top_trades.is_empty() {
        msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Top Trades \u{2550}\u{2550}\u{2550}\n"));
        for (i, t) in report.top_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            msg.push_str(&format!(
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}\u{2194}{}\n",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                t.leg_a_platform,
                t.leg_b_platform,
            ));
        }
    }

    if !report.worst_trades.is_empty() {
        msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Worst Trades \u{2550}\u{2550}\u{2550}\n"));
        for (i, t) in report.worst_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            let reason = t.failure_reason.as_deref().unwrap_or("n/a");
            msg.push_str(&format!(
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}\n",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                reason,
            ));
        }
    }

    // System health
    let hours = report.uptime_secs / 3600;
    let mins = (report.uptime_secs % 3600) / 60;
    let db_mb = report.db_size_bytes as f64 / 1_048_576.0;
    msg.push_str(&format!(
        "\n\u{2550}\u{2550}\u{2550} System Health \u{2550}\u{2550}\u{2550}\n\
         Uptime: {}h {}m\n\
         WS Reconnects: {}\n\
         API Errors: {}\n\
         DB Size: {:.1}MB",
        hours, mins,
        report.ws_reconnects,
        report.api_errors,
        db_mb,
    ));

    msg
}

fn truncate_question(q: &str, max_len: usize) -> String {
    if max_len <= 3 || q.len() <= max_len { 
        return TelegramBot::escape_html(q); 
    }
    let mut end = max_len - 3;
    while end > 0 && !q.is_char_boundary(end) { end -= 1; }
    let truncated = format!("{}...", &q[..end]);
    TelegramBot::escape_html(&truncated)
}
```

## File: config/default.yaml
```yaml
trading:
  kelly_fraction_multiplier: 0.25
  min_net_spread_threshold: "0.02"
  max_single_trade_pct: "0.02"
  max_daily_loss_pct: "0.03"
  max_drawdown_pct: "0.10"
  max_platform_exposure_pct: "0.40"
  stale_data_timeout_ms: 5000
  max_concurrent_arbs: 3
  gas_price_max_gwei: 50
  rebalance_threshold_pct: "0.15"
  max_open_positions: 20
  initial_bankroll: "10000.00"

platforms:
  polymarket:
    enabled: true
    rest_url: "https://clob.polymarket.com"
    ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market"
    sports_ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/sports"
  kalshi:
    enabled: true
    rest_url: "https://trading-api.kalshi.com/trade-api/v2"
    ws_url: "wss://trading-api.kalshi.com/trade-api/ws/v2"
  cdna:
    enabled: false
    rest_url: ""
    ws_url: ""
  forecastex:
    enabled: false
    fix_host: ""
    fix_port: 0

database:
  path: "/opt/mercury/data/mercury.db"
  pool_size: 4
  busy_timeout_ms: 5000

telegram:
  enabled: false
  daily_report_hour_utc: 21

polygon_rpc:
  url: "https://polygon-rpc.com"
  gas_poll_interval_secs: 15

logging:
  level: "info"
  file: "logs/mercury.log"

health:
  port: 9090
```

## File: src/crypto/jwt.rs
```rust
use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct KalshiClaims {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Kalshi API JWT authenticator
pub struct KalshiAuth {
    api_key_id: String,
    encoding_key: EncodingKey,
    /// Cached token: (token_string, expiry_timestamp_secs)
    token_cache: Mutex<Option<(String, u64)>>,
}

impl KalshiAuth {
    /// Create from RSA private key PEM and API key ID
    pub fn new(api_key_id: String, rsa_private_key_pem: &[u8]) -> Result<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(rsa_private_key_pem)
            .context("Failed to parse RSA private key PEM for Kalshi")?;
        info!(api_key_id = %api_key_id, "Kalshi JWT auth initialized");
        Ok(Self {
            api_key_id,
            encoding_key,
            token_cache: Mutex::new(None),
        })
    }

    /// Return current Unix timestamp in seconds, falling back to 0 on clock error.
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Generate a fresh JWT token (valid for 10 minutes)
    pub fn generate_token(&self) -> Result<String> {
        let now = Self::now_secs();
        let claims = KalshiClaims {
            sub: self.api_key_id.clone(),
            iat: now,
            exp: now + 600,
        };
        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &self.encoding_key)
            .context("Failed to encode JWT")?;
        Ok(token)
    }

    /// Generate Authorization header value, using a cached token when still valid.
    pub async fn auth_header(&self) -> Result<String> {
        let now = Self::now_secs();
        {
            let cache = self.token_cache.lock().await;
            if let Some((ref cached_token, expiry)) = *cache {
                if expiry > now + 30 {
                    return Ok(format!("Bearer {}", cached_token));
                }
            }
        }

        let token = self.generate_token()?;
        let expiry = now + 600;
        {
            let mut cache = self.token_cache.lock().await;
            *cache = Some((token.clone(), expiry));
        }
        Ok(format!("Bearer {}", token))
    }
}
```

## File: src/db/migrations.rs
```rust
use anyhow::Result;
use sqlx::SqlitePool;

/// Current schema version.
pub const CURRENT_VERSION: u32 = 1;

/// Rollback migrations for safety
pub async fn rollback_migrations(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DROP TABLE IF EXISTS config_history").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS audit_log").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS daily_snapshots").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS platform_balances").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS positions").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS trades").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS markets").execute(&mut *tx).await?;
    sqlx::query("DROP TABLE IF EXISTS schema_version").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Run all migrations up to CURRENT_VERSION.
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );"
    ).execute(pool).await?;

    let version: u32 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);

    if version < 1 {
        let mut tx = pool.begin().await?;
        // sqlx allows multiple statements in one query execution
        sqlx::query(MIGRATION_V1).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_version (version) VALUES (1)").execute(&mut *tx).await?;
        tx.commit().await?;
    }

    Ok(())
}

const MIGRATION_V1: &str = r#"
-- Markets
CREATE TABLE IF NOT EXISTS markets (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    unified_id          TEXT UNIQUE NOT NULL,
    question            TEXT NOT NULL,
    resolution_source   TEXT,
    expiration          TEXT,
    platforms           TEXT NOT NULL DEFAULT '{}',
    category            TEXT,
    confidence          REAL NOT NULL DEFAULT 1.0,
    status              TEXT NOT NULL DEFAULT 'active',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_markets_unified ON markets(unified_id);
CREATE INDEX IF NOT EXISTS idx_markets_status ON markets(status);

-- Trades
CREATE TABLE IF NOT EXISTS trades (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    opp_id              TEXT NOT NULL,
    market_id           TEXT NOT NULL,
    market_question     TEXT NOT NULL DEFAULT '',
    leg_a_platform      TEXT NOT NULL,
    leg_a_side          TEXT NOT NULL,
    leg_a_price         TEXT NOT NULL,
    leg_a_size          TEXT NOT NULL,
    leg_a_fill_price    TEXT NOT NULL DEFAULT '0',
    leg_a_fee           TEXT NOT NULL DEFAULT '0',
    leg_b_platform      TEXT NOT NULL,
    leg_b_side          TEXT NOT NULL,
    leg_b_price         TEXT NOT NULL,
    leg_b_size          TEXT NOT NULL,
    leg_b_fill_price    TEXT NOT NULL DEFAULT '0',
    leg_b_fee           TEXT NOT NULL DEFAULT '0',
    raw_spread          TEXT NOT NULL,
    net_spread          TEXT NOT NULL,
    profit              TEXT NOT NULL,
    status              TEXT NOT NULL,
    failure_reason      TEXT,
    execution_ms        INTEGER NOT NULL DEFAULT 0,
    executed_at         TEXT NOT NULL,
    bankroll_after      TEXT NOT NULL,
    bankroll_change_pct TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_trades_market ON trades(market_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_trades_opp_id ON trades(opp_id);
CREATE INDEX IF NOT EXISTS idx_trades_executed ON trades(executed_at);
CREATE INDEX IF NOT EXISTS idx_trades_status ON trades(status);

-- Positions
CREATE TABLE IF NOT EXISTS positions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id       TEXT NOT NULL,
    platform        TEXT NOT NULL,
    side            TEXT NOT NULL,
    quantity        TEXT NOT NULL,
    avg_entry_price TEXT NOT NULL,
    unrealized_pnl  TEXT NOT NULL DEFAULT '0',
    opened_at       TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    closed          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(closed) WHERE closed = 0;
CREATE INDEX IF NOT EXISTS idx_positions_market ON positions(market_id);

-- Platform balances
CREATE TABLE IF NOT EXISTS platform_balances (
    platform            TEXT PRIMARY KEY,
    available           TEXT NOT NULL DEFAULT '0',
    reserved            TEXT NOT NULL DEFAULT '0',
    pending_settlement  TEXT NOT NULL DEFAULT '0',
    total               TEXT NOT NULL DEFAULT '0',
    updated_at          TEXT NOT NULL
);

-- Daily snapshots
CREATE TABLE IF NOT EXISTS daily_snapshots (
    date                TEXT PRIMARY KEY,
    bankroll            TEXT NOT NULL,
    gross_pnl           TEXT NOT NULL DEFAULT '0',
    fees_paid           TEXT NOT NULL DEFAULT '0',
    net_pnl             TEXT NOT NULL DEFAULT '0',
    trades_count        INTEGER NOT NULL DEFAULT 0,
    success_count       INTEGER NOT NULL DEFAULT 0,
    fail_count          INTEGER NOT NULL DEFAULT 0,
    success_rate        TEXT NOT NULL DEFAULT '0',
    peak_bankroll       TEXT NOT NULL DEFAULT '0',
    drawdown_pct        TEXT NOT NULL DEFAULT '0',
    kelly_utilization   TEXT NOT NULL DEFAULT '0',
    report_sent         INTEGER NOT NULL DEFAULT 0
);

-- Audit log
CREATE TABLE IF NOT EXISTS audit_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,
    module       TEXT NOT NULL,
    event_type   TEXT NOT NULL,
    data         TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(timestamp_ns);

-- Config history
CREATE TABLE IF NOT EXISTS config_history (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    changed_at TEXT NOT NULL,
    key       TEXT NOT NULL,
    old_value TEXT NOT NULL,
    new_value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_config_hist_ts ON config_history(changed_at);
"#;
```

## File: src/feeds/base.rs
```rust
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::types::{NormalizedTick, Platform};

#[async_trait]
pub trait FeedHandler: Send + Sync + 'static {
    fn platform(&self) -> Platform;

    /// Connect and start processing. Should run until disconnected.
    /// Returns Err on fatal error, Ok(()) on clean disconnect.
    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()>;

    /// Clear all local order book state.
    ///
    /// Called immediately on every disconnect (clean or error) before the reconnect
    /// backoff begins. This prevents stale book data from being visible to the spread
    /// engine during the reconnect window. The first snapshot received after reconnect
    /// will repopulate the books from authoritative exchange state.
    fn clear_books(&mut self);
}

/// Run a feed handler with automatic reconnection.
/// The loop exits cleanly when `token` is cancelled.
pub async fn run_with_reconnect(
    mut handler: Box<dyn FeedHandler>,
    tick_tx: broadcast::Sender<NormalizedTick>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
    metrics: std::sync::Arc<crate::monitoring::metrics::Metrics>,
    token: CancellationToken,
) {
    let platform = handler.platform();
    let mut backoff_secs = 1u64;
    // 5 s maximum backoff — 60 s is an eternity for an HFT engine.
    // During a 60 s blind window, resting limit orders become stale and
    // can be sniped by other bots or filled at unfavorable prices.
    let max_backoff = 5u64;

    loop {
        info!(%platform, "Connecting feed handler");

        tokio::select! {
            _ = token.cancelled() => {
                info!(%platform, "Feed handler cancelled via token");
                break;
            }
            result = handler.connect_and_run(tick_tx.clone()) => {
                // Immediately discard all book state so no stale prices are visible
                // to the spread engine during the reconnect window. The first snapshot
                // after reconnect will rebuild from authoritative exchange data.
                handler.clear_books();
                match result {
                    Ok(()) => {
                        info!(%platform, "Feed handler disconnected cleanly — books cleared");
                        backoff_secs = 1;
                    }
                    Err(e) => {
                        error!(%platform, error = %e, "Feed handler error — books cleared");
                        // Alert IMMEDIATELY on disconnect — the orchestrator must
                        // halt trading on this platform until the book is rebuilt.
                        // Use try_send (non-blocking) to avoid blocking the reconnect loop.
                        let _ = alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                            severity: "critical".into(),
                            message: format!(
                                "{platform} feed DISCONNECTED — market data is stale, \
                                 trading halted on this platform until reconnect and book rebuild: {e}"
                            ),
                        });
                    }
                }
            }
        }

        // Check again before sleeping so a cancellation during backoff exits promptly.
        tokio::select! {
            _ = token.cancelled() => {
                info!(%platform, "Feed handler cancelled during backoff");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
        }

        warn!(%platform, backoff_secs, "Reconnecting after backoff");
        // L-7 FIX: Report reconnect metrics
        metrics.inc_reconnects();
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}
```

## File: src/inventory/reconciler.rs
```rust
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::db::Database;
use crate::types::*;
use crate::execution::kalshi_client::KalshiClient;
use crate::execution::polymarket_client::PolymarketClient;

pub struct Reconciler {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    kalshi_client: Option<KalshiClient>,
    polymarket_client: Option<PolymarketClient>,
    interval: Duration,
    threshold: Decimal,
}

impl Reconciler {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        kalshi_client: Option<KalshiClient>,
        polymarket_client: Option<PolymarketClient>,
        interval_secs: u64,
        threshold: Decimal,
    ) -> Self {
        Self { db, alert_tx, kalshi_client, polymarket_client, interval: Duration::from_secs(interval_secs), threshold }
    }

    pub async fn run(self) {
        info!(interval_secs = self.interval.as_secs(), "Reconciler started");
        let mut interval = tokio::time::interval(self.interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.reconcile().await {
                error!(error = %e, "Reconciliation cycle failed");
            }
        }
    }

    async fn reconcile(&self) -> Result<()> {
        // MED-7: Proactive DB connection ping
        let _ = self.db.db_size_bytes().await.context("Database health check ping failed")?;

        let positions = self.db.get_open_positions().await?;
        let balances = self.db.get_all_balances().await?;

        let audit = AuditEntry {
            timestamp_ns: now_ns(),
            module: "reconciler".into(),
            event_type: "reconciliation_cycle".into(),
            data: serde_json::json!({
                "open_positions": positions.len(),
                "platforms_with_balance": balances.len(),
            }),
        };
        self.db.append_audit(&audit).await?;

        for pos in &positions {
            let age = chrono::Utc::now() - pos.opened_at;
            if age.num_days() > 30 {
                warn!(position_id = pos.id, market_id = %pos.market_id, age_days = age.num_days(), "Stale position detected");
                let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                    severity: "critical".into(),
                    message: format!(
                        "🚨 STALE POSITION: Position #{} on {} has been open for {} days. Manual resolution required. Market ID: {}",
                        pos.id, pos.platform, age.num_days(), pos.market_id
                    ),
                }).await;
            }
        }

        // Active API Reconciliation (CRITICAL FIX 3-C)
        if let Some(kalshi) = &self.kalshi_client {
            if let Ok(live) = kalshi.get_balance().await {
                let db_bal = balances.iter().find(|b| b.platform == Platform::Kalshi).map(|b| b.total).unwrap_or(Decimal::ZERO);
                if (live - db_bal).abs() > self.threshold {
                    warn!(live = %live, db = %db_bal, "Kalshi balance mismatch");
                    let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("Kalshi Balance Mismatch: API=${live}, DB=${db_bal}"),
                    }).await;
                }
            }
        }

        if let Some(poly) = &self.polymarket_client {
            if let Ok(live) = poly.get_balance().await {
                let db_bal = balances.iter().find(|b| b.platform == Platform::Polymarket).map(|b| b.total).unwrap_or(Decimal::ZERO);
                if (live - db_bal).abs() > self.threshold {
                    warn!(live = %live, db = %db_bal, "Polymarket balance mismatch");
                    let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("Polymarket Balance Mismatch: API=${live}, DB=${db_bal}"),
                    }).await;
                }
            }
        }

        let total_position_value: Decimal = positions.iter()
            .map(|p| p.quantity * p.avg_entry_price)
            .sum();

        // Enforce DB TTL pruning to prevent unbounded disk growth (Issue #8)
        if let Err(e) = self.db.prune_audit_log(7).await {
            warn!(error = %e, "Failed to prune audit log during reconciliation cycle");
        }

        // MED-9 FIX: Bound WAL file growth periodically
        let _ = self.db.checkpoint_wal().await;

        info!(open_positions = positions.len(), total_position_value = %total_position_value.round_dp(2), "Reconciliation complete");
        Ok(())
    }
}
```

## File: src/monitoring/gas_oracle.rs
```rust
use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::{interval_at, Duration, Instant};
use tracing::{debug, error, info, warn};

/// Sent to the main event loop whenever live prices are refreshed.
#[derive(Debug, Clone)]
pub struct GasUpdate {
    /// Polygon network gas price in gwei.
    pub gas_gwei: u64,
    /// MATIC/USD spot price.
    pub matic_usd: Decimal,
}

pub struct GasOracle {
    rpc_url: String,
    poll_interval_secs: u64,
    update_tx: mpsc::Sender<GasUpdate>,
    http: reqwest::Client,
}

// ── JSON shapes for eth_gasPrice RPC ───────────────────────────────────────

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<String>,
    /// JSON-RPC error object returned with HTTP 200 on RPC-level errors.
    /// Must be captured to produce useful diagnostics instead of "null result".
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}


/// Alert if this many consecutive CoinGecko fetches fail (rate-limited or down).
const COINGECKO_ALERT_THRESHOLD: u32 = 5;

impl GasOracle {
    pub fn new(
        rpc_url: String,
        poll_interval_secs: u64,
        update_tx: mpsc::Sender<GasUpdate>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("mercury-gas-oracle/1.0")
            .build()
            .expect("failed to build gas oracle HTTP client");
        Self { rpc_url, poll_interval_secs, update_tx, http }
    }

    pub async fn run(self) {
        // Start with sane defaults so the engine is never un-initialised.
        let mut last_gwei: u64 = 50;
        let mut last_matic: Decimal = dec!(0.50);
        let mut coingecko_failures: u32 = 0;

        let poll_secs = self.poll_interval_secs.max(10); // floor at 10s

        // Use interval_at(now) so the FIRST tick fires immediately, pushing live
        // values to the spread engine before any arb detection begins.
        let mut ticker = interval_at(Instant::now(), Duration::from_secs(poll_secs));

        info!(
            interval_secs = poll_secs,
            rpc_url = %self.rpc_url,
            "Gas oracle started (first fetch is immediate)"
        );

        loop {
            ticker.tick().await;

            match self.fetch_gas_gwei().await {
                Ok(gwei) => {
                    if gwei != last_gwei {
                        info!(gwei, prev_gwei = last_gwei, "Polygon gas price updated");
                    } else {
                        debug!(gwei, "Polygon gas price unchanged");
                    }
                    last_gwei = gwei;
                }
                Err(e) => {
                    warn!(error = %e, last_gwei, "Failed to fetch Polygon gas price — keeping last value");
                }
            }

            match self.fetch_matic_usd().await {
                Ok(raw_price) => {
                    // CRITICAL FIX (3-D): Hard floor/ceiling to prevent severe oracle glitches
                    // from multiplying gas estimates by 1000x and breaking the spread math.
                    let price = raw_price.clamp(dec!(0.20), dec!(5.00));
                    
                    let change_pct = if last_matic > Decimal::ZERO {
                        ((price - last_matic) / last_matic * Decimal::from(100)).abs()
                    } else {
                        Decimal::from(100)
                    };
                    if change_pct > dec!(1) {
                        info!(matic_usd = %price, prev = %last_matic, "MATIC/USD price updated");
                    } else {
                        debug!(matic_usd = %price, "MATIC/USD price refreshed");
                    }
                    last_matic = price;
                    coingecko_failures = 0;
                }
                Err(e) => {
                    coingecko_failures += 1;
                    if coingecko_failures >= COINGECKO_ALERT_THRESHOLD {
                        error!(
                            failures = coingecko_failures,
                            last_matic = %last_matic,
                            error = %e,
                            "CoinGecko MATIC/USD fetch has failed {} consecutive times — \
                             gas costs may be stale, arb profitability estimates unreliable",
                            coingecko_failures
                        );
                    } else {
                        warn!(
                            error = %e,
                            last_matic = %last_matic,
                            failures = coingecko_failures,
                            "Failed to fetch MATIC/USD — keeping last value"
                        );
                    }
                }
            }

            let update = GasUpdate { gas_gwei: last_gwei, matic_usd: last_matic };
            // Use try_send (non-blocking) so the oracle never blocks the main loop
            if let Err(e) = self.update_tx.try_send(update) {
                match e {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => {
                        warn!("Gas oracle channel full — skipping update");
                    }
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                        debug!("Gas oracle channel closed — exiting cleanly");
                        return;
                    }
                }
            }
        }
    }

    /// Call `eth_gasPrice` on the configured Polygon RPC endpoint.
    async fn fetch_gas_gwei(&self) -> Result<u64> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_gasPrice",
            "params": [],
            "id": 1
        });

        let resp: RpcResponse = self.http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        // Surface JSON-RPC errors (returned with HTTP 200 by most RPC providers).
        if let Some(rpc_err) = resp.error {
            return Err(anyhow::anyhow!(
                "Polygon RPC error {}: {}",
                rpc_err.code,
                rpc_err.message
            ));
        }

        let hex = resp.result
            .ok_or_else(|| anyhow::anyhow!("eth_gasPrice returned null result (no error field)"))?;

        // Result is a hex string like "0x..." representing wei
        let hex_stripped = hex.trim_start_matches("0x");
        let wei = u64::from_str_radix(hex_stripped, 16)
            .map_err(|e| anyhow::anyhow!("failed to parse gas price hex '{}': {}", hex, e))?;
        let gwei = wei / 1_000_000_000;
        Ok(gwei.max(1)) // floor at 1 gwei to avoid zero gas cost in spread calc
    }

    /// Fetch MATIC/USD (or POL/USD) from CoinGecko's free simple/price endpoint.
    /// Tries both the legacy `matic-network` and new `polygon-ecosystem-token` IDs.
    async fn fetch_matic_usd(&self) -> Result<Decimal> {
        // Try the current ID first, fall back to legacy
        for coin_id in &["polygon-ecosystem-token", "matic-network"] {
            match self.try_coingecko_price(coin_id).await {
                Ok(price) => return Ok(price),
                Err(e) => {
                    tracing::debug!(coin_id, error = %e, "CoinGecko fetch failed, trying next ID");
                }
            }
        }
        // MED-6: Binance fallback for stability
        match self.try_binance_price("MATICUSDT").await {
            Ok(price) => return Ok(price),
            Err(e) => tracing::debug!(error = %e, "Binance fallback failed"),
        }
        Err(anyhow::anyhow!("All price oracles failed for MATIC/POL price"))
    }

    async fn try_binance_price(&self, symbol: &str) -> Result<Decimal> {
        let http_resp = self.http.get("https://api.binance.com/api/v3/ticker/price")
            .query(&[("symbol", symbol)]).send().await?;
        if !http_resp.status().is_success() {
            return Err(anyhow::anyhow!("Binance HTTP {}", http_resp.status()));
        }
        let resp: serde_json::Value = http_resp.json().await?;
        let price_str = resp.get("price").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing price"))?;
        std::str::FromStr::from_str(price_str).map_err(|e| anyhow::anyhow!("Parse error: {}", e))
    }

    async fn try_coingecko_price(&self, coin_id: &str) -> Result<Decimal> {
        let http_resp = self.http
            .get("https://api.coingecko.com/api/v3/simple/price")
            .query(&[("ids", coin_id), ("vs_currencies", "usd")])
            .send()
            .await?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "CoinGecko HTTP {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            ));
        }

        let resp: serde_json::Value = http_resp.json().await?;
        let usd = resp.get(coin_id)
            .and_then(|v| v.get("usd"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("CoinGecko response missing '{}.usd' field", coin_id))?;

        if !usd.is_finite() {
            return Err(anyhow::anyhow!("CoinGecko value is not finite: {}", usd));
        }
        Decimal::from_f64(usd)
            .ok_or_else(|| anyhow::anyhow!("Failed to convert f64 to Decimal: {}", usd))
    }
}
```

## File: src/monitoring/mod.rs
```rust
pub mod metrics;
pub mod health;
pub mod gas_oracle;
pub mod backup;
```

## File: src/risk/kelly.rs
```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Kelly Criterion calculator for arbitrage
pub struct KellyCalculator {
    fraction: Decimal,
    min_fraction: Decimal,
    max_fraction: Decimal,
    config_max_fraction: Decimal,
    pub arb_loss_fraction: Decimal, // LOW-10
    hard_cap: Decimal,
}

impl KellyCalculator {
    pub fn new(fraction: Decimal, hard_cap: Decimal) -> Self {
        Self {
            fraction,
            min_fraction: dec!(0.05),
            max_fraction: fraction,
            config_max_fraction: fraction,
            arb_loss_fraction: dec!(0.005),
            hard_cap,
        }
    }

    pub fn update_fraction(&mut self, fraction: Decimal, hard_cap: Decimal) {
        // MED-9: Do not reset current fraction immediately if in drawdown
        self.config_max_fraction = fraction;
        self.max_fraction = fraction;
        self.hard_cap = hard_cap;
    }

    pub fn set_fraction(&mut self, fraction: Decimal) {
        self.fraction = fraction.max(self.min_fraction).min(self.max_fraction);
    }

    pub fn fraction(&self) -> Decimal {
        self.fraction
    }

    pub fn optimal_fraction(&self, exec_probability: Decimal, net_spread: Decimal) -> Decimal {
        if net_spread <= Decimal::ZERO || exec_probability <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let p = exec_probability;
        let q = Decimal::ONE - p;
        // Arb-aware Kelly: on execution failure we lose only the fees paid on the
        // failed leg (~0.5% of notional), not the full notional.  Using net_spread
        // as `b` in the standard formula (which assumes full-notional loss) produces
        // near-zero fractions for any realistic spread and kills all trading.
        let arb_loss_fraction = self.arb_loss_fraction; // LOW-10
        let full_kelly = (p * net_spread - q * arb_loss_fraction) / (net_spread + arb_loss_fraction);
        if full_kelly <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (full_kelly * self.fraction).max(Decimal::ZERO).min(self.hard_cap)
    }

    pub fn position_size(
        &self,
        bankroll: Decimal,
        exec_probability: Decimal,
        net_spread: Decimal,
        max_single_trade_pct: Decimal,
    ) -> Decimal {
        let kelly_frac = self.optimal_fraction(exec_probability, net_spread);
        let kelly_size = bankroll * kelly_frac;
        let max_size = bankroll * max_single_trade_pct;
        kelly_size.min(max_size).max(Decimal::ZERO)
    }

    pub fn multi_asset_kelly(
        &self,
        opportunities: &[(Decimal, Decimal)],
        max_total_exposure: Decimal,
    ) -> Vec<Decimal> {
        if opportunities.is_empty() {
            return Vec::new();
        }
        let individual: Vec<Decimal> = opportunities.iter()
            .map(|(p, spread)| self.optimal_fraction(*p, *spread))
            .collect();
        let total: Decimal = individual.iter().sum();
        if total <= Decimal::ZERO {
            return vec![Decimal::ZERO; opportunities.len()];
        }
        if total > max_total_exposure {
            let scale = max_total_exposure / total;
            individual.iter().map(|f| *f * scale).collect()
        } else {
            individual
        }
    }

    /// Adjust the Kelly fraction in response to drawdown.
    ///
    /// `current_drawdown_pct` must be **percent-scale** (0–100);
    /// e.g. pass `15.0` for a 15 % drawdown from peak.
    ///
    /// Drawdown tiers reduce sizing; full recovery (≤ 5 %) restores `max_fraction`
    /// so the system does not permanently under-trade after recovering from any loss.
    pub fn adjust_for_drawdown(&mut self, current_drawdown_pct: Decimal) {
        if current_drawdown_pct > dec!(15) {
            self.set_fraction(dec!(0.10));
        } else if current_drawdown_pct > dec!(10) {
            self.set_fraction(dec!(0.15));
        } else if current_drawdown_pct > dec!(5) {
            self.set_fraction(dec!(0.20));
        } else {
            // Drawdown ≤ 5 %: fully recovered — restore to max fraction.
            self.max_fraction = self.config_max_fraction; // MED-9: Restore config ceiling
            self.set_fraction(self.max_fraction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_optimal_fraction() {
        let calc = KellyCalculator::new(dec!(0.10), dec!(0.10));
        // High win prob, positive spread => > 0
        let frac = calc.optimal_fraction(dec!(0.90), dec!(0.05));
        assert!(frac > Decimal::ZERO);
        assert!(frac <= dec!(0.10));

        // Low win prob, negative spread => 0
        let frac_bad = calc.optimal_fraction(dec!(0.10), dec!(-0.05));
        assert_eq!(frac_bad, Decimal::ZERO);
    }
}
```

## File: Cargo.toml
```toml
[package]
name = "mercury"
version = "0.1.0"
edition = "2021"
description = "Cross-market prediction arbitrage engine"

[dependencies]
native-tls = "0.2"
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.21", features = ["native-tls"] }
ring = "0.17"
reqwest = { version = "0.12", features = ["json", "native-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tokio-native-tls = "0.3"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-native-tls", "chrono", "uuid", "rust_decimal"] }
alloy = { version = "0.8.2", features = ["full", "eip712"] }
alloy-sol-types = "0.8.26"
jsonwebtoken = "9"
rsa = { version = "0.9", features = ["pem"] }
rust_decimal = { version = "1", features = ["serde-with-str"] }
rust_decimal_macros = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
notify = "6"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
anyhow = "1"
async-trait = "0.1"
futures-util = "0.3"
hex = "0.4"
sha2 = "0.10"
zeroize = { version = "1", features = ["derive"] }
tokio-util = "0.7"
lru = "0.12"
clap = { version = "4", features = ["derive"] }
tracing-appender = "0.2.4"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

## File: src/execution/cdna_client.rs
```rust
use anyhow::{Context, Result};
use ring::hmac;
use rust_decimal::Decimal;
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::types::Side;

#[derive(Clone)]
pub struct CdnaClient {
    http: reqwest::Client,
    rest_url: String,
    api_key: String,
    api_secret: String,
}

impl CdnaClient {
    pub fn new(rest_url: String, api_key: String, api_secret: String) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5));
            
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = reqwest::tls::Certificate::from_pem(&cert_pem) {
                builder = builder.add_root_certificate(cert);
            }
        }
            
        let http = builder.build().expect("Failed to build CDNA HTTP client");
        Self { http, rest_url, api_key, api_secret }
    }

    /// Build CDNA HMAC-SHA256 authentication headers.
    ///
    /// CDNA signs requests as:
    ///   signature = HMAC-SHA256(api_secret, nonce + method + path + body)
    /// where nonce is a millisecond-precision Unix timestamp string.
    fn auth_headers(&self, method: &str, path: &str, body: &str) -> Result<[(&'static str, String); 3]> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock before Unix epoch")?
            .as_millis()
            .to_string();

        let sign_payload = format!("{}{}{}{}", nonce, method, path, body);
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.api_secret.as_bytes());
        let tag = hmac::sign(&key, sign_payload.as_bytes());
        let signature = hex::encode(tag.as_ref());

        Ok([
            ("X-API-KEY", self.api_key.clone()),
            ("X-ACCESS-NONCE", nonce),
            ("X-ACCESS-SIGN", signature),
        ])
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for CdnaClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
        info!(market_id, action = ?action, side = %side, price = %price, size = %size, "Submitting CDNA order");
        
        // CRITICAL FIX: CDNA uses a single instrument where long = YES, short = NO.
        // To natively sell (unwind) a position, we must inverse the initial order type.
        let side_str = match (action, side) {
            (OrderAction::Buy, Side::Yes) => "BUY",
            (OrderAction::Buy, Side::No) => "SELL",
            (OrderAction::Sell, Side::Yes) => "SELL", // Dump long position
            (OrderAction::Sell, Side::No) => "BUY",   // Cover short position
        };
        
        // CRITICAL FIX: CDNA is priced exclusively in YES terms.
        // If we are Buying NO at 0.40, we must SELL the instrument at 0.60.
        // If we are covering a short (Selling NO aggressively at 0.01), we must BUY the instrument up to 0.99.
        let cdna_price = match (action, side) {
            (OrderAction::Buy, Side::Yes) => price,
            (OrderAction::Buy, Side::No) => Decimal::ONE - price,
            (OrderAction::Sell, Side::Yes) => price, 
            (OrderAction::Sell, Side::No) => Decimal::ONE - price, 
        };
        
        let path = "/private/create-order";
        let url = format!("{}{}", self.rest_url, path);
        let body_json = serde_json::json!({
            "instrument_name": market_id,
            "side": side_str,
            "type": "LIMIT",
            "price": cdna_price.to_string(),
            "quantity": size.to_string(),
            // Ensure FOK so partial fills don't leave orphaned legs
            "time_in_force": "FOK", 
        });
        let body_str = body_json.to_string();
        let auth = self.auth_headers("POST", path, &body_str)?;

        let resp = self.http
            .post(&url)
            .header(auth[0].0, &auth[0].1)
            .header(auth[1].0, &auth[1].1)
            .header(auth[2].0, &auth[2].1)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .context("CDNA order failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: Some(format!("HTTP {}: {}", status, body_text)),
            });
        }

        let result: serde_json::Value = resp.json().await.unwrap_or_default();

        let filled = result.get("result").and_then(|r| r.get("status"))
            .and_then(|s| s.as_str()).map(|s| s == "FILLED").unwrap_or(false);
        let fill_price = result.get("result").and_then(|r| r.get("avg_price"))
            .and_then(|p| p.as_str()).and_then(|s| s.parse::<Decimal>().ok()).unwrap_or(price);

        let api_status = result.get("result").and_then(|r| r.get("status"))
            .and_then(|s| s.as_str()).unwrap_or("UNKNOWN").to_string();

        let fill_size = if filled { size } else { Decimal::ZERO };
        // Compute taker fee from the bps rate supplied by the caller.
        let fee = Decimal::from(fee_rate_bps) / Decimal::from(10_000) * fill_price * fill_size;
        Ok(OrderResult {
            filled,
            fill_price,
            fill_size,
            fee,
            order_id: result.get("result").and_then(|r| r.get("order_id"))
                .and_then(|o| o.as_str()).unwrap_or("").to_string(),
            error: if !filled { Some(api_status) } else { None },
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let path = "/private/cancel-order";
        let url = format!("{}{}", self.rest_url, path);
        let body_str = serde_json::json!({"order_id": order_id}).to_string();
        let auth = self.auth_headers("POST", path, &body_str)?;
        let resp = self.http.post(&url)
            .header(auth[0].0, &auth[0].1)
            .header(auth[1].0, &auth[1].1)
            .header(auth[2].0, &auth[2].1)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .context("CDNA cancel_order send failed")?;
        if !resp.status().is_success() {
            tracing::warn!("CDNA cancel_order returned HTTP {}", resp.status());
        }
        Ok(())
    }
}
```

## File: src/execution/forecastex_client.rs
```rust
use anyhow::Result;
use rust_decimal::Decimal;
use tracing::{info, warn};

use super::executor::{OrderResult, PlatformOrderClient};
use crate::types::Side;

#[derive(Clone)]
pub struct ForecastExClient {
    fix_host: String,
    fix_port: u16,
}

impl ForecastExClient {
    pub fn new(fix_host: String, fix_port: u16) -> Self {
        Self { fix_host, fix_port }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for ForecastExClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, _fee_rate_bps: u32) -> Result<OrderResult> {
        info!(market_id, action = ?action, side = %side, price = %price, size = %size, "ForecastEx order rejected — FIX execution not yet implemented");
        warn!(
            fix_host = %self.fix_host,
            fix_port = self.fix_port,
            "ForecastEx FIX execution not implemented — rejecting order to prevent unhedged leg-A positions"
        );
        // Return Err so the executor aborts the arb opportunity BEFORE executing any
        // counterpart leg. An Ok(filled: false) response only blocks leg-B but still
        // allows leg-A to run, which would require an unwind trade on a real platform.
        anyhow::bail!("ForecastEx FIX execution not yet implemented")
    }

    async fn cancel_order(&self, _order_id: &str) -> Result<()> {
        warn!("ForecastEx cancel not yet implemented");
        Ok(())
    }
}
```

## File: src/feeds/discovery.rs
```rust
//! Market discovery: polls platform REST APIs, matches equivalent markets
//! cross-platform, and registers them in the MarketRegistry.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::PlatformsConfig;
use crate::feeds::normalizer::compute_unified_market_id;
use crate::types::*;

/// Discovered market from a single platform before cross-matching.
#[derive(Debug, Clone)]
struct DiscoveredMarket {
    platform: Platform,
    platform_market_id: String,
    question: String,
    /// Normalized question for fuzzy matching (lowercase, trimmed, stripped punctuation).
    question_normalized: String,
    resolution_source: String,
    expiration: DateTime<Utc>,
    category: MarketCategory,
    fee_rate_bps: u16,
    min_order_size: Decimal,
    tick_size: Decimal,
}

/// Result of the discovery cycle: a fully matched cross-platform market.
#[derive(Debug, Clone)]
pub struct MatchedMarket {
    pub market: Market,
}

pub struct MarketDiscovery {
    platforms_config: PlatformsConfig,
    http: reqwest::Client,
    poll_interval: Duration,
    kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>,
}

impl MarketDiscovery {
    pub fn new(platforms_config: PlatformsConfig, poll_interval_secs: u64, kalshi_auth: Option<crate::crypto::jwt::KalshiAuth>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("mercury-discovery/1.0")
            .build()
            .expect("failed to build discovery HTTP client");
        Self {
            platforms_config,
            http,
            poll_interval: Duration::from_secs(poll_interval_secs),
            kalshi_auth,
        }
    }

    /// Run the discovery loop, sending matched markets to the registry channel.
    pub async fn run(self, matched_tx: mpsc::Sender<MatchedMarket>) {
        info!(
            interval_secs = self.poll_interval.as_secs(),
            "Market discovery started"
        );

        // First fetch is immediate.
        let mut ticker = tokio::time::interval_at(
            tokio::time::Instant::now(),
            self.poll_interval,
        );

        loop {
            ticker.tick().await;

            match self.discover_and_match().await {
                Ok(matched) => {
                    info!(count = matched.len(), "Discovery cycle complete");
                    for m in matched {
                        if let Err(e) = matched_tx.send(m).await {
                            warn!(error = %e, "Matched market channel closed — skipping");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Market discovery cycle failed");
                }
            }
        }
    }

    async fn discover_and_match(&self) -> Result<Vec<MatchedMarket>> {
        let mut all_discovered: Vec<DiscoveredMarket> = Vec::new();

        if self.platforms_config.polymarket.enabled {
            match self.fetch_polymarket_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Polymarket markets fetched");
                    all_discovered.extend(markets);
                }
                Err(e) => warn!(error = %e, "Failed to fetch Polymarket markets"),
            }
        }

        if self.platforms_config.kalshi.enabled {
            match self.fetch_kalshi_markets().await {
                Ok(markets) => {
                    info!(count = markets.len(), "Kalshi markets fetched");
                    all_discovered.extend(markets);
                }
                Err(e) => warn!(error = %e, "Failed to fetch Kalshi markets"),
            }
        }

        let mut matched = Vec::new();
        let mut seen_pairs = std::collections::HashSet::new();
        let poly_markets: Vec<_> = all_discovered.iter().filter(|m| m.platform == Platform::Polymarket).collect();
        let kalshi_markets: Vec<_> = all_discovered.iter().filter(|m| m.platform == Platform::Kalshi).collect();

        // H-3 FIX: Group Kalshi markets by expiration hour to reduce O(N^2) complexity to O(N * K)
        let mut kalshi_by_hour: HashMap<i64, Vec<&DiscoveredMarket>> = HashMap::new();
        for km in &kalshi_markets {
            let hour = km.expiration.timestamp() / 3600;
            kalshi_by_hour.entry(hour).or_default().push(km);
        }

        for pm in &poly_markets {
            let pm_hour = pm.expiration.timestamp() / 3600;
            // Only compare with Kalshi markets expiring in the same, previous, or next hour
            for hour_offset in -1..=1 {
                if let Some(kms) = kalshi_by_hour.get(&(pm_hour + hour_offset)) {
                    for km in kms {
                        // 1. Dual-Tier Expiration Check
                let exp_diff_secs = (pm.expiration - km.expiration).num_seconds().abs();
                let is_15m_market = pm.question_normalized.contains("15 min") || km.question_normalized.contains("15 min");

                if is_15m_market {
                    // CRITICAL: 15-minute candles MUST expire at basically the exact same time (within 2 mins)
                    // Otherwise a 14:00 candle might match with a 14:15 candle and result in naked exposure.
                    if exp_diff_secs > 120 { continue; }
                } else {
                    // Standard generic markets (e.g. politics, yearly price targets)
                    if exp_diff_secs > 48 * 3600 { continue; }
                }

                // 2. Token Overlap Jaccard Similarity
                let tokens_a: std::collections::HashSet<&str> = pm.question_normalized.split_whitespace().collect();
                let tokens_b: std::collections::HashSet<&str> = km.question_normalized.split_whitespace().collect();
                let intersection = tokens_a.intersection(&tokens_b).count();
                let union = tokens_a.union(&tokens_b).count();
                let sim = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };

                // 70% overlap for safer automated cross-platform matching
                if sim >= 0.7 {
                    // CRITICAL FIX: Extract numerical targets to prevent mismatched strikes (e.g. $60k vs $70k)
                    let nums_pm: Vec<f64> = pm.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();
                    let nums_km: Vec<f64> = km.question_normalized.split_whitespace()
                        .filter_map(|w| w.replace("$", "").replace(",", "").parse::<f64>().ok())
                        .collect();

                    if nums_pm != nums_km && (!nums_pm.is_empty() || !nums_km.is_empty()) {
                        tracing::warn!(
                            pm_question = %pm.question,
                            km_question = %km.question,
                            pm_nums = ?nums_pm,
                            km_nums = ?nums_km,
                            "DIFFERENT NUMERICAL TARGETS in fuzzy match — discarding match"
                        );
                        continue;
                    }

                    let unified_id = compute_unified_market_id(
                        &pm.question,
                        "cross_platform",
                        &pm.expiration.to_rfc3339(),
                    );

                    if !seen_pairs.insert(unified_id) {
                        continue;
                    }

                    let mut platform_infos = HashMap::new();
                    platform_infos.insert(pm.platform, PlatformMarketInfo {
                        platform: pm.platform,
                        platform_market_id: pm.platform_market_id.clone(),
                        fee_rate_bps: pm.fee_rate_bps,
                        min_order_size: pm.min_order_size,
                        tick_size: pm.tick_size,
                    });
                    platform_infos.insert(km.platform, PlatformMarketInfo {
                        platform: km.platform,
                        platform_market_id: km.platform_market_id.clone(),
                        fee_rate_bps: km.fee_rate_bps,
                        min_order_size: km.min_order_size,
                        tick_size: km.tick_size,
                    });

                    matched.push(MatchedMarket {
                        market: Market {
                            unified_id,
                            question: format!("{} / {}", pm.question, km.question), // Store both for auditability
                            resolution_source: "cross_platform".into(),
                            expiration: pm.expiration,
                            platforms: platform_infos,
                            // LOW-3 / MED-2 FIX: Better category extraction
                            category: {
                                let q = pm.question_normalized.as_str();
                                if q.contains("trump") || q.contains("election") || q.contains("biden") || q.contains("harris") { MarketCategory::Politics }
                                else if q.contains("bitcoin") || q.contains("btc") || q.contains("eth") || q.contains("crypto") { MarketCategory::Crypto }
                                else if q.contains("nba") || q.contains("nfl") || q.contains("super bowl") { MarketCategory::Sports }
                                else { MarketCategory::Other }
                            },
                            confidence: if sim > 0.7 { 0.98 } else { 0.95 },
                            // HIGH-1: All discovered markets start as Suspended to mandate human review, 
                            // preventing fuzzy matcher blindspots from executing mismatched strikes.
                            status: MarketStatus::Suspended,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        }
                    });
                }
                    }
                }
            }
        }

        Ok(matched)
    }


fn normalize_question(q: &str) -> String {
        // 1. Single initial allocation
        let mut lower = q.to_lowercase();
        
        // 2. Fast zero-allocation lookahead: Only allocate a new string if the word actually exists
        let replacements = [
            ("bitcoin", "btc"),
            ("ethereum", "eth"),
            ("solana", "sol"),
            ("ripple", "xrp"),
            ("dogecoin", "doge"),
            ("binance coin", "bnb"),
            ("minutes", "min"),
            ("minute", "min"),
        ];

        for (from, to) in replacements {
            if lower.contains(from) {
                lower = lower.replace(from, to);
            }
        }

        // 3. Single-pass iteration to strip non-alphanumeric chars and deduplicate spaces without Vecs
        let mut result = String::with_capacity(lower.len());
        let mut last_was_space = true;

        for c in lower.chars() {
            if c.is_alphanumeric() {
                result.push(c);
                last_was_space = false;
            } else if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        }

        // Clean up trailing space if the string ended with a special character
        if result.ends_with(' ') {
            result.pop();
        }

        result
    }

    async fn fetch_polymarket_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        // Polymarket CLOB API: GET /markets
        let url = format!("{}/markets", self.platforms_config.polymarket.rest_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("active", "true"), ("limit", "1000")])
            .send()
            .await
            .context("Polymarket markets fetch failed")?
            .json()
            .await
            .context("Polymarket markets parse failed")?;

        let mut markets = Vec::new();
        if let Some(arr) = resp.as_array() {
            for item in arr {
                let question = item
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if question.is_empty() {
                    continue;
                }
                // Polymarket CTF requires buying the specific NO token ID to short the market.
                // We extract both YES (index 0) and NO (index 1) token IDs and store them as a pair.
                let yes_token = item.get("tokens").and_then(|v| v.as_array()).and_then(|arr| arr.get(0)).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("");
                let no_token = item.get("tokens").and_then(|v| v.as_array()).and_then(|arr| arr.get(1)).and_then(|t| t.get("token_id")).and_then(|v| v.as_str()).unwrap_or("");
                
                if yes_token.is_empty() || yes_token.starts_with("0x") || no_token.is_empty() {
                    continue; 
                }
                let token_id = format!("{},{}", yes_token, no_token);
                let end_date = item
                    .get("end_date_iso")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

                markets.push(DiscoveredMarket {
                    platform: Platform::Polymarket,
                    platform_market_id: token_id,
                    question_normalized: Self::normalize_question(&question),
                    question,
                    resolution_source: "polymarket".into(),
                    expiration: end_date,
                    category: MarketCategory::Other,
                    fee_rate_bps: 200,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2), // 0.01
                });
            }
        }
        Ok(markets)
    }

    async fn fetch_kalshi_markets(&self) -> Result<Vec<DiscoveredMarket>> {
        let url = format!("{}/markets", self.platforms_config.kalshi.rest_url);
        let mut req = self.http.get(&url).query(&[("status", "open"), ("limit", "1000")]);
        
        if let Some(auth) = &self.kalshi_auth {
            if let Ok(token) = auth.generate_token() {
                req = req.header("Authorization", format!("Bearer {}", token));
            }
        }
        
        let resp: serde_json::Value = req.send()
            .await
            .context("Kalshi markets fetch failed")?
            .json()
            .await
            .context("Kalshi markets parse failed")?;

        let mut markets = Vec::new();
        if let Some(arr) = resp
            .get("markets")
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                let ticker = item
                    .get("ticker")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if ticker.is_empty() {
                    continue;
                }
                let close_time = item
                    .get("close_time")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

                markets.push(DiscoveredMarket {
                    platform: Platform::Kalshi,
                    platform_market_id: ticker,
                    question_normalized: Self::normalize_question(&title),
                    question: title,
                    resolution_source: "kalshi".into(),
                    expiration: close_time,
                    category: MarketCategory::Other,
                    fee_rate_bps: 175,
                    min_order_size: Decimal::ONE,
                    tick_size: Decimal::new(1, 2),
                });
            }
        }
        Ok(markets)
    }
}
```

## File: src/feeds/forecastex.rs
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tracing::{debug, info};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::ForecastExConfig;
use crate::types::*;

/// Minimal FIX 4.4 client for ForecastEx market data
pub struct ForecastExFeed {
    config: ForecastExConfig,
    subscriptions: Vec<(String, Uuid)>,
    books: std::collections::HashMap<String, FexOrderBook>,
    sequence: u64,
    msg_seq_num: u64,
    sender_comp_id: String,
    target_comp_id: String,
    expected_seq_num: u64,
}

struct FexOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
}

impl FexOrderBook {
    fn new() -> Self { Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new() } }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / rust_decimal::Decimal::from(2))
    }
    
    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        levels.extend(self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels.extend(self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels
    }
}

const SOH: char = '\x01';

impl ForecastExFeed {
    pub fn new(config: ForecastExConfig, subscriptions: Vec<(String, Uuid)>) -> Self {
        Self {
            config,
            subscriptions,
            books: std::collections::HashMap::new(),
            sequence: 0,
            msg_seq_num: 1,
            sender_comp_id: std::env::var("FEX_SENDER_COMP_ID").unwrap_or_else(|_| "MERCURY".into()),
            target_comp_id: std::env::var("FEX_TARGET_COMP_ID").unwrap_or_else(|_| "FORECASTEX".into()),
            expected_seq_num: 0,
        }
    }

    fn symbol_to_market_id(&self, symbol: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(s, _)| s == symbol)
            .map(|(_, id)| *id)
    }

    fn emit_tick(&self, symbol: &str) -> Option<NormalizedTick> {
        let market_id = self.symbol_to_market_id(symbol)?;
        let book = self.books.get(symbol)?;
        // best_bid/best_ask return None when either side is empty.
        // Returning None here suppresses the tick so the spread engine never
        // sees phantom prices (bid=0 or ask=1) that would trigger fake arbs.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        Some(NormalizedTick {
            platform: Platform::ForecastEx,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: std::sync::Arc::new(book.depth()),
            fee_rate_bps: 0,
            sequence: 0,
        })
    }

    fn build_fix_message(&mut self, msg_type: &str, body_fields: &[(u32, &str)]) -> String {
        let mut body = format!("35={}{}", msg_type, SOH);
        body.push_str(&format!("49={}{}", self.sender_comp_id, SOH));
        body.push_str(&format!("56={}{}", self.target_comp_id, SOH));
        body.push_str(&format!("34={}{}", self.msg_seq_num, SOH));
        body.push_str(&format!("52={}{}", chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f"), SOH));
        for (tag, val) in body_fields {
            body.push_str(&format!("{}={}{}", tag, val, SOH));
        }
        self.msg_seq_num += 1;

        let header = format!("8=FIX.4.4{}9={}{}", SOH, body.len(), SOH);
        let full = format!("{}{}", header, body);

        let checksum: u32 = full.bytes().map(|b| b as u32).sum::<u32>() % 256;
        format!("{}10={:03}{}", full, checksum, SOH)
    }

    fn parse_fix_fields(msg: &str) -> std::collections::HashMap<u32, String> {
        let mut fields = std::collections::HashMap::new();
        
        // MED-4: Validate Checksum (Tag 10)
        if let Some(csum_idx) = msg.find("10=") {
            let body_to_checksum = &msg[..csum_idx];
            let expected_checksum: u32 = body_to_checksum.bytes().map(|b| b as u32).sum::<u32>() % 256;
            
            let end_idx = msg[csum_idx..].find(SOH).unwrap_or(msg.len() - csum_idx);
            let provided_checksum_str = &msg[csum_idx + 3 .. csum_idx + end_idx];
            
            if let Ok(provided_checksum) = provided_checksum_str.parse::<u32>() {
                if expected_checksum != provided_checksum {
                    tracing::warn!("FIX Checksum mismatch: expected {}, got {}", expected_checksum, provided_checksum);
                    return fields; // Reject corrupt message
                }
            }
        }

        for part in msg.split(SOH) {
            if let Some(eq_pos) = part.find('=') {
                if let Ok(tag) = part[..eq_pos].parse::<u32>() {
                    fields.insert(tag, part[eq_pos + 1..].to_string());
                }
            }
        }
        fields
    }
}

#[async_trait]
impl FeedHandler for ForecastExFeed {
    fn platform(&self) -> Platform {
        Platform::ForecastEx
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        if !self.config.enabled {
            info!("ForecastEx feed disabled, skipping");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.fix_host, self.config.fix_port);
        info!(addr = %addr, "Connecting to ForecastEx FIX gateway");

        let stream = TcpStream::connect(&addr)
            .await
            .context("Failed to connect to ForecastEx FIX gateway")?;
            
        let mut tls_builder = native_tls::TlsConnector::builder();
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                tls_builder.add_root_certificate(cert);
            }
        }
        let tls_connector = tls_builder.build().context("Failed to build TLS")?;
        let tokio_tls = tokio_native_tls::TlsConnector::from(tls_connector);
        let tls_stream = tokio_tls.connect(&self.config.fix_host, stream).await.context("TLS handshake failed")?;

        let (reader, mut writer) = tokio::io::split(tls_stream);
        let mut buf_reader = BufReader::new(reader);

        // Send Logon (35=A)
        let logon = self.build_fix_message("A", &[
            (98, "0"),
            (108, "30"),
        ]);
        writer.write_all(logon.as_bytes()).await?;
        info!("FIX Logon sent");

        let sub_symbols: Vec<String> = self.subscriptions.iter().map(|(s, _)| s.clone()).collect();
        for symbol in &sub_symbols {
            self.books.entry(symbol.clone()).or_insert_with(FexOrderBook::new);
        }

        // Subscribe to market data (35=V)
        let symbols: Vec<(usize, String)> = self.subscriptions.iter()
            .enumerate()
            .map(|(i, (s, _))| (i, s.clone()))
            .collect();
        for (i, symbol) in &symbols {
            let md_req_id = format!("MDR{}", i);
            let md_request = self.build_fix_message("V", &[
                (262, &md_req_id),
                (263, "1"),
                (264, "10"),
                (267, "2"),
                (269, "0"),
                (269, "1"),
                (146, "1"),
                (55, symbol),
            ]);
            writer.write_all(md_request.as_bytes()).await?;
        }
        info!(count = self.subscriptions.len(), "FIX market data requests sent");

        let mut msg_buf = Vec::new();
        let mut line_buf = String::new();
        loop {
            msg_buf.clear();
            match tokio::time::timeout(
                std::time::Duration::from_secs(45),
                buf_reader.read_until(SOH as u8, &mut msg_buf),
            ).await {
                Ok(Ok(0)) => {
                    info!("FIX connection closed");
                    return Ok(());
                }
                Ok(Ok(_)) => {
                    let field = String::from_utf8_lossy(&msg_buf);
                    line_buf.push_str(&field);

                    if line_buf.contains("\x0110=") && line_buf.ends_with('\x01') {
                        let fields = Self::parse_fix_fields(&line_buf);
                        let msg_type = fields.get(&35).map(|s| s.as_str()).unwrap_or("");

                        // HIGH-2 FIX: Session-Level Sequence Validation
                        let seq_str = fields.get(&34).map(|s| s.as_str()).unwrap_or("0");
                        let seq_num = seq_str.parse::<u64>().unwrap_or(0);
                        if seq_num > 0 {
                            if seq_num < self.expected_seq_num {
                                tracing::warn!("FIX Sequence error: expected {}, got {}", self.expected_seq_num, seq_num);
                            } else if seq_num > self.expected_seq_num && self.expected_seq_num > 0 {
                                tracing::warn!("FIX Sequence gap: expected {}, got {}. Requesting Resend.", self.expected_seq_num, seq_num);
                                let expected_str = self.expected_seq_num.to_string();
                                let resend = self.build_fix_message("2", &[(7, &expected_str), (16, "0")]);
                                let _ = writer.write_all(resend.as_bytes()).await;
                            }
                            self.expected_seq_num = seq_num + 1;
                        }

                        match msg_type {
                            "W" | "X" => {
                                self.handle_market_data(&fields, &line_buf, &tick_tx);
                            }
                            "0" => {
                                let hb = self.build_fix_message("0", &[]);
                                let _ = writer.write_all(hb.as_bytes()).await;
                            }
                            "1" => {
                                let test_req_id = fields.get(&112).map(|s| s.as_str()).unwrap_or("0");
                                let hb = self.build_fix_message("0", &[(112, test_req_id)]);
                                let _ = writer.write_all(hb.as_bytes()).await;
                            }
                            "5" => {
                                info!("FIX Logout received");
                                return Ok(());
                            }
                            _ => {
                                debug!(msg_type, "FIX message received");
                            }
                        }
                        line_buf.clear();
                    }
                }
                Ok(Err(e)) => {
                    return Err(e).context("FIX read error");
                }
                Err(_) => {
                    let hb = self.build_fix_message("0", &[]);
                    writer.write_all(hb.as_bytes()).await?;
                }
            }
        }
    }
}

impl ForecastExFeed {
    fn handle_market_data(
        &mut self,
        fields: &std::collections::HashMap<u32, String>,
        raw_msg: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let symbol = fields.get(&55).map(|s| s.as_str()).unwrap_or("");
        if symbol.is_empty() { return; }

        let book = match self.books.get_mut(symbol) {
            Some(b) => b,
            None => return,
        };

        // FIX repeating groups: parse all 269/270/271 triplets from the raw message.
        // Tags appear in order: 269 (type), 270 (price), 271 (size), then next 269, etc.
        let parts: Vec<&str> = raw_msg.split(SOH).collect();
        let mut i = 0;
        let mut updated = false;
        while i < parts.len() {
            if let Some(eq) = parts[i].find('=') {
                let tag_str = &parts[i][..eq];
                if tag_str == "269" {
                    let entry_type = &parts[i][eq + 1..];
                    // Look ahead for 270 and 271
                    let price = Self::find_next_tag(&parts, i + 1, 270);
                    let size = Self::find_next_tag(&parts, i + 1, 271);
                    match (price, size) {
                        (Some(p_str), Some(s_str)) => {
                            let price = match Decimal::from_str(p_str) {
                                Ok(p) => p,
                                Err(_) => {
                                    tracing::error!(symbol, raw = p_str,
                                        "ForecastEx: bad price in repeating group — skipping entry");
                                    i += 1;
                                    continue;
                                }
                            };
                            let size = match Decimal::from_str(s_str) {
                                Ok(s) => s,
                                Err(_) => {
                                    tracing::error!(symbol, raw = s_str,
                                        "ForecastEx: bad size in repeating group — skipping entry");
                                    i += 1;
                                    continue;
                                }
                            };
                            match entry_type {
                                "0" => { // Bid
                                    if size == Decimal::ZERO { book.bids.remove(&price); }
                                    else { book.bids.insert(price, size); }
                                    updated = true;
                                }
                                "1" => { // Offer
                                    if size == Decimal::ZERO { book.asks.remove(&price); }
                                    else { book.asks.insert(price, size); }
                                    updated = true;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            i += 1;
        }

        if updated {
            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(symbol) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }
    }

    /// Scan forward from `start` for the next occurrence of `target_tag`.
    /// Stop if we hit another 269 (start of next entry) or run out of parts.
    fn find_next_tag<'a>(parts: &[&'a str], start: usize, target_tag: u32) -> Option<&'a str> {
        let target = target_tag.to_string();
        for i in start..parts.len() {
            if let Some(eq) = parts[i].find('=') {
                let tag = &parts[i][..eq];
                if tag == target {
                    return Some(&parts[i][eq + 1..]);
                }
                // Stop at next entry group
                if tag == "269" {
                    return None;
                }
            }
        }
        None
    }
}
```

## File: src/inventory/settlement.rs
```rust
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::db::Database;
use crate::types::*;
use serde_json::json;

pub struct SettlementMonitor {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    settlement_tx: mpsc::Sender<SettlementResult>,
    check_interval: Duration,
    kalshi_client: Option<crate::execution::kalshi_client::KalshiClient>,
}

impl SettlementMonitor {
    pub fn new(
        db: Arc<dyn Database>, 
        alert_tx: mpsc::Sender<AlertMessage>, 
        settlement_tx: mpsc::Sender<SettlementResult>,
        check_interval_secs: u64,
        kalshi_client: Option<crate::execution::kalshi_client::KalshiClient>,
    ) -> Self {
        Self { db, alert_tx, settlement_tx, check_interval: Duration::from_secs(check_interval_secs), kalshi_client }
    }

    pub async fn run(self) {
        info!("Settlement monitor started");
        let mut interval = tokio::time::interval(self.check_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.check_settlements().await {
                tracing::error!(error = %e, "Settlement check failed");
            }
        }
    }

    async fn check_settlements(&self) -> Result<()> {
        let positions = self.db.get_open_positions().await?;
        let now = chrono::Utc::now();

        for position in &positions {
            if let Some(market) = self.db.get_market(&position.market_id).await? {
                match market.status {
                    MarketStatus::Resolved => {
                        info!(position_id = position.id, market = %market.question, "Market resolved - position ready for settlement");
                        
                        let mut realized_pnl = -(position.avg_entry_price * position.quantity); // Assume total loss by default
                        
                        if let Some(info) = market.platforms.get(&position.platform) {
                            if position.platform == Platform::Kalshi {
                                if let Some(client) = &self.kalshi_client {
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    if let Ok(pnl) = client.fetch_settlement_payout(&info.platform_market_id, position.quantity, position.avg_entry_price).await {
                                        realized_pnl = pnl;
                                    }
                                }
                            } else if position.platform == Platform::Polymarket || position.platform == Platform::PolymarketUs || position.platform == Platform::Cdna || position.platform == Platform::ForecastEx {
                                tracing::warn!(position_id = position.id, platform = %position.platform, "Settlement for this platform requires manual verification");
                                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                    severity: "critical".into(),
                                    message: format!("Position #{} on {} resolved. Position closed in DB, but MANUAL PnL CREDIT REQUIRED to bankroll.", position.id, position.platform),
                                });
                                // Fix: Close position in DB to prevent infinite settlement loops, but skip automated realization.
                                if let Err(e) = self.db.close_position(position.id).await {
                                    tracing::error!(error = %e, "Failed to close position in DB");
                                }
                                continue; 
                            }
                        }
                        
                        let audit = AuditEntry {
                            timestamp_ns: now_ns(),
                            module: "settlement".into(),
                            event_type: "position_settled".into(),
                            data: json!({
                                "position_id": position.id,
                                "market": market.question,
                                "platform": position.platform.to_string(),
                                "quantity": position.quantity.to_string(),
                                "avg_entry_price": position.avg_entry_price.to_string(),
                                "realized_pnl": realized_pnl.to_string(),
                            }),
                        };
                        if let Err(e) = self.db.append_audit(&audit).await {
                            warn!(error = %e, position_id = position.id, "Failed to write settlement audit entry");
                        }
                        
                        let _ = self.settlement_tx.try_send(SettlementResult {
                            realized_pnl,
                            platform: position.platform,
                            market_id: position.market_id,
                            quantity: position.quantity,
                            avg_entry_price: position.avg_entry_price,
                        });
                        self.db.close_position(position.id).await?;
                        
                        let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                            severity: "info".into(),
                            message: format!(
                                "Position #{} settled: {} {} on {} ({} contracts @ ${})\nRealized PnL: ${}",
                                position.id, position.side, market.question,
                                position.platform, position.quantity, position.avg_entry_price, realized_pnl.round_dp(2),
                            ),
                        });
                    }
                    MarketStatus::Expired => {
                        warn!(position_id = position.id, market = %market.question, "Market expired with open position");
                        let audit = AuditEntry {
                            timestamp_ns: now_ns(),
                            module: "settlement".into(),
                            event_type: "position_expired".into(),
                            data: json!({
                                "position_id": position.id,
                                "market": market.question,
                                "platform": position.platform.to_string(),
                            }),
                        };
                        if let Err(e) = self.db.append_audit(&audit).await {
                            warn!(error = %e, position_id = position.id, "Failed to write expiry audit entry");
                        }
                        self.db.close_position(position.id).await?;
                    }
                    _ => {
                        let time_to_expiry = market.expiration - now;
                        if time_to_expiry.num_hours() < 1 && time_to_expiry.num_seconds() > 0 {
                            let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                severity: "warning".into(),
                                message: format!(
                                    "Position #{} expiring in {:.0} minutes: {}",
                                    position.id, time_to_expiry.num_minutes(), market.question,
                                ),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
```

## File: src/risk/circuit_breaker.rs
```rust
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct BreakerTrip {
    pub breaker_type: String,
    pub details: String,
    pub action: String,
    pub resume_at: Option<DateTime<Utc>>,
}

pub struct CircuitBreakers {
    max_single_trade_pct: Decimal,
    max_daily_loss_pct: Decimal,
    max_drawdown_pct: Decimal,
    max_platform_exposure_pct: Decimal,
    max_exec_failure_rate: Decimal,
    gas_price_max_gwei: u64,
    stale_feed_timeout_ms: u64,
    max_open_positions: usize,
    /// CB5: max fraction of bankroll on any single market across all platforms
    max_single_market_exposure_pct: Decimal,
    /// CB7: halt after N consecutive failed trades
    max_consecutive_failures: usize,

    trading_halted: bool,
    halt_resume_at: Option<DateTime<Utc>>,
    exec_failures: VecDeque<DateTime<Utc>>,
    exec_successes: VecDeque<DateTime<Utc>>,
    current_gas_gwei: u64,
    /// CB7 state: running count of consecutive failures (reset on success)
    consecutive_failures: usize,
    engine_start_time: DateTime<Utc>,
}

impl CircuitBreakers {
    pub fn new(
        max_single_trade_pct: Decimal,
        max_daily_loss_pct: Decimal,
        max_drawdown_pct: Decimal,
        max_platform_exposure_pct: Decimal,
        gas_price_max_gwei: u64,
        stale_feed_timeout_secs: u64,
        max_open_positions: usize,
    ) -> Self {
        Self {
            max_single_trade_pct,
            max_daily_loss_pct,
            max_drawdown_pct,
            max_platform_exposure_pct,
            max_exec_failure_rate: dec!(0.15),
            gas_price_max_gwei,
            stale_feed_timeout_ms: stale_feed_timeout_secs * 1000,
            max_open_positions,
            max_single_market_exposure_pct: dec!(0.20),
            max_consecutive_failures: 5,
            trading_halted: false,
            halt_resume_at: None,
            exec_failures: VecDeque::new(),
            exec_successes: VecDeque::new(),
            current_gas_gwei: 50,
            consecutive_failures: 0,
            engine_start_time: Utc::now(),
        }
    }

    pub fn update_limits(
        &mut self,
        max_single_trade_pct: Decimal,
        max_daily_loss_pct: Decimal,
        max_drawdown_pct: Decimal,
        max_platform_exposure_pct: Decimal,
        gas_price_max_gwei: u64,
        stale_feed_timeout_secs: u64,
        max_open_positions: usize,
    ) {
        self.max_single_trade_pct = max_single_trade_pct;
        self.max_daily_loss_pct = max_daily_loss_pct;
        self.max_drawdown_pct = max_drawdown_pct;
        self.max_platform_exposure_pct = max_platform_exposure_pct;
        self.gas_price_max_gwei = gas_price_max_gwei;
        self.stale_feed_timeout_ms = stale_feed_timeout_secs * 1000;
        self.max_open_positions = max_open_positions;
    }

    pub fn is_trading_halted(&mut self) -> bool {
        if !self.trading_halted {
            return false;
        }
        if let Some(resume) = self.halt_resume_at {
            if Utc::now() >= resume {
                self.trading_halted = false;
                self.halt_resume_at = None;
                return false;
            }
        }
        true
    }

    pub fn update_gas_price(&mut self, gwei: u64) {
        self.current_gas_gwei = gwei;
    }

    pub fn record_execution(&mut self, success: bool) {
        let now = Utc::now();
        if success {
            self.exec_successes.push_back(now);
            self.consecutive_failures = 0;
        } else {
            self.exec_failures.push_back(now);
            self.consecutive_failures += 1;
        }
        let cutoff = now - Duration::hours(1);
        // HIGH-2: Add a hard cap of 500 to prevent unbounded growth during failure storms
        while self.exec_failures.front().map(|t| *t < cutoff).unwrap_or(false) || self.exec_failures.len() > 500 {
            self.exec_failures.pop_front();
        }
        while self.exec_successes.front().map(|t| *t < cutoff).unwrap_or(false) || self.exec_successes.len() > 500 {
            self.exec_successes.pop_front();
        }
    }

    /// Run all circuit breaker checks.
    ///
    /// # Parameters
    /// - `trade_size`: notional value of the proposed trade
    /// - `bankroll`: total capital
    /// - `daily_loss_pct`: percent-scale (0–100)
    /// - `drawdown_pct`: percent-scale (0–100)
    /// - `platform_exposure_pct`: fraction-scale (0–1)
    /// - `open_positions`: current number of open arb pairs
    /// - `involves_polymarket`: whether trade touches Polymarket
    /// - `ms_since_last_tick`: milliseconds since last feed tick (for CB9)
    /// - `market_exposure_pct`: fraction of bankroll already on this specific market (for CB5)
    pub fn check_all(
        &mut self,
        trade_size: Decimal,
        bankroll: Decimal,
        daily_loss_pct: Decimal,
        drawdown_pct: Decimal,
        platform_exposure_pct: Decimal,
        open_positions: usize,
        involves_polymarket: bool,
        ms_since_last_tick: u64,
        market_exposure_pct: Decimal,
    ) -> Vec<BreakerTrip> {
        let mut trips = Vec::new();

        // CB1: Max Single Trade Size
        if bankroll > Decimal::ZERO {
            let trade_pct = trade_size / bankroll;
            if trade_pct > self.max_single_trade_pct {
                trips.push(BreakerTrip {
                    breaker_type: "CB1: Max Single Trade Size".into(),
                    details: format!("Trade {}% of bankroll exceeds {}% limit",
                        (trade_pct * Decimal::from(100)).round_dp(2),
                        (self.max_single_trade_pct * Decimal::from(100)).round_dp(2)),
                    action: "Hard reject, no override".into(),
                    resume_at: None,
                });
            }
        }

        // CB2: Max Daily Loss
        // CB2: Max Daily Loss
        // daily_loss_pct is on percent-scale (0–100), max_daily_loss_pct is fraction-scale (0–1).
        // Convert max to percent-scale for consistent comparison.
        let max_daily_loss_percent = self.max_daily_loss_pct * Decimal::from(100);
        if daily_loss_pct > max_daily_loss_percent {
            let resume = Utc::now() + Duration::hours(24);
            self.trading_halted = true;
            self.halt_resume_at = Some(resume);
            trips.push(BreakerTrip {
                breaker_type: "CB2: Max Daily Loss".into(),
                    details: format!("Daily loss {:.2}% exceeds {:.2}% limit",
                    daily_loss_pct, max_daily_loss_percent),
                action: "Trading halted for 24h".into(),
                resume_at: Some(resume),
            });
        }

        // CB3: Max Drawdown from Peak
        let max_drawdown_percent = self.max_drawdown_pct * Decimal::from(100);
        if drawdown_pct > max_drawdown_percent {
            trips.push(BreakerTrip {
                breaker_type: "CB3: Max Drawdown".into(),
                details: format!("Drawdown {:.2}% exceeds {:.2}% limit",
                    drawdown_pct, max_drawdown_percent),
                action: "Reduce Kelly fraction to 0.10, alert".into(),
                resume_at: None,
            });
        }

        // CB4: Max Platform Exposure
        if platform_exposure_pct > self.max_platform_exposure_pct {
            trips.push(BreakerTrip {
                breaker_type: "CB4: Max Platform Exposure".into(),
                details: format!("Platform exposure {:.2}% exceeds {:.2}% limit",
                    (platform_exposure_pct * Decimal::from(100)).round_dp(2),
                    (self.max_platform_exposure_pct * Decimal::from(100)).round_dp(2)),
                action: "Reject new orders on overweight platform".into(),
                resume_at: None,
            });
        }

        // CB5: Max Single-Market Correlated Exposure
        if market_exposure_pct > self.max_single_market_exposure_pct {
            trips.push(BreakerTrip {
                breaker_type: "CB5: Correlated Market Exposure".into(),
                details: format!("Market exposure {:.2}% exceeds {:.2}% limit",
                    (market_exposure_pct * Decimal::from(100)).round_dp(2),
                    (self.max_single_market_exposure_pct * Decimal::from(100)).round_dp(2)),
                action: "Skip — too much capital on one question".into(),
                resume_at: None,
            });
        }

        // CB6: Execution Failure Rate (>15% in 1h window)
        let total_execs = self.exec_failures.len() + self.exec_successes.len();
        if total_execs > 5 {
            let failure_rate = Decimal::from(self.exec_failures.len() as u64)
                / Decimal::from(total_execs as u64);
            if failure_rate > self.max_exec_failure_rate {
                trips.push(BreakerTrip {
                    breaker_type: "CB6: Execution Failure Rate".into(),
                    details: format!("Failure rate {:.1}% in last hour ({} failures / {} total)",
                        (failure_rate * Decimal::from(100)).round_dp(1),
                        self.exec_failures.len(), total_execs),
                    action: "Pause trading, diagnose connectivity".into(),
                    resume_at: None,
                });
            }
        }

        // CB7: Consecutive Failure Streak
        if self.consecutive_failures >= self.max_consecutive_failures {
            let resume = Utc::now() + Duration::minutes(10);
            self.trading_halted = true;
            self.halt_resume_at = Some(resume);
            trips.push(BreakerTrip {
                breaker_type: "CB7: Consecutive Failures".into(),
                details: format!("{} consecutive failed trades (limit {})",
                    self.consecutive_failures, self.max_consecutive_failures),
                action: "Pause 10 minutes — possible systemic issue".into(),
                resume_at: Some(resume),
            });
        }

        // CB8: Gas Price Spike (Polygon)
        if involves_polymarket && self.current_gas_gwei > self.gas_price_max_gwei {
            trips.push(BreakerTrip {
                breaker_type: "CB8: Gas Price Spike".into(),
                details: format!("Polygon gas {} gwei exceeds {} gwei limit",
                    self.current_gas_gwei, self.gas_price_max_gwei),
                action: "Pause Polymarket-leg trades".into(),
                resume_at: None,
            });
        }

        // CB9: Stale Feed Data (CRITICAL FIX 3-E)
        let uptime_ms = (Utc::now() - self.engine_start_time).num_milliseconds() as u64;
        if uptime_ms > self.stale_feed_timeout_ms {
            if ms_since_last_tick > self.stale_feed_timeout_ms && ms_since_last_tick != u64::MAX {
                trips.push(BreakerTrip {
                    breaker_type: "CB9: Stale Feed".into(),
                    details: format!("No tick received for {}ms (limit {}ms)",
                        ms_since_last_tick, self.stale_feed_timeout_ms),
                    action: "Halt trading — market data may be stale".into(),
                    resume_at: None,
                });
            }
        }

        // CB10: Max Open Positions
        if open_positions >= self.max_open_positions {
            trips.push(BreakerTrip {
                breaker_type: "CB10: Max Open Positions".into(),
                details: format!("{} positions >= {} limit", open_positions, self.max_open_positions),
                action: "Queue new opportunities until positions close".into(),
                resume_at: None,
            });
        }

        if !trips.is_empty() {
            warn!(count = trips.len(), "Circuit breakers tripped");
        }
        trips
    }

    pub fn reset_halt(&mut self) {
        self.trading_halted = false;
        self.halt_resume_at = None;
        self.consecutive_failures = 0;
        info!("Trading halt reset");
    }

    /// Explicitly pause or resume trading via manual Telegram command
    pub fn manual_halt(&mut self, halt: bool) {
        self.trading_halted = halt;
        if halt {
            // Effectively permanent halt until manually restarted
            self.halt_resume_at = Some(Utc::now() + Duration::days(365));
            info!("System manually HALTED via Telegram command.");
        } else {
            self.halt_resume_at = None;
            info!("System manually RESUMED via Telegram command.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_circuit_breaker_trade_size() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05), // 5% max single trade
            dec!(0.10),
            dec!(0.20),
            dec!(0.50),
            100,
            5,
            10,
        );

        // Trade size 600 out of 10000 bankroll is 6%, which exceeds 5% limit
        let trips = cb.check_all(
            dec!(600),
            dec!(10000),
            dec!(0),
            dec!(0),
            dec!(0),
            0,
            false,
            100,
            dec!(0),
        );
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].breaker_type, "CB1: Max Single Trade Size");
    }

    #[test]
    fn test_circuit_breaker_platform_exposure() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05),
            dec!(0.10),
            dec!(0.20),
            dec!(0.30), // 30% max platform exposure
            100,
            5,
            10,
        );

        // Platform exposure is 35% (0.35)
        let trips = cb.check_all(
                dec!(100),
                dec!(10000),
                dec!(0),
                dec!(0),
                dec!(0.35),
                0,
                false,
                100,
                dec!(0),
            );
            assert_eq!(trips.len(), 1);
            assert_eq!(trips[0].breaker_type, "CB4: Max Platform Exposure");
        }
    }
```

## File: src/types.rs
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

// ─── Side ───

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

impl fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketStatus::Active => write!(f, "active"),
            MarketStatus::Suspended => write!(f, "suspended"),
            MarketStatus::Resolved => write!(f, "resolved"),
            MarketStatus::Expired => write!(f, "expired"),
        }
    }
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
    pub book_depth: std::sync::Arc<Vec<PriceLevel>>,
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
    /// Platform-native market/token identifier (e.g. Polymarket token ID, Kalshi ticker).
    /// Used by execution clients; distinct from the unified UUID `market_id` on the opportunity.
    pub platform_market_id: String,
    pub fee_rate_bps: u32,
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

// ─── Validated Opportunity ───

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
    /// Size that was reserved in in_flight_notional when this trade was dispatched.
    /// Used by the event loop to release the reservation on settlement.
    /// Not persisted to the database.
    #[serde(default)]
    pub approved_size: Decimal,
}

// ─── Settlement Result ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    pub realized_pnl: Decimal,
    pub platform: Platform,
    pub market_id: Uuid,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
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

/// Returns nanoseconds since epoch. Cast to u64 wraps in 2554, which is acceptable.
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_nanos() as u64
}

// ─── System Commands (Telegram Control) ───

#[derive(Debug, Clone)]
pub enum SystemCommand {
    StartTrading,
    StopTrading,
    EnablePlatform(Platform),
    DisablePlatform(Platform),
    RequestDailyReport,
    ActivateMarket(Uuid),
}

#[derive(serde::Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    pub callback_query: Option<TelegramCallbackQuery>,
}

#[derive(serde::Deserialize)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub message: Option<TelegramMessage>,
    pub data: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct TelegramMessage {
    pub text: Option<String>,
    pub chat: TelegramChat,
    pub from: Option<TelegramUser>,
}

#[derive(serde::Deserialize)]
pub struct TelegramUser {
    pub id: i64,
}

#[derive(serde::Deserialize)]
pub struct TelegramChat {
    pub id: i64,
}

#[derive(serde::Deserialize)]
pub struct TelegramUpdatesResponse {
    pub ok: bool,
    pub result: Vec<TelegramUpdate>,
}
```

## File: src/config.rs
```rust
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
        anyhow::ensure!(t.stale_data_timeout_ms >= 1000,
            "stale_data_timeout_ms must be at least 1000ms, got {}", t.stale_data_timeout_ms);
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
            Err(poisoned) => poisoned.into_inner().clone(),
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
```

## File: src/db/traits.rs
```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::types::*;

#[async_trait]
pub trait Database: Send + Sync + 'static {
    // Markets
    async fn upsert_market(&self, market: &Market) -> Result<()>;
    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>>;
    async fn get_active_markets(&self) -> Result<Vec<Market>>;
    async fn get_suspended_markets(&self) -> Result<Vec<Market>>;
    async fn update_market_status(&self, id: &Uuid, status: MarketStatus) -> Result<()>;

    // Trades
    async fn insert_trade(&self, result: &TradeResult) -> Result<i64>;
    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>>;
    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>>;
    async fn get_trade_count(&self) -> Result<i64>;
    /// Count distinct in-flight arbitrage pairs (not individual position records).
    async fn get_open_arb_count(&self) -> Result<usize>;
    /// Sum cumulative profit of all successful trades to recover state post-crash
    async fn get_cumulative_profit(&self) -> Result<Decimal>;

    // Positions
    async fn upsert_position(&self, position: &Position) -> Result<()>;
    /// Upsert two positions (both legs of an arbitrage) atomically.
    async fn upsert_position_pair(&self, pos_a: &Position, pos_b: &Position) -> Result<()>;
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
    /// Batch-insert multiple audit entries in a single transaction.
    async fn append_audit_batch(&self, entries: &[AuditEntry]) -> Result<()>;

    // Config history
    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()>;

    // Utility
    async fn prune_audit_log(&self, keep_days: u32) -> Result<()>;
    async fn checkpoint_wal(&self) -> Result<()>;
    async fn db_size_bytes(&self) -> Result<u64>;
    /// Create an atomic backup of the database to the given file path.
    async fn backup_to_file(&self, dest_path: &str) -> Result<()>;
}
```

## File: src/engine/order_book.rs
```rust
use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;

use crate::types::*;

/// Per-platform order book for a single market
#[derive(Debug, Clone)]
pub struct PlatformBook {
    pub platform: Platform,
    pub market_id: Uuid,
    pub bids: std::collections::BTreeMap<Decimal, Decimal>,
    pub asks: std::collections::BTreeMap<Decimal, Decimal>,
    pub last_update_ns: u64,
    pub fee_rate_bps: u16,
    pub sequence: u64,
}

impl PlatformBook {
    pub fn new(platform: Platform, market_id: Uuid) -> Self {
        Self {
            platform,
            market_id,
            bids: std::collections::BTreeMap::new(),
            asks: std::collections::BTreeMap::new(),
            last_update_ns: 0,
            fee_rate_bps: 0,
            sequence: 0,
        }
    }

    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    pub fn mid_price(&self) -> Decimal {
        match (self.best_bid(), self.best_ask()) {
            (Some((b, _)), Some((a, _))) => (b + a) / Decimal::from(2),
            (Some((b, _)), None) => b,
            (None, Some((a, _))) => a,
            _ => Decimal::ZERO,
        }
    }

    pub fn ask_depth(&self) -> Vec<PriceLevel> {
        self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn bid_depth(&self) -> Vec<PriceLevel> {
        self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }).collect()
    }

    pub fn is_stale(&self, timeout_ns: u64) -> bool {
        let now = crate::types::now_ns();
        now.saturating_sub(self.last_update_ns) > timeout_ns
    }

    /// Update from a NormalizedTick
    ///
    /// Ticks with a non-zero sequence that is ≤ the stored sequence are dropped
    /// to prevent out-of-order or replayed updates (e.g. after a reconnect)
    /// from overwriting a newer book with stale data.
    /// Sequence-0 ticks are treated as full snapshots and always applied.
    pub fn update_from_tick(&mut self, tick: &NormalizedTick) {
        if tick.sequence > 0 && tick.sequence <= self.sequence {
            return;
        }

        // M-1 FIX: Avoid BTreeMap allocation churn by retaining existing nodes instead of clear()
        self.bids.retain(|k, _| {
            if *k == tick.bid_price { return true; }
            tick.book_depth.iter().any(|l| {
                if &l.price != k { return false; }
                if l.price <= tick.bid_price { return true; }
                if l.price < tick.ask_price && (tick.ask_price - l.price >= l.price - tick.bid_price) { return true; }
                false
            })
        });
        
        self.asks.retain(|k, _| {
            if *k == tick.ask_price { return true; }
            tick.book_depth.iter().any(|l| {
                if &l.price != k { return false; }
                if l.price >= tick.ask_price { return true; }
                if l.price > tick.bid_price && (tick.ask_price - l.price < l.price - tick.bid_price) { return true; }
                false
            })
        });

        for level in tick.book_depth.iter() {
            if level.price <= tick.bid_price {
                self.bids.insert(level.price, level.size);
            } else if level.price >= tick.ask_price {
                self.asks.insert(level.price, level.size);
            } else {
                // Mid-spread level: classify by which side it's closer to
                if tick.ask_price - level.price < level.price - tick.bid_price {
                    self.asks.insert(level.price, level.size);
                } else {
                    self.bids.insert(level.price, level.size);
                }
            }
        }

        if tick.bid_price > Decimal::ZERO && tick.bid_size > Decimal::ZERO {
            // CRITICAL: Prevent crossed books by wiping asks that are lower than the new bid
            self.asks.retain(|&p, _| p > tick.bid_price);
            self.bids.insert(tick.bid_price, tick.bid_size);
        }
        if tick.ask_price > Decimal::ZERO && tick.ask_size > Decimal::ZERO {
            // CRITICAL: Prevent crossed books by wiping bids that are higher than the new ask
            self.bids.retain(|&p, _| p < tick.ask_price);
            self.asks.insert(tick.ask_price, tick.ask_size);
        }

        self.last_update_ns = tick.timestamp_ns;
        self.fee_rate_bps = tick.fee_rate_bps;
        self.sequence = tick.sequence;
    }
}

/// Unified Order Book: aggregates all platform books for all markets
pub struct UnifiedOrderBook {
    books: HashMap<(Uuid, Platform), PlatformBook>,
}

impl UnifiedOrderBook {
    pub fn new() -> Self {
        Self { books: HashMap::new() }
    }

    pub fn clear(&mut self) {
        self.books.clear();
    }

    pub fn update(&mut self, tick: &NormalizedTick) {
        let key = (tick.market_id, tick.platform);
        let book = self.books
            .entry(key)
            .or_insert_with(|| PlatformBook::new(tick.platform, tick.market_id));
        book.update_from_tick(tick);
    }

    pub fn get_book(&self, market_id: &Uuid, platform: &Platform) -> Option<&PlatformBook> {
        self.books.get(&(*market_id, *platform))
    }
}
```

## File: src/feeds/cdna.rs
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message, Connector};
use tracing::{info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::CdnaConfig;
use crate::types::*;

pub struct CdnaFeed {
    config: CdnaConfig,
    subscriptions: std::collections::HashMap<String, Uuid>,
    fee_rates: std::collections::HashMap<String, u16>,
    books: std::collections::HashMap<String, CdnaOrderBook>,
    sequence: u64,
    request_id: u64,
}

struct CdnaOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
}

impl CdnaOrderBook {
    fn new() -> Self { Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new() } }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / rust_decimal::Decimal::from(2))
    }
    
    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        levels.extend(self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels.extend(self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels
    }
}

#[derive(Serialize)]
struct CdnaSubscribe {
    id: u64,
    method: String,
    params: CdnaSubParams,
}

#[derive(Serialize)]
struct CdnaSubParams {
    channels: Vec<String>,
}

impl CdnaFeed {
    pub fn new(config: CdnaConfig, subscriptions: Vec<(String, Uuid, u16)>) -> Self {
        let mut subs = std::collections::HashMap::new();
        let mut fees = std::collections::HashMap::new();
        for (inst, id, fee) in subscriptions {
            subs.insert(inst.clone(), id);
            fees.insert(inst, fee);
        }
        Self {
            config,
            subscriptions: subs,
            fee_rates: fees,
            books: std::collections::HashMap::new(),
            sequence: 0,
            request_id: 1,
        }
    }

    fn instrument_to_market_id(&self, instrument: &str) -> Option<Uuid> {
        self.subscriptions.get(instrument).copied()
    }

    fn emit_tick(&self, instrument: &str) -> Option<NormalizedTick> {
        let market_id = self.instrument_to_market_id(instrument)?;
        let book = self.books.get(instrument)?;
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        Some(NormalizedTick {
            platform: Platform::Cdna,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: std::sync::Arc::new(book.depth()),
            fee_rate_bps: self.fee_rates.get(instrument).copied().unwrap_or(150),
            sequence: 0,
        })
    }
}

#[async_trait]
impl FeedHandler for CdnaFeed {
    fn platform(&self) -> Platform {
        Platform::Cdna
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to CDNA WebSocket");

        let mut tls_builder = native_tls::TlsConnector::builder();
        tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                tls_builder.add_root_certificate(cert);
            }
        }
        let tls_connector = tls_builder.build().context("Failed to build CDNA TLS connector")?;
        let (ws_stream, _) = connect_async_tls_with_config(
            url, None, false, Some(Connector::NativeTls(tls_connector)),
        )
            .await
            .context("Failed to connect to CDNA WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        let channels: Vec<String> = self.subscriptions.keys()
            .map(|instrument| format!("book.{}", instrument))
            .collect();

        if !channels.is_empty() {
            let sub = CdnaSubscribe {
                id: self.request_id,
                method: "subscribe".into(),
                params: CdnaSubParams { channels: channels.clone() },
            };
            self.request_id += 1;
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = channels.len(), "Subscribed to CDNA channels");
        }

        for instrument in self.subscriptions.keys() {
            self.books.entry(instrument.clone()).or_insert_with(CdnaOrderBook::new);
        }

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_message(&text, &tick_tx) {
                        warn!(error = %e, "Failed to process CDNA message");
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    info!("CDNA WebSocket closed");
                    return Ok(());
                }
                Err(e) => return Err(e).context("CDNA WebSocket error"),
                _ => {}
            }
        }

        Ok(())
    }
}

impl CdnaFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let v: serde_json::Value = serde_json::from_str(text)?;

        // Accept both the initial subscription confirmation and subsequent push
        // updates. Push updates have a different (or absent) "method" value;
        // the reliable discriminator is the channel name in result.channel.
        let channel = v.get("result")
            .and_then(|r| r.get("channel"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if !channel.starts_with("book.") {
            return Ok(());
        }

        let instrument = &channel[5..];

        if let Some(data) = v.get("result").and_then(|r| r.get("data")) {
            if let Some(book) = self.books.get_mut(instrument) {
                if let Some(bids) = data.get("bids").and_then(|b| b.as_array()) {
                    for entry in bids {
                        if let (Some(p), Some(s)) = (
                            entry.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                            entry.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        ) {
                            if s == Decimal::ZERO { book.bids.remove(&p); } else { book.bids.insert(p, s); }
                        }
                    }
                }

                if let Some(asks) = data.get("asks").and_then(|b| b.as_array()) {
                    for entry in asks {
                        if let (Some(p), Some(s)) = (
                            entry.get(0).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                            entry.get(1).and_then(|v| v.as_str()).and_then(|s| Decimal::from_str(s).ok()),
                        ) {
                            if s == Decimal::ZERO { book.asks.remove(&p); } else { book.asks.insert(p, s); }
                        }
                    }
                }

                self.sequence += 1;
                if let Some(mut tick) = self.emit_tick(instrument) {
                    tick.sequence = self.sequence;
                    let _ = tick_tx.send(tick);
                }
            }
        }

        Ok(())
    }
}
```

## File: src/telegram/bot.rs
```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::warn;

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";
const MAX_MESSAGE_LENGTH: usize = 4096;
const RATE_LIMIT_PER_SECOND: u32 = 1;

#[derive(Clone)]
pub struct TelegramBot {
    client: reqwest::Client,
    token: zeroize::Zeroizing<String>, // HIGH-3: Secure token from memory dumps
    last_send: Arc<Mutex<Instant>>,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_markup: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct AnswerCallbackQueryRequest<'a> {
    callback_query_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
}

impl TelegramBot {

    /// SECURITY NOTE (CRIT-6): Telegram's API inherently requires the bot token to be
    /// passed in the URL path. Ensure that proxy servers, load balancers, or standard output
    /// do not log plain HTTP request URLs for api.telegram.org to prevent token leakage.
    pub fn new(token: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            token: zeroize::Zeroizing::new(token),
            last_send: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2))),
        }
    }

    /// Send a message to a specific chat, respecting rate limits
    pub async fn send_message(&self, chat_id: &str, text: &str) -> Result<()> {
        self.send_message_with_markup(chat_id, text, None).await
    }

    /// Send a message with an optional inline keyboard markup
    pub async fn send_message_with_markup(&self, chat_id: &str, text: &str, reply_markup: Option<serde_json::Value>) -> Result<()> {
        // Rate limiting: compute sleep duration while holding the lock,
        // then release the lock before sleeping so other callers aren't blocked.
        {
            let sleep_for = {
                let mut last = self.last_send.lock().await;
                let elapsed = last.elapsed();
                let min_interval = Duration::from_millis(1000 / RATE_LIMIT_PER_SECOND as u64);
                if elapsed < min_interval {
                    let wait = min_interval - elapsed;
                    // Pre-emptively advance the timer for the NEXT concurrent caller
                    *last += min_interval;
                    Some(wait)
                } else {
                    *last = Instant::now();
                    None
                }
            };
            if let Some(dur) = sleep_for {
                tokio::time::sleep(dur).await;
            }
        }

        // Truncate if too long, respecting UTF-8 boundaries
        let text = if text.len() > MAX_MESSAGE_LENGTH {
            let mut end = MAX_MESSAGE_LENGTH - 20;
            while end > 0 && !text.is_char_boundary(end) { end -= 1; }
            &text[..end]
        } else {
            text
        };

        let url = format!("{}{}/sendMessage", TELEGRAM_API_BASE, self.token.as_str());
        let body = SendMessageRequest {
            chat_id,
            text,
            parse_mode: "HTML",
            disable_web_page_preview: true,
            reply_markup,
        };

        // Retry with exponential backoff (max 3 attempts)
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let resp_body: TelegramResponse = resp.json().await
                        .unwrap_or(TelegramResponse { ok: false, description: Some("Failed to parse response".into()) });

                    if resp_body.ok {
                        return Ok(());
                    }

                    let desc = resp_body.description.unwrap_or_default();
                    if status.as_u16() == 429 {
                        warn!(attempt, "Telegram rate limited, backing off");
                        if attempt >= 3 {
                            anyhow::bail!("Telegram rate limited after {} attempts: {}", attempt, desc);
                        }
                        tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                        continue;
                    }

                    anyhow::bail!("Telegram API error ({}): {}", status, desc);
                }
                Err(e) => {
                    if attempt >= 3 {
                        return Err(e).context("Failed to send Telegram message after 3 attempts");
                    }
                    warn!(attempt, error = %e, "Telegram send failed, retrying");
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
    }

    /// Long-poll the Telegram API to fetch new commands
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<crate::types::TelegramUpdate>> {
        // LOW-7: More robust URL construction
        let url = format!("{}{}/getUpdates?offset={}&timeout=5", 
            TELEGRAM_API_BASE, 
            self.token.as_str(), 
            offset
        );
        let resp = self.client.get(&url).send().await?
            .json::<crate::types::TelegramUpdatesResponse>().await?;
        
        if resp.ok {
            Ok(resp.result)
        } else {
            Ok(vec![])
        }
    }

    /// Escape HTML special characters for Telegram HTML parse mode
    pub fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    
/// Acknowledge a callback query to remove the loading state from Telegram buttons
    pub async fn answer_callback_query(&self, query_id: &str, text: Option<&str>) -> Result<()> {
        let url = format!("{}{}/answerCallbackQuery", TELEGRAM_API_BASE, self.token.as_str());
        let body = AnswerCallbackQueryRequest { callback_query_id: query_id, text };
        self.client.post(&url).json(&body).send().await?;
        Ok(())
    }
}
```

## File: src/engine/spread.rs
```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::engine::order_book::PlatformBook;
use crate::feeds::normalizer;
use crate::types::*;

/// Result of spread computation for one direction of an arb pair
#[derive(Debug, Clone)]
pub struct SpreadResult {
    pub market_id: Uuid,
    pub leg_a_platform: Platform,
    pub leg_a_side: Side,
    pub leg_a_price: Decimal,
    pub leg_a_available: Decimal,
    pub leg_a_fee: Decimal,
    pub leg_b_platform: Platform,
    pub leg_b_side: Side,
    pub leg_b_price: Decimal,
    pub leg_b_available: Decimal,
    pub leg_b_fee: Decimal,
    pub raw_spread: Decimal,
    pub net_spread: Decimal,
    pub slippage_a: Decimal,
    pub slippage_b: Decimal,
    pub gas_cost: Decimal,
}

pub struct NetSpreadEngine {
    min_threshold: Decimal,
    gas_price_gwei: Decimal,
    /// MATIC/USD spot price (Polymarket runs on Polygon; gas is paid in MATIC, not ETH).
    /// Default is $0.50 MATIC — do NOT set this to an ETH price (~$2,000+) or gas costs
    /// will be overestimated 4,000x, suppressing all Polymarket arb opportunities.
    matic_price_usd: Decimal,
}

impl NetSpreadEngine {
    pub fn new(min_threshold: Decimal) -> Self {
        Self {
            min_threshold,
            gas_price_gwei: Decimal::from(50),
            matic_price_usd: dec!(0.50),
        }
    }

    pub fn update_threshold(&mut self, min_threshold: Decimal) {
        self.min_threshold = min_threshold;
    }

    pub fn update_gas_price(&mut self, gwei: Decimal) {
        self.gas_price_gwei = gwei;
    }

    pub fn update_matic_price(&mut self, price: Decimal) {
        self.matic_price_usd = price;
    }

    /// Compute spread for both directions of an arb pair
    pub fn compute_spreads(
        &self,
        book_a: &PlatformBook,
        book_b: &PlatformBook,
        target_size: Decimal,
    ) -> Vec<SpreadResult> {
        let mut results = Vec::new();

        // Direction 1: Buy YES on A, Buy NO on B
        if let (Some((ask_a, ask_a_size)), Some((bid_b, bid_b_size))) = (book_a.best_ask(), book_b.best_bid()) {
            let ask_b_no = Decimal::ONE - bid_b;
            let raw_spread = Decimal::ONE - ask_a - ask_b_no;

            if raw_spread > Decimal::ZERO {
                let depth_a = book_a.ask_depth();
                let depth_b = book_b.bid_depth();
                let total_a: Decimal = depth_a.iter().map(|l| l.size).sum();
                let total_b: Decimal = depth_b.iter().map(|l| l.size).sum();
                
                let actual_target = total_a.min(total_b).min(target_size);

                let fee_a = self.compute_fee(book_a.platform, ask_a, actual_target, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b_no, actual_target, book_b.fee_rate_bps);
                
                match (
                    normalizer::estimate_slippage(actual_target, &depth_a),
                    normalizer::estimate_slippage(actual_target, &depth_b),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                        let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        
                        // FIX: Normalize against actual_target to prevent artificial spread inflation
                        let per_contract_fee_a = fee_a / actual_target;
                        let per_contract_fee_b = fee_b / actual_target;
                        let per_contract_gas = gas / actual_target;

                        let net_spread = raw_spread - per_contract_fee_a - per_contract_fee_b - slippage_a - slippage_b - per_contract_gas;

                        if net_spread > self.min_threshold {
                            results.push(SpreadResult {
                                market_id: book_a.market_id,
                                leg_a_platform: book_a.platform,
                                leg_a_side: Side::Yes,
                                leg_a_price: ask_a,
                                leg_a_available: ask_a_size.min(target_size),
                                leg_a_fee: fee_a,
                                leg_b_platform: book_b.platform,
                                leg_b_side: Side::No,
                                leg_b_price: ask_b_no,
                                leg_b_available: bid_b_size.min(target_size),
                                leg_b_fee: fee_b,
                                raw_spread,
                                net_spread,
                                slippage_a,
                                slippage_b,
                                gas_cost: gas,
                            });
                        }
                    }
                    _ => {
                        tracing::warn!(
                            market_id = %book_a.market_id,
                            direction = "YES-A/NO-B",
                            "Insufficient liquidity to size opportunity — skipping"
                        );
                    }
                }
            }
        }

        // Direction 2: Buy NO on A, Buy YES on B
        if let (Some((bid_a, bid_a_size)), Some((ask_b, ask_b_size))) = (book_a.best_bid(), book_b.best_ask()) {
            let ask_a_no = Decimal::ONE - bid_a;
            let raw_spread = Decimal::ONE - ask_a_no - ask_b;

            if raw_spread > Decimal::ZERO {
                // CRITICAL FIX: Clamp to the total available depth, not just the top-of-book size.
                // This allows the VWAP estimator to correctly "walk the book" and consume 
                // deeper liquidity if the total spread remains profitable.
                let depth_a = book_a.bid_depth();
                let depth_b = book_b.ask_depth();
                
                let total_a: Decimal = depth_a.iter().map(|l| l.size).sum();
                let total_b: Decimal = depth_b.iter().map(|l| l.size).sum();

                let actual_target = total_a.min(total_b).min(target_size);

                let fee_a = self.compute_fee(book_a.platform, ask_a_no, actual_target, book_a.fee_rate_bps);
                let fee_b = self.compute_fee(book_b.platform, ask_b, actual_target, book_b.fee_rate_bps);
                
                // Optimized: Reuse the previously allocated depth vectors to save CPU cycles 
                // during the hot-path match evaluation.
                match (
                    normalizer::estimate_slippage(actual_target, &depth_a),
                    normalizer::estimate_slippage(actual_target, &depth_b),
                ) {
                    (Some(slippage_a), Some(slippage_b)) => {
                    let gas = self.gas_cost_if_onchain(book_a.platform, book_b.platform);
                        
                        // CRITICAL FIX: Mathematical Unit Mismatch.
                        // `fee_a` and `fee_b` were computed using `actual_target`, NOT `target_size`. 
                        // Dividing by the larger `target_size` artificially underestimated the fee drag,
                        // inflating `net_spread` and causing the engine to execute structurally unprofitable arbs.
                        let per_contract_fee_a = fee_a / actual_target;
                        let per_contract_fee_b = fee_b / actual_target;
                        let per_contract_gas = gas / actual_target;

                        let net_spread = raw_spread - per_contract_fee_a - per_contract_fee_b - slippage_a - slippage_b - per_contract_gas;

                        if net_spread > self.min_threshold {
                            results.push(SpreadResult {
                                market_id: book_a.market_id,
                                leg_a_platform: book_a.platform,
                                leg_a_side: Side::No,
                                leg_a_price: ask_a_no,
                                leg_a_available: bid_a_size.min(target_size),
                                leg_a_fee: fee_a,
                                leg_b_platform: book_b.platform,
                                leg_b_side: Side::Yes,
                                leg_b_price: ask_b,
                                leg_b_available: ask_b_size.min(target_size),
                                leg_b_fee: fee_b,
                                raw_spread,
                                net_spread,
                                slippage_a,
                                slippage_b,
                                gas_cost: gas,
                            });
                        }
                    }
                    _ => {
                        tracing::warn!(
                            market_id = %book_a.market_id,
                            direction = "NO-A/YES-B",
                            "Insufficient liquidity to size opportunity — skipping"
                        );
                    }
                }
            }
        }

        results
    }

    fn compute_fee(&self, platform: Platform, price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                normalizer::polymarket_fee(price, quantity, fee_rate_bps)
            }
            Platform::Kalshi => {
                // Use the dynamic fee_rate_bps provided by the Kalshi feed tick
                let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
                rate * quantity * price
            }
            Platform::Cdna => {
                let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
                rate * quantity * price.max(Decimal::ONE - price)
            }
            Platform::ForecastEx => Decimal::ZERO,
        }
    }

    fn gas_cost_if_onchain(&self, platform_a: Platform, platform_b: Platform) -> Decimal {
        let mut tx_count = Decimal::ZERO;
        if matches!(platform_a, Platform::Polymarket | Platform::PolymarketUs) {
            tx_count += Decimal::ONE;
        }
        if matches!(platform_b, Platform::Polymarket | Platform::PolymarketUs) {
            tx_count += Decimal::ONE;
        }

        if tx_count == Decimal::ZERO {
            return Decimal::ZERO;
        }

        // CRITICAL FIX (3-B): Update empirical gas limit. 
        // 200k was an overestimate; CTF exchange averages 120k-150k.
        let gas_units = Decimal::from(150_000) * tx_count;
        let gwei_to_matic = dec!(0.000000001); // 1 gwei = 10^-9 MATIC
        self.gas_price_gwei * gas_units * gwei_to_matic * self.matic_price_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_gas_cost_offchain() {
        let engine = NetSpreadEngine::new(dec!(0.01));
        // Kalshi <-> CDNA trade uses zero gas
        assert_eq!(engine.gas_cost_if_onchain(Platform::Kalshi, Platform::Cdna), Decimal::ZERO);
    }
}
```

## File: src/feeds/normalizer.rs
```rust
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::types::*;

/// Compute a deterministic unified market ID from question + resolution + expiry
pub fn compute_unified_market_id(question: &str, resolution_source: &str, expiry: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(question.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(resolution_source.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(expiry.as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Calculate Polymarket fee for given price and fee_rate_bps
pub fn polymarket_fee(price: Decimal, quantity: Decimal, fee_rate_bps: u16) -> Decimal {
    let rate = Decimal::from(fee_rate_bps) / Decimal::from(10000);
    let max_side = price.max(Decimal::ONE - price);
    rate * quantity * max_side
}

/// Calculate VWAP slippage for a target order size against order book depth.
///
/// `depth` **must** be pre-sorted in fill-walk order by the caller:
///   - For buys against asks: ascending price (`PlatformBook::ask_depth()`)
///   - For sells into bids: descending price (`PlatformBook::bid_depth()`)
///
/// Returns `None` when the available depth is insufficient to fill `target_size`.
/// Returns `Some(slippage)` — the absolute VWAP deviation from best quoted price.
pub fn estimate_slippage(target_size: Decimal, depth: &[PriceLevel]) -> Option<Decimal> {
    // CRITICAL FIX: If target size is zero or less, we must return None.
    // Returning Some(0) causes a fatal Divide-By-Zero panic in the spread engine 
    // when it attempts to normalize fees (fee_a / actual_target).
    if depth.is_empty() || target_size <= Decimal::ZERO {
        return None; 
    }

    let levels: Vec<&PriceLevel> = depth.iter().filter(|l| l.size > Decimal::ZERO).collect();
    if levels.is_empty() {
        // Return None to explicitly signal a complete lack of liquidity
        return None;
    }

    let best_price = levels[0].price;
    let mut remaining = target_size;
    let mut total_cost = Decimal::ZERO;

    for level in &levels {
        let fill_qty = remaining.min(level.size);
        total_cost += fill_qty * level.price;
        remaining -= fill_qty;
        if remaining <= Decimal::ZERO {
            break;
        }
    }

    if remaining > Decimal::ZERO {
        return None;
    }

    let vwap = total_cost / target_size;
    Some((vwap - best_price).abs())
}

pub fn kalshi_fee(price: Decimal, quantity: Decimal) -> Decimal {
    let max_fee = rust_decimal_macros::dec!(0.07);
    let implied_fee = price * rust_decimal_macros::dec!(0.10);
    max_fee.min(implied_fee) * quantity
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_polymarket_fee() {
        // Price > 0.5 (e.g. 0.6), quantity = 100, fee = 200 bps (0.02)
        // Fee = 0.02 * 100 * max(0.6, 0.4) = 2 * 0.6 = 1.2
        let fee = polymarket_fee(dec!(0.6), dec!(100), 200);
        assert_eq!(fee, dec!(1.20));

        // Price < 0.5 (e.g. 0.2), quantity = 50, fee = 100 bps (0.01)
        // Fee = 0.01 * 50 * max(0.2, 0.8) = 0.5 * 0.8 = 0.4
        let fee2 = polymarket_fee(dec!(0.2), dec!(50), 100);
        assert_eq!(fee2, dec!(0.40));
    }

    #[test]
    fn test_estimate_slippage() {
        let depth = vec![
            PriceLevel { price: dec!(0.50), size: dec!(10) },
            PriceLevel { price: dec!(0.52), size: dec!(20) },
        ];

        // Target size fully within first level
        let slip1 = estimate_slippage(dec!(5), &depth);
        assert_eq!(slip1, Some(dec!(0)));

        // Target size spans both levels:
        // 10 @ 0.50 = 5.0
        // 5 @ 0.52 = 2.6
        // Total cost = 7.6 for 15 contracts -> VWAP = 0.50666...
        // Best price = 0.50
        // Slippage = 0.006666...
        let slip2 = estimate_slippage(dec!(15), &depth).unwrap();
        assert!(slip2 > dec!(0.006));
        assert!(slip2 < dec!(0.007));

        // Insufficient depth
        let slip3 = estimate_slippage(dec!(50), &depth);
        assert_eq!(slip3, None);
    }
}
```

## File: src/feeds/polymarket.rs
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};
use tokio_tungstenite::Connector;
use tracing::{debug, info};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::PolymarketConfig;
use crate::types::*;

pub struct PolymarketFeed {
    config: PolymarketConfig,
    db: std::sync::Arc<dyn crate::db::Database>,
    subscriptions: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, LocalOrderBook>,
    fee_rates: std::collections::HashMap<String, u16>,
    sequence: u64,
}

struct LocalOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_trade_price: Decimal,
    sequence: u64,
}

impl LocalOrderBook {
    fn new() -> Self {
        Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new(), last_trade_price: Decimal::ZERO, sequence: 0 }
    }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (bid, _) = self.best_bid()?;
        let (ask, _) = self.best_ask()?;
        Some((bid + ask) / rust_decimal::Decimal::from(2))
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::with_capacity(20);
        levels.extend(self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels.extend(self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels
    }

    fn apply_update(&mut self, side: &str, price: Decimal, size: Decimal) {
        let book = if side == "BUY" || side == "bid" { &mut self.bids } else { &mut self.asks };
        if size == Decimal::ZERO {
            book.remove(&price);
        } else {
            book.insert(price, size);
        }
    }

    fn apply_snapshot(&mut self, bids: &[(Decimal, Decimal)], asks: &[(Decimal, Decimal)]) {
        self.bids.clear();
        self.asks.clear();
        for (p, s) in bids { self.bids.insert(*p, *s); }
        for (p, s) in asks { self.asks.insert(*p, *s); }
    }
}

#[derive(Deserialize)]
struct WsMessage {
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    market: String,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    bids: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    asks: Option<Vec<PriceSizeEntry>>,
    #[serde(default)]
    changes: Option<Vec<BookChange>>,
    #[serde(default)]
    sequence: Option<u64>,
}

#[derive(Deserialize)]
struct PriceSizeEntry {
    price: String,
    size: String,
}

#[derive(Deserialize)]
struct BookChange {
    side: String,
    price: String,
    size: String,
}

#[derive(Serialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    assets_ids: Vec<String>,
}

impl PolymarketFeed {
    // CRITICAL FIX: Added `db` parameter and converted Vec to HashMap to match struct definition
    pub fn new(
        config: PolymarketConfig,
        db: std::sync::Arc<dyn crate::db::Database>,
        subscriptions: Vec<(String, Uuid, u16)>,
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        let mut fee_rates = std::collections::HashMap::new();
        for (asset_id, market_id, fee_bps) in subscriptions {
            subs_map.insert(asset_id.clone(), market_id);
            fee_rates.insert(asset_id, fee_bps);
        }

        Self {
            config,
            db,
            subscriptions: subs_map,
            books: std::collections::HashMap::new(),
            fee_rates,
            sequence: 0,
        }
    }

    fn asset_to_market_id(&self, asset_id: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            .find(|(a, _)| {
                // CRITICAL FIX: Prevent prefix-matching bugs. 
                // If asset_id is "123", a.starts_with("123") would erroneously match "12345,678".
                // We strictly extract the YES token (first element) and do an exact match.
                let first_token = a.split(',').next().unwrap_or(a.as_str());
                first_token == asset_id
            })
            .map(|(_, id)| *id)
    }

    fn emit_tick(&self, asset_id: &str) -> Option<NormalizedTick> {
        let market_id = self.asset_to_market_id(asset_id)?;
        let book = self.books.get(asset_id)?;
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;
        let fee_bps = self.fee_rates.get(asset_id).copied().unwrap_or(200);

        Some(NormalizedTick {
            platform: Platform::Polymarket,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: book.last_trade_price,
            last_trade_size: Decimal::ZERO,
            book_depth: std::sync::Arc::new(book.depth()),
            fee_rate_bps: fee_bps,
            sequence: book.sequence, // CRITICAL FIX: Pass the actual sequence counter
        })
    }
}

#[async_trait]
impl FeedHandler for PolymarketFeed {
    fn platform(&self) -> Platform {
        Platform::Polymarket
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
        }
    }

async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        let url = &self.config.ws_url;
        info!(url, "Connecting to Polymarket WebSocket");

        let mut tls_builder = native_tls::TlsConnector::builder();
        tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                tls_builder.add_root_certificate(cert);
            }
        }
        let tls_connector = tls_builder.build().context("Failed to build TLS connector")?;
        let connector = Connector::NativeTls(tls_connector);
        let (ws_stream, _) = connect_async_tls_with_config(
            url,
            None, // WebSocket config
            false, // disable_nagle
            Some(connector),
        )
            .await
            .context("Failed to connect to Polymarket WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // We only subscribe to the YES token (the first token in the comma-separated pair) 
        // because the spread engine automatically derives the NO price from the YES orderbook.
        let asset_ids: Vec<String> = self.subscriptions.iter()
            .map(|(a, _)| a.split(',').next().unwrap_or(a).to_string())
            .collect();
            
        if !asset_ids.is_empty() {
            let sub_msg = SubscribeMessage {
                msg_type: "subscribe".into(),
                assets_ids: asset_ids.clone(),
            };
            let msg_text = serde_json::to_string(&sub_msg)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = asset_ids.len(), "Subscribed to Polymarket markets");
        }

        for (asset_id, _) in &self.subscriptions {
            self.books.entry(asset_id.clone()).or_insert_with(LocalOrderBook::new);
        }

        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            tokio::select! {
                msg_opt = read.next() => {
                    let msg = match msg_opt {
                        Some(m) => m,
                        None => continue,
                    };
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                tracing::warn!(error = %e, "Failed to process Polymarket message");
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Ping(data)) => {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await;
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            info!("Polymarket WebSocket closed by server");
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                        _ => {}
                    }
                }
                _ = sync_interval.tick() => {
                    // CRITICAL FIX: Dynamically ingest newly discovered 15-min candles without rebooting
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_subs = Vec::new();
                        for m in markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Polymarket) {
                                let asset_id = info.platform_market_id.clone();
                                if !self.subscriptions.contains_key(&asset_id) {
                                    self.subscriptions.insert(asset_id.clone(), m.unified_id);
                                    self.fee_rates.insert(asset_id.clone(), info.fee_rate_bps);
                                    new_subs.push(asset_id.split(',').next().unwrap_or(&asset_id).to_string());
                                }
                            }
                        }
                        if !new_subs.is_empty() {
                            let sub_msg = SubscribeMessage { msg_type: "subscribe".into(), assets_ids: new_subs.clone() };
                            if let Ok(msg_text) = serde_json::to_string(&sub_msg) {
                                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(msg_text.into())).await;
                                tracing::info!(count = new_subs.len(), "Dynamically subscribed to new Polymarket markets");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PolymarketPayload {
    Array(Vec<WsMessage>),
    Single(WsMessage),
}

impl PolymarketFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        match serde_json::from_str::<PolymarketPayload>(text) {
            Ok(PolymarketPayload::Array(messages)) => {
                for msg in messages {
                    self.process_event(&msg, tick_tx)?;
                }
            }
            Ok(PolymarketPayload::Single(msg)) => {
                self.process_event(&msg, tick_tx)?;
            }
            Err(e) => {
                tracing::debug!(error = %e, "Failed to parse Polymarket WS message");
            }
        }
        Ok(())
    }

    fn process_event(&mut self, msg: &WsMessage, tick_tx: &broadcast::Sender<NormalizedTick>) -> Result<()> {
        let asset_id = if !msg.asset_id.is_empty() {
            &msg.asset_id
        } else if !msg.market.is_empty() {
            &msg.market
        } else {
            return Ok(());
        };

        match msg.event_type.as_str() {
            "book" => {
                if let (Some(bids), Some(asks)) = (&msg.bids, &msg.asks) {
                    let mut seq_to_emit = None;
                    
                    if let Some(book) = self.books.get_mut(asset_id) {
                        book.bids.clear();
                        book.asks.clear();
                        
                        for e in bids {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                book.bids.insert(p, s);
                            }
                        }
                        for e in asks {
                            if let (Ok(p), Ok(s)) = (Decimal::from_str(&e.price), Decimal::from_str(&e.size)) {
                                book.asks.insert(p, s);
                            }
                        }
                        
                        // LOW-7 FIX: Properly handle book_snapshot sequence tracking
                        // Snapshots from Polymarket establish the base sequence. We accept it unconditionally 
                        // because a snapshot means we reconnected and need to hard-reset our local tracker.
                        if let Some(msg_seq) = msg.sequence {
                            book.sequence = msg_seq;
                        } else {
                            book.sequence += 1;
                        }
                        seq_to_emit = Some(book.sequence);
                    }
                    
                    if let Some(seq) = seq_to_emit {
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = seq;
                            let _ = tick_tx.send(tick);
                        }
                    }
                }
            }
            "price_change" | "book_update" => {
                if let Some(changes) = &msg.changes {
                    let mut seq_to_emit = None;
                    
                    if let Some(book) = self.books.get_mut(asset_id) {
                        if let Some(msg_seq) = msg.sequence {
                            if msg_seq <= book.sequence && book.sequence > 0 { return Ok(()); }
                            if book.sequence > 0 && msg_seq > book.sequence + 1 {
                                return Err(anyhow::anyhow!("Polymarket sequence gap detected: expected {}, got {}", book.sequence + 1, msg_seq));
                            }
                            book.sequence = msg_seq;
                        } else {
                            book.sequence += 1;
                        }
                        
                        for change in changes {
                            if let (Ok(p), Ok(s)) = (
                                Decimal::from_str(&change.price),
                                Decimal::from_str(&change.size),
                            ) {
                                book.apply_update(&change.side, p, s);
                            }
                        }
                        seq_to_emit = Some(book.sequence);
                    }
                    
                    if let Some(seq) = seq_to_emit {
                        if let Some(mut tick) = self.emit_tick(asset_id) {
                            tick.sequence = seq;
                            let _ = tick_tx.send(tick);
                        }
                    }
                }
            }
            "last_trade_price" => {
                if let Some(price_str) = &msg.price {
                    if let Ok(price) = Decimal::from_str(price_str) {
                        if let Some(book) = self.books.get_mut(asset_id) {
                            book.last_trade_price = price;
                        }
                    }
                }
            }
            _ => {
                debug!(event_type = %msg.event_type, "Unknown Polymarket event type");
            }
        }
        Ok(())
    }
}
```

## File: src/engine/detector.rs
```rust
use rust_decimal::Decimal;
use tracing::{debug};
use uuid::Uuid;
use rust_decimal::prelude::ToPrimitive;

use crate::engine::market_registry::MarketRegistry;
use crate::engine::order_book::UnifiedOrderBook;
use crate::engine::spread::{NetSpreadEngine, SpreadResult};
use crate::types::*;

/// Rejection reason for the 5-gate pipeline
#[derive(Debug, Clone)]
pub enum RejectionReason {
    BelowSpreadThreshold(Decimal),
    InsufficientLiquidity { available: Decimal, required: Decimal },
    StaleData { age_ms: u64, max_ms: u64 },
    CorrelationExposure { current: Decimal, max: Decimal },
    RiskBudgetExceeded { reason: String },
}

/// Detection statistics
#[derive(Debug, Default)]
pub struct DetectorStats {
    pub opportunities_detected: u64,
    pub gate1_rejected: u64,
    pub gate2_rejected: u64,
    pub gate3_rejected: u64,
    pub gate4_rejected: u64,
    pub gate5_rejected: u64,
    pub opportunities_passed: u64,
}

pub struct ArbitrageDetector {
    min_spread: Decimal,
    min_order_size: Decimal,
    stale_timeout_ms: u64,
    max_concurrent: usize,
    active_arbs: usize,
    pub stats: DetectorStats,
    paused_until_ns: u64,
}

impl ArbitrageDetector {
    pub fn new(
        min_spread: Decimal,
        min_order_size: Decimal,
        stale_timeout_ms: u64,
        max_concurrent: usize,
    ) -> Self {
        Self {
            min_spread,
            min_order_size,
            stale_timeout_ms,
            max_concurrent,
            active_arbs: 0,
            stats: DetectorStats::default(),
            paused_until_ns: 0,
        }
    }

    pub fn pause_detection_until(&mut self, ns: u64) {
        self.paused_until_ns = ns;
    }

    pub fn update_thresholds(&mut self, min_spread: Decimal, stale_timeout_ms: u64, max_concurrent: usize) {
        self.min_spread = min_spread;
        self.stale_timeout_ms = stale_timeout_ms;
        self.max_concurrent = max_concurrent;
    }

    pub fn set_active_arbs(&mut self, count: usize) {
        self.active_arbs = count;
    }

    /// Evaluates spreads ONLY for the specific market that just updated.
    pub fn detect_for_market(
        &mut self,
        market_id: &Uuid,
        registry: &MarketRegistry,
        uob: &UnifiedOrderBook,
        spread_engine: &NetSpreadEngine,
        target_size: Decimal,
    ) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        // HIGH-1 FIX: Do not evaluate spreads if the engine is in a cooldown period
        // (e.g., recovering from a broadcast::Lagged event repopulating the order books).
        if crate::types::now_ns() < self.paused_until_ns {
            return opportunities;
        }

        let pairs = match registry.get_arb_pairs_for_market(market_id) {
            Some(p) => p,
            None => return opportunities,
        };

        for pair in pairs {
            // CRITICAL FIX: The Time-to-Maturity Trap
            // Do not evaluate spreads if the market resolves in less than 60 seconds.
            // If a hedge fails at T-25s, the 30-second Unwind Watchdog will not wake up 
            // in time to dump the naked leg before the exchange locks the order book.
            if let Some(market) = registry.get_market(&pair.market_id) {
                let seconds_to_exp = (market.expiration - chrono::Utc::now()).num_seconds();
                if seconds_to_exp < 60 {
                    continue; 
                }
            }

            let book_a = match uob.get_book(&pair.market_id, &pair.platform_a) {
                Some(b) => b,
                None => continue,
            };
            let book_b = match uob.get_book(&pair.market_id, &pair.platform_b) {
                Some(b) => b,
                None => continue,
            };

            let spreads = spread_engine.compute_spreads(book_a, book_b, target_size);

            for spread in spreads {
                self.stats.opportunities_detected += 1;

                match self.run_gates(&spread, book_a, book_b, pair.confidence) {
                    Ok(()) => {
                        self.stats.opportunities_passed += 1;

                        // Fetch the native exchange identifiers and fee rates from the registry
                        let info_a = registry.get_platform_info(&pair.market_id, &pair.platform_a);
                        let info_b = registry.get_platform_info(&pair.market_id, &pair.platform_b);
                        
                        let (plat_id_a, fee_bps_a) = if let Some(i) = info_a { (i.platform_market_id.clone(), i.fee_rate_bps) } else { continue; };
                        let (plat_id_b, fee_bps_b) = if let Some(i) = info_b { (i.platform_market_id.clone(), i.fee_rate_bps) } else { continue; };

                        let liquidity = spread.leg_a_available.min(spread.leg_b_available);
                        let log_liq = if liquidity > Decimal::ZERO {
                            // HIGH-3 FIX: Accepted f64 imprecision with explicit documentation.
                            // Rust Decimal lacks a native ln() function. The floating-point conversion here 
                            // only impacts the relative ranking queue of opportunities (the score), 
                            // not the actual financial math, risk limits, or threshold gates.
                            // Add 1.0 to the natural log so a liquidity of 1.0 yields a multiplier of 1.0 (ln(1) = 0 + 1 = 1)
                            let val = liquidity.to_f64().unwrap_or(1.0).ln() + 1.0;
                            // Natively cast f64 to Decimal to eliminate string allocation in the hot path
                            Decimal::try_from(val.max(0.1)).unwrap_or(Decimal::ONE)
                        } else {
                            Decimal::ONE
                        };
                        let confidence_dec = Decimal::try_from(pair.confidence).unwrap_or(Decimal::ONE);
                        let score = spread.net_spread * log_liq * confidence_dec;

                        opportunities.push(ArbitrageOpportunity {
                            opp_id: Uuid::new_v4(),
                            market_id: spread.market_id,
                            market_question: String::new(), // M-9 FIX: Defer allocation until after truncation
                            leg_a: LegDetail {
                                platform: spread.leg_a_platform,
                                side: spread.leg_a_side,
                                price: spread.leg_a_price,
                                available_size: spread.leg_a_available,
                                fee_estimate: spread.leg_a_fee,
                                fee_rate_bps: fee_bps_a as u32, 
                                platform_market_id: plat_id_a, 
                            },
                            leg_b: LegDetail {
                                platform: spread.leg_b_platform,
                                side: spread.leg_b_side,
                                price: spread.leg_b_price,
                                available_size: spread.leg_b_available,
                                fee_estimate: spread.leg_b_fee,
                                fee_rate_bps: fee_bps_b as u32,
                                platform_market_id: plat_id_b, 
                            },
                            raw_spread: spread.raw_spread,
                            net_spread: spread.net_spread,
                            kelly_fraction: Decimal::ZERO, 
                            recommended_size: spread.leg_a_available.min(spread.leg_b_available), 
                            score,
                            detected_at: now_ns(),
                            // FIX: Reduce Time-to-Live to 200ms. If the execution queue backs up,
                            // prices will move. Drops stale arbs before they execute at a loss.
                            ttl_ms: 200,
                        });
                    }
                    Err(reason) => {
                        debug!(?reason, market_id = %spread.market_id, "Opportunity rejected");
                    }
                }
            }
        }

        opportunities.sort_by(|a, b| b.score.cmp(&a.score));
        let slots = self.max_concurrent.saturating_sub(self.active_arbs);
        opportunities.truncate(slots);

        // M-9 FIX: Allocate strings only for the opportunities that actually made the cut
        for opp in &mut opportunities {
            if let Some(market) = registry.get_market(&opp.market_id) {
                opp.market_question = market.question.clone();
            }
        }

        opportunities
    }

    fn run_gates(
        &mut self,
        spread: &SpreadResult,
        book_a: &crate::engine::order_book::PlatformBook,
        book_b: &crate::engine::order_book::PlatformBook,
        confidence: f64,
    ) -> Result<(), RejectionReason> {
        if spread.net_spread < self.min_spread {
            self.stats.gate1_rejected += 1;
            return Err(RejectionReason::BelowSpreadThreshold(spread.net_spread));
        }

        let min_available = spread.leg_a_available.min(spread.leg_b_available);
        if min_available < self.min_order_size {
            self.stats.gate2_rejected += 1;
            return Err(RejectionReason::InsufficientLiquidity {
                available: min_available,
                required: self.min_order_size,
            });
        }

        let stale_timeout_ns = self.stale_timeout_ms * 1_000_000;
        let now = now_ns();

        let age_a = now.saturating_sub(book_a.last_update_ns);
        if age_a > stale_timeout_ns {
            self.stats.gate3_rejected += 1;
            return Err(RejectionReason::StaleData {
                age_ms: age_a / 1_000_000,
                max_ms: self.stale_timeout_ms,
            });
        }

        let age_b = now.saturating_sub(book_b.last_update_ns);
        if age_b > stale_timeout_ns {
            self.stats.gate3_rejected += 1;
            return Err(RejectionReason::StaleData {
                age_ms: age_b / 1_000_000,
                max_ms: self.stale_timeout_ms,
            });
        }

        if confidence < 0.95 {
            self.stats.gate4_rejected += 1;
            return Err(RejectionReason::CorrelationExposure {
                current: Decimal::ZERO,
                max: Decimal::ZERO,
            });
        }

        if self.active_arbs >= self.max_concurrent {
            self.stats.gate5_rejected += 1;
            return Err(RejectionReason::RiskBudgetExceeded {
                reason: format!("Max concurrent arbs reached: {}", self.max_concurrent),
            });
        }

        Ok(())
    }
}
```

## File: src/feeds/kalshi.rs
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::client::IntoClientRequest, tungstenite::Message, Connector};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::base::FeedHandler;
use crate::config::KalshiConfig;
use crate::crypto::jwt::KalshiAuth;
use crate::types::*;
use std::sync::Arc;

pub struct KalshiFeed {
    config: KalshiConfig,
    auth: Option<Arc<KalshiAuth>>,
    db: std::sync::Arc<dyn crate::db::Database>,
    subscriptions: std::collections::HashMap<String, Uuid>,
    books: std::collections::HashMap<String, KalshiOrderBook>,
    sequence: u64,
}

struct KalshiOrderBook {
    bids: std::collections::BTreeMap<Decimal, Decimal>,
    asks: std::collections::BTreeMap<Decimal, Decimal>,
    last_seq: u64,
    is_initialized: bool,
}

impl KalshiOrderBook {
    fn new() -> Self {
        Self { bids: std::collections::BTreeMap::new(), asks: std::collections::BTreeMap::new(), last_seq: 0, is_initialized: false }
    }

    fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(&p, &s)| (p, s))
    }

    fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(&p, &s)| (p, s))
    }

    fn mid_price(&self) -> Option<Decimal> {
        let (b, _) = self.best_bid()?;
        let (a, _) = self.best_ask()?;
        Some((b + a) / rust_decimal::Decimal::from(2))
    }

    fn depth(&self) -> Vec<PriceLevel> {
        let mut levels = Vec::new();
        levels.extend(self.bids.iter().rev().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels.extend(self.asks.iter().take(10).map(|(&p, &s)| PriceLevel { price: p, size: s }));
        levels
    }
}

#[derive(Deserialize)]
struct KalshiWsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    msg: Option<KalshiMsgPayload>,
}

#[derive(Deserialize)]
pub struct KalshiMsgPayload {
    #[serde(default)]
    pub market_ticker: String,
    pub seq: Option<u64>,
    pub yes: Option<Vec<[String; 2]>>,
    pub no: Option<Vec<[String; 2]>>,
    pub price_deltas: Option<KalshiDeltas>,
}

#[derive(Deserialize)]
pub struct KalshiDeltas {
    pub yes: Option<Vec<[String; 2]>>,
    pub no: Option<Vec<[String; 2]>>,
}

#[derive(Serialize)]
struct KalshiSubscribe {
    id: u64,
    cmd: String,
    params: KalshiSubParams,
}

#[derive(Serialize)]
struct KalshiSubParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
}

impl KalshiFeed {
    pub fn new(
        config: KalshiConfig,
        auth: Option<KalshiAuth>,
        db: std::sync::Arc<dyn crate::db::Database>,
        subscriptions: Vec<(String, Uuid)>
    ) -> Self {
        let mut subs_map = std::collections::HashMap::new();
        for (ticker, market_id) in subscriptions {
            subs_map.insert(ticker, market_id);
        }
        Self {
            config,
            auth: auth.map(Arc::new),
            db,
            subscriptions: subs_map,
            books: std::collections::HashMap::new(),
            sequence: 0,
        }
    }

    fn ticker_to_market_id(&self, ticker: &str) -> Option<Uuid> {
        self.subscriptions.iter()
            // CRITICAL FIX: Explicitly extract the string slice for comparison
            .find(|(t, _)| t.as_str() == ticker)
            .map(|(_, id)| *id)
    }
    fn emit_tick(&self, ticker: &str) -> Option<NormalizedTick> {
        let market_id = self.ticker_to_market_id(ticker)?;
        let book = self.books.get(ticker)?;
        // Both sides must be present — phantom fallbacks (bid=0, ask=1) would
        // make the spread engine see a fake ~100% arb and fire real orders.
        let bid = book.best_bid()?;
        let ask = book.best_ask()?;
        let mid = book.mid_price()?;

        // CRIT-5 FIX: Route feed calculations through the centralized normalizer function
        // to guarantee identical math between the execution engine and the feed estimator.
        let fee_per_contract = crate::feeds::normalizer::kalshi_fee(mid, Decimal::ONE);
        
        let fee_bps = if mid > Decimal::ZERO {
            let bps = (fee_per_contract / mid) * Decimal::from(10000);
            // CRITICAL FIX: Decimal to u16 conversion fails if there is any fractional remainder.
            // We must round the BPS first, otherwise it will constantly default to 175.
            bps.round().try_into().unwrap_or(175u16)
        } else {
            175
        };

        Some(NormalizedTick {
            platform: Platform::Kalshi,
            market_id,
            timestamp_ns: now_ns(),
            bid_price: bid.0,
            bid_size: bid.1,
            ask_price: ask.0,
            ask_size: ask.1,
            mid_price: mid,
            last_trade_price: Decimal::ZERO,
            last_trade_size: Decimal::ZERO,
            book_depth: std::sync::Arc::new(book.depth()),
            fee_rate_bps: fee_bps,
            sequence: 0,
        })
    }
}

#[async_trait]
impl FeedHandler for KalshiFeed {
    fn platform(&self) -> Platform {
        Platform::Kalshi
    }

    fn clear_books(&mut self) {
        for book in self.books.values_mut() {
            book.bids.clear();
            book.asks.clear();
            book.last_seq = 0;
            // Fix: Mark book as uninitialized to reject all deltas until snapshot arrives
            book.is_initialized = false;
        }
    }

    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()> {
        info!("Connecting to Kalshi WebSocket");

        // Send JWT in Authorization header — never in the URL where it can be
        // logged by proxies, CDNs, or the server's access log (CRIT-2 fix).
        let (ws_stream, _) = if let Some(auth) = &self.auth {
            let token = auth.generate_token()?;
            let mut request = self.config.ws_url.as_str()
                .into_client_request()
                .context("Invalid Kalshi WebSocket URL")?;
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
                format!("Bearer {}", token)
                    .try_into()
                    .context("Failed to build Kalshi auth header")?,
            );
            {
                let mut tls_builder = native_tls::TlsConnector::builder();
                tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                // M-8 FIX: Explicit TLS Cert Pinning
                if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
                    if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                        tls_builder.add_root_certificate(cert);
                    }
                }
                let tls = tls_builder.build().context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(request, None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        } else {
            {
                let mut tls_builder = native_tls::TlsConnector::builder();
                tls_builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                // M-8 FIX: Explicit TLS Cert Pinning
                if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
                    if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
                        tls_builder.add_root_certificate(cert);
                    }
                }
                let tls = tls_builder.build().context("Failed to build Kalshi TLS connector")?;
                connect_async_tls_with_config(self.config.ws_url.as_str(), None, false, Some(Connector::NativeTls(tls)))
                    .await.context("Failed to connect to Kalshi WebSocket")?
            }
        };

        let (mut write, mut read) = ws_stream.split();

        let tickers: Vec<String> = self.subscriptions.keys().cloned().collect();
        if !tickers.is_empty() {
            let sub = KalshiSubscribe {
                id: 1,
                cmd: "subscribe".into(),
                params: KalshiSubParams {
                    channels: vec!["orderbook_snapshot".into(), "orderbook_delta".into(), "trade".into()],
                    market_tickers: tickers.clone(),
                },
            };
            let msg_text = serde_json::to_string(&sub)?;
            write.send(Message::Text(msg_text.into())).await?;
            info!(count = tickers.len(), "Subscribed to Kalshi markets");
        }

        for (ticker, _) in &self.subscriptions {
            self.books.entry(ticker.clone()).or_insert_with(KalshiOrderBook::new);
        }

        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            tokio::select! {
                msg_opt = read.next() => {
                    let msg = match msg_opt {
                        Some(m) => m,
                        None => continue,
                    };
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text, &tick_tx) {
                                tracing::warn!(error = %e, "Failed to process Kalshi message");
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Ping(data)) => {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await;
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            tracing::info!("Kalshi WebSocket closed");
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                        _ => {}
                    }
                }
                _ = sync_interval.tick() => {
                    // CRITICAL FIX: Dynamically ingest newly discovered markets to prevent Kalshi blindspots
                    if let Ok(markets) = self.db.get_active_markets().await {
                        let mut new_subs = Vec::new();
                        for m in markets {
                            if let Some(info) = m.platforms.get(&crate::types::Platform::Kalshi) {
                                let ticker = info.platform_market_id.clone();
                                if !self.subscriptions.contains_key(&ticker) {
                                    self.subscriptions.insert(ticker.clone(), m.unified_id);
                                    new_subs.push(ticker);
                                }
                            }
                        }
                        if !new_subs.is_empty() {
                            let sub = KalshiSubscribe {
                                id: 2,
                                cmd: "subscribe".into(),
                                params: KalshiSubParams {
                                    channels: vec!["orderbook_snapshot".into(), "orderbook_delta".into()],
                                    market_tickers: new_subs.clone(),
                                },
                            };
                            if let Ok(msg_text) = serde_json::to_string(&sub) {
                                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(msg_text.into())).await;
                                tracing::info!(count = new_subs.len(), "Dynamically subscribed to new Kalshi markets");
                            }
                        }
                    }
                }
            }
        }
    }
}

impl KalshiFeed {
    fn handle_message(
        &mut self,
        text: &str,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> Result<()> {
        let msg: KalshiWsMessage = serde_json::from_str(text)?;

        match msg.msg_type.as_str() {
            "orderbook_snapshot" => {
                if let Some(data) = msg.msg {
                    self.handle_orderbook_snapshot(&data, tick_tx);
                }
            }
            "orderbook_delta" => {
                if let Some(data) = msg.msg {
                    if self.handle_orderbook_delta(&data, tick_tx) {
                        return Err(anyhow::anyhow!("Sequence gap — reconnecting for fresh snapshot"));
                    }
                }
            }
            "trade" => {}
            "subscribed" => {
                info!("Kalshi subscription confirmed");
            }
            _ => {
                debug!(msg_type = %msg.msg_type, "Unknown Kalshi message type");
            }
        }

        Ok(())
    }

    fn handle_orderbook_snapshot(
        &mut self,
        data: &KalshiMsgPayload,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) {
        let ticker = &data.market_ticker;
        if let Some(book) = self.books.get_mut(ticker) {
            book.is_initialized = true; // CRITICAL FIX: Mark book safe for incoming deltas
            book.bids.clear();
            book.asks.clear();

            // Kalshi API v2 sends prices as dollar-formatted strings ("0.4200"),
            // NOT cent integers. Parse directly — no /100 conversion.
            if let Some(yes_bids) = &data.yes {
                for level in yes_bids {
                    if let (Ok(p), Ok(s)) = (
                        Decimal::from_str(&level[0]),
                        Decimal::from_str(&level[1]),
                    ) {
                        book.bids.insert(p, s);
                    }
                }
            }

            if let Some(no_asks) = &data.no {
                for level in no_asks {
                    if let (Ok(p), Ok(s)) = (
                        Decimal::from_str(&level[0]),
                        Decimal::from_str(&level[1]),
                    ) {
                        // CRITICAL FIX: Kalshi sends bids for the NO token. 
                        // A bid for NO at 0.40 is equivalent to an ask for YES at 0.60.
                        let yes_ask_price = Decimal::ONE - p;
                        book.asks.insert(yes_ask_price, s);
                    }
                }
            }

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(ticker) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }
    }

    fn handle_orderbook_delta(
        &mut self,
        data: &KalshiMsgPayload,
        tick_tx: &broadcast::Sender<NormalizedTick>,
    ) -> bool {
        let ticker = &data.market_ticker;
        let seq = data.seq.unwrap_or(0);

        if let Some(book) = self.books.get(ticker) {
            // Fix: If we receive a delta before a snapshot, explicitly trigger a reconnect.
            if !book.is_initialized {
                return true; 
            }
            if seq > 0 && book.last_seq > 0 && seq != book.last_seq + 1 {
                warn!(ticker, expected = book.last_seq + 1, got = seq,
                    "Kalshi sequence gap — reconnecting to get fresh snapshot");
                return true; // signal caller to reconnect
            }
        }

        if let Some(book) = self.books.get_mut(ticker) {
            book.last_seq = seq;

            if let Some(deltas) = &data.price_deltas {
                if let Some(bid_deltas) = &deltas.yes {
                    for delta in bid_deltas {
                        if let (Ok(p), Ok(s)) = (
                            Decimal::from_str(&delta[0]),
                            Decimal::from_str(&delta[1]),
                        ) {
                            if s == Decimal::ZERO { book.bids.remove(&p); }
                            else { book.bids.insert(p, s); }
                        }
                    }
                }

            if let Some(ask_deltas) = &deltas.no {
                    for delta in ask_deltas {
                        if let (Ok(p), Ok(s)) = (
                            Decimal::from_str(&delta[0]),
                            Decimal::from_str(&delta[1]),
                        ) {
                            // CRITICAL FIX: Invert NO bids to YES asks
                            let yes_ask_price = Decimal::ONE - p;
                            if s == Decimal::ZERO { book.asks.remove(&yes_ask_price); }
                            else { book.asks.insert(yes_ask_price, s); }
                        }
                    }
                }
            }

            self.sequence += 1;
            if let Some(mut tick) = self.emit_tick(ticker) {
                tick.sequence = self.sequence;
                let _ = tick_tx.send(tick);
            }
        }

        false // no reconnect needed
    }
}
```

## File: src/risk/bankroll.rs
```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tracing::info;
use uuid::Uuid;

use crate::types::*;

pub struct BankrollManager {
    total_bankroll: Decimal,
    peak_bankroll: Decimal,
    platform_balances: HashMap<Platform, Decimal>,
    platform_exposure: HashMap<Platform, Decimal>,
    market_exposure: HashMap<Uuid, Decimal>,
    daily_pnl: Decimal,
    daily_start_bankroll: Decimal,
    daily_fees: Decimal,
    day_start: DateTime<Utc>,
    trades_today: i32,
    success_today: i32,
    fail_today: i32,
    exec_success_rate: Decimal,
    exec_history: std::collections::VecDeque<(chrono::DateTime<chrono::Utc>, bool)>,
}

impl BankrollManager {
    pub fn new(initial_bankroll: Decimal) -> Self {
        Self {
            total_bankroll: initial_bankroll,
            peak_bankroll: initial_bankroll,
            platform_balances: HashMap::new(),
            platform_exposure: HashMap::new(),
            market_exposure: HashMap::new(),
            daily_pnl: Decimal::ZERO,
            daily_start_bankroll: initial_bankroll,
            daily_fees: Decimal::ZERO,
            day_start: Utc::now(),
            trades_today: 0,
            success_today: 0,
            fail_today: 0,
            exec_success_rate: dec!(0.90),
            exec_history: std::collections::VecDeque::new(),
        }
    }

    pub fn total_bankroll(&self) -> Decimal { self.total_bankroll }
    pub fn peak_bankroll(&self) -> Decimal { self.peak_bankroll }
    pub fn daily_pnl(&self) -> Decimal { self.daily_pnl }
    pub fn daily_fees(&self) -> Decimal { self.daily_fees }
    pub fn trades_today(&self) -> i32 { self.trades_today }
    pub fn success_today(&self) -> i32 { self.success_today }
    pub fn fail_today(&self) -> i32 { self.fail_today }
    pub fn exec_success_rate(&self) -> Decimal { self.exec_success_rate }

    pub fn drawdown_pct(&self) -> Decimal {
        if self.peak_bankroll > Decimal::ZERO {
            (self.peak_bankroll - self.total_bankroll) / self.peak_bankroll * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    pub fn daily_loss_pct(&self) -> Decimal {
        // Only trigger the loss percentage calculation if PnL is actually negative
        if self.daily_start_bankroll > Decimal::ZERO && self.daily_pnl < Decimal::ZERO {
            (self.daily_pnl.abs() / self.daily_start_bankroll) * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    pub fn platform_exposure(&self, platform: &Platform) -> Decimal {
        self.platform_exposure.get(platform).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn platform_exposure_pct(&self, platform: &Platform) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.platform_exposure(platform) / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn total_exposure(&self) -> Decimal {
        self.platform_exposure.values().sum()
    }

    pub fn total_exposure_pct(&self) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.total_exposure() / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn market_exposure(&self, market_id: &Uuid) -> Decimal {
        self.market_exposure.get(market_id).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn market_exposure_pct(&self, market_id: &Uuid) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.market_exposure(market_id) / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn add_market_exposure(&mut self, market_id: Uuid, amount: Decimal) {
        *self.market_exposure.entry(market_id).or_insert(Decimal::ZERO) += amount;
    }

    pub fn remove_market_exposure(&mut self, market_id: Uuid, amount: Decimal) {
        if let Some(exp) = self.market_exposure.get_mut(&market_id) {
            *exp = (*exp - amount).max(Decimal::ZERO);
            if *exp == Decimal::ZERO {
                self.market_exposure.remove(&market_id);
            }
        }
    }

    /// Credits the bankroll with realized PnL from an expired/settled market.
    /// Winning legs pay $1.00 per contract; losing legs pay $0.00. 
    pub fn record_settlement(&mut self, realized_pnl: Decimal) {
        self.total_bankroll += realized_pnl;
        if self.total_bankroll > self.peak_bankroll {
            self.peak_bankroll = self.total_bankroll;
        }
        tracing::info!(realized_pnl = %realized_pnl, bankroll = %self.total_bankroll, "Settlement credited to bankroll");
    }

    pub fn record_trade(&mut self, result: &TradeResult) {
        self.daily_pnl += result.profit;
        self.daily_fees += result.leg_a_fee + result.leg_b_fee;
        self.total_bankroll += result.profit;
        self.trades_today += 1;

        match result.status {
            TradeStatus::Success => {
                self.success_today += 1;
                self.exec_history.push_back((chrono::Utc::now(), true));
            }
            TradeStatus::Fail | TradeStatus::Partial => {
                self.fail_today += 1;
                self.exec_history.push_back((chrono::Utc::now(), false));
            }
        }

        if self.total_bankroll > self.peak_bankroll {
            self.peak_bankroll = self.total_bankroll;
        }

        // LOW-8 FIX: Evict execution tracking metrics based on time (24h) rather than a rigid 100 count.
        let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
        while let Some(&(time, _)) = self.exec_history.front() {
            if time < cutoff {
                self.exec_history.pop_front();
            } else {
                break;
            }
        }
        
        // MED-8 FIX: Provide a hard upper bound to prevent memory exhaustion during extreme volume
        while self.exec_history.len() > 10000 {
            self.exec_history.pop_front();
        }
        
        if !self.exec_history.is_empty() {
            let successes = self.exec_history.iter().filter(|&&(_, s)| s).count();
            self.exec_success_rate = Decimal::from(successes as u64) / Decimal::from(self.exec_history.len() as u64);
        }

        info!(profit = %result.profit, bankroll = %self.total_bankroll, daily_pnl = %self.daily_pnl, "Trade recorded");
    }

    pub fn add_exposure(&mut self, platform: Platform, amount: Decimal) {
        *self.platform_exposure.entry(platform).or_insert(Decimal::ZERO) += amount;
    }

    pub fn remove_exposure(&mut self, platform: Platform, amount: Decimal) {
        if let Some(exp) = self.platform_exposure.get_mut(&platform) {
            *exp = (*exp - amount).max(Decimal::ZERO);
        }
    }

    pub fn reset_daily(&mut self) {
        self.daily_pnl = Decimal::ZERO;
        self.daily_fees = Decimal::ZERO;
        self.daily_start_bankroll = self.total_bankroll;
        self.trades_today = 0;
        self.success_today = 0;
        self.fail_today = 0;
        self.day_start = Utc::now();
        // REMOVED: peak_bankroll reset. 
        // The Kelly calculator in src/risk/kelly.rs already handles recovery 
        // smoothly by restoring the fraction when drawdown < 5%. Resetting peak 
        // here causes lethal over-sizing during multi-day losing streaks.
        info!(bankroll = %self.total_bankroll, "Daily counters reset");
    }

    pub fn restore_state(&mut self, cumulative_profit: Decimal) {
        self.total_bankroll += cumulative_profit;
        self.peak_bankroll = self.total_bankroll.max(self.peak_bankroll);
        tracing::info!(cumulative_profit = %cumulative_profit, "Bankroll state restored from DB");
    }

    pub fn daily_snapshot(&self, kelly_utilization: Decimal) -> DailySnapshot {
        let success_rate = if self.trades_today > 0 {
            Decimal::from(self.success_today) / Decimal::from(self.trades_today)
        } else {
            Decimal::ZERO
        };

        DailySnapshot {
            date: Utc::now().date_naive(),
            bankroll: self.total_bankroll,
            gross_pnl: self.daily_pnl + self.daily_fees,
            fees_paid: self.daily_fees,
            net_pnl: self.daily_pnl,
            trades_count: self.trades_today,
            success_count: self.success_today,
            fail_count: self.fail_today,
            success_rate,
            peak_bankroll: self.peak_bankroll,
            drawdown_pct: self.drawdown_pct(),
            kelly_utilization,
            report_sent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_daily_reset_preserves_peak() {
        let mut bm = BankrollManager::new(dec!(1000));
        bm.record_settlement(dec!(500)); // total is 1500, peak is 1500
        assert_eq!(bm.peak_bankroll(), dec!(1500));
        bm.reset_daily();
        // M-10 FIX: Enforce via unit test that peak bankroll persists across daily resets
        assert_eq!(bm.peak_bankroll(), dec!(1500));
    }

    #[test]
    fn test_bankroll_record_trade_and_settlement() {
        let mut bm = BankrollManager::new(dec!(1000));
        
        let trade = TradeResult {
            trade_id: 1, opp_id: Uuid::new_v4(), market_id: Uuid::new_v4(),
            market_question: "".into(), leg_a_platform: Platform::Polymarket, leg_a_side: Side::Yes,
            leg_a_price: dec!(0.4), leg_a_size: dec!(10), leg_a_fill_price: dec!(0.4), leg_a_fee: dec!(0.1),
            leg_b_platform: Platform::Kalshi, leg_b_side: Side::No, leg_b_price: dec!(0.5), leg_b_size: dec!(10),
            leg_b_fill_price: dec!(0.5), leg_b_fee: dec!(0.1), raw_spread: dec!(0.1), net_spread: dec!(0.08),
            profit: dec!(0.8), status: TradeStatus::Success, failure_reason: None, execution_ms: 50,
            executed_at: chrono::Utc::now(), bankroll_after: dec!(0), bankroll_change_pct: dec!(0), approved_size: dec!(10)
        };
        
        bm.record_trade(&trade);
        assert_eq!(bm.total_bankroll(), dec!(1000.8));
        assert_eq!(bm.success_today(), 1);
        
        bm.record_settlement(dec!(10)); 
        assert_eq!(bm.total_bankroll(), dec!(1010.8));
        assert_eq!(bm.peak_bankroll(), dec!(1010.8));
    }
}
```

## File: src/db/sqlite.rs
```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

use super::migrations;
use super::traits::Database;
use crate::types::*;

/// SQLite-backed async implementation of [`Database`].
#[derive(Clone)]
pub struct SqliteDb {
    pool: SqlitePool,
}

impl SqliteDb {
    /// Open (or create) a SQLite database at `path` using async SQLx.
    pub async fn new(path: &str, pool_size: u32, busy_timeout_ms: u64) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut conn_opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", path))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_millis(busy_timeout_ms))
            .foreign_keys(true);

        // C-3 FIX: Support encryption-at-rest via PRAGMA key if DB_ENCRYPTION_KEY is provided
        if let Ok(key) = std::env::var("DB_ENCRYPTION_KEY") {
            conn_opts = conn_opts.pragma("key", key);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(pool_size)
            .connect_with(conn_opts)
            .await
            .context("failed to build SQLite connection pool")?;

        // Run migrations
        migrations::run_migrations(&pool).await?;

        // MED-8 FIX: Ensure partial index exists for the get_open_arb_count query
        // This guarantees O(1) lookup times for the 60-second synchronization loop
        // regardless of how large the historical positions table grows.
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_positions_open ON positions(market_id, opened_at) WHERE closed = 0")
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dec(s: &str) -> Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("Failed to parse Decimal from DB: {}", e))
}

fn dec_to_string(d: &Decimal) -> String {
    d.to_string()
}

fn platform_from_db(s: &str) -> Result<Platform> {
    match s {
        "Polymarket" => Ok(Platform::Polymarket),
        "Polymarket US" => Ok(Platform::PolymarketUs),
        "Kalshi" => Ok(Platform::Kalshi),
        "CDNA" => Ok(Platform::Cdna),
        "ForecastEx" => Ok(Platform::ForecastEx),
        _ => Err(anyhow::anyhow!("Unknown platform string in DB: {}", s)),
    }
}

fn side_from_db(s: &str) -> Result<Side> {
    match s {
        "YES" => Ok(Side::Yes),
        "NO" => Ok(Side::No),
        _ => Err(anyhow::anyhow!("Unknown side string in DB: {}", s)),
    }
}

async fn upsert_position_on<'e, E>(executor: E, pos: &Position) -> Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    if pos.id == 0 {
        sqlx::query(
            "INSERT INTO positions (market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
        )
        .bind(pos.market_id.to_string())
        .bind(pos.platform.to_string())
        .bind(pos.side.to_string())
        .bind(dec_to_string(&pos.quantity))
        .bind(dec_to_string(&pos.avg_entry_price))
        .bind(dec_to_string(&pos.unrealized_pnl))
        .bind(dt_to_str(&pos.opened_at))
        .bind(dt_to_str(&pos.updated_at))
        .execute(executor).await?;
    } else {
        sqlx::query(
            "INSERT INTO positions (id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT(id) DO UPDATE SET
               quantity = excluded.quantity,
               avg_entry_price = excluded.avg_entry_price,
               unrealized_pnl = excluded.unrealized_pnl,
               updated_at = excluded.updated_at"
        )
        .bind(pos.id)
        .bind(pos.market_id.to_string())
        .bind(pos.platform.to_string())
        .bind(pos.side.to_string())
        .bind(dec_to_string(&pos.quantity))
        .bind(dec_to_string(&pos.avg_entry_price))
        .bind(dec_to_string(&pos.unrealized_pnl))
        .bind(dt_to_str(&pos.opened_at))
        .bind(dt_to_str(&pos.updated_at))
        .execute(executor).await?;
    }
    Ok(())
}

fn trade_status_from_db(s: &str) -> TradeStatus {
    match s {
        "success" => TradeStatus::Success,
        "fail" => TradeStatus::Fail,
        "partial" => TradeStatus::Partial,
        _ => TradeStatus::Fail,
    }
}

fn market_status_from_db(s: &str) -> MarketStatus {
    match s {
        "active" => MarketStatus::Active,
        "suspended" => MarketStatus::Suspended,
        "resolved" => MarketStatus::Resolved,
        "expired" => MarketStatus::Expired,
        _ => MarketStatus::Active,
    }
}

fn category_from_db(s: &str) -> MarketCategory {
    match s {
        "sports" => MarketCategory::Sports,
        "politics" => MarketCategory::Politics,
        "finance" => MarketCategory::Finance,
        "crypto" => MarketCategory::Crypto,
        "weather" => MarketCategory::Weather,
        "culture" => MarketCategory::Culture,
        _ => MarketCategory::Other,
    }
}

fn category_to_str(c: &MarketCategory) -> &'static str {
    match c {
        MarketCategory::Sports => "sports",
        MarketCategory::Politics => "politics",
        MarketCategory::Finance => "finance",
        MarketCategory::Crypto => "crypto",
        MarketCategory::Weather => "weather",
        MarketCategory::Culture => "culture",
        MarketCategory::Other => "other",
    }
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn parse_naive_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|_| Utc::now().date_naive())
}

// ---------------------------------------------------------------------------
// Database trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Database for SqliteDb {
    // -- Markets --------------------------------------------------------

    async fn upsert_market(&self, market: &Market) -> Result<()> {
        let platforms_json = serde_json::to_string(&market.platforms)?;
        sqlx::query(
            "INSERT INTO markets (unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT(unified_id) DO UPDATE SET
               question = excluded.question,
               resolution_source = excluded.resolution_source,
               expiration = excluded.expiration,
               platforms = excluded.platforms,
               category = excluded.category,
               confidence = excluded.confidence,
               status = excluded.status,
               updated_at = excluded.updated_at"
        )
        .bind(market.unified_id.to_string())
        .bind(&market.question)
        .bind(&market.resolution_source)
        .bind(dt_to_str(&market.expiration))
        .bind(platforms_json)
        .bind(category_to_str(&market.category))
        .bind(market.confidence)
        .bind(market.status.to_string())
        .bind(dt_to_str(&market.created_at))
        .bind(dt_to_str(&market.updated_at))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_market(&self, id: &Uuid) -> Result<Option<Market>> {
        let row = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE unified_id = $1"
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            Ok(Some(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_active_markets(&self) -> Result<Vec<Market>> {
        let rows = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE status = 'active'"
        ).fetch_all(&self.pool).await?;
        
        let mut markets = Vec::new();
        for row in rows {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            markets.push(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            });
        }
        Ok(markets)
    }

    async fn get_suspended_markets(&self) -> Result<Vec<Market>> {
        let rows = sqlx::query(
            "SELECT unified_id, question, resolution_source, expiration, platforms, category, confidence, status, created_at, updated_at
             FROM markets WHERE status = 'suspended'"
        ).fetch_all(&self.pool).await?;
        
        let mut markets = Vec::new();
        for row in rows {
            let platforms_str: String = row.try_get(4)?;
            let platforms: HashMap<Platform, PlatformMarketInfo> = serde_json::from_str(&platforms_str).unwrap_or_default();
            markets.push(Market {
                unified_id: Uuid::parse_str(&row.try_get::<String, _>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
                question: row.try_get(1)?,
                resolution_source: row.try_get(2)?,
                expiration: parse_dt(&row.try_get::<String, _>(3)?),
                platforms,
                category: category_from_db(&row.try_get::<String, _>(5)?),
                confidence: row.try_get(6)?,
                status: market_status_from_db(&row.try_get::<String, _>(7)?),
                created_at: parse_dt(&row.try_get::<String, _>(8)?),
                updated_at: parse_dt(&row.try_get::<String, _>(9)?),
            });
        }
        Ok(markets)
    }

    async fn update_market_status(&self, id: &Uuid, status: MarketStatus) -> Result<()> {
        sqlx::query("UPDATE markets SET status = $1 WHERE unified_id = $2")
            .bind(status.to_string())
            .bind(id.to_string())
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Trades ---------------------------------------------------------

    async fn insert_trade(&self, trade: &TradeResult) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO trades (opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)"
        )
        .bind(trade.opp_id.to_string())
        .bind(trade.market_id.to_string())
        .bind(&trade.market_question)
        .bind(trade.leg_a_platform.to_string())
        .bind(trade.leg_a_side.to_string())
        .bind(dec_to_string(&trade.leg_a_price))
        .bind(dec_to_string(&trade.leg_a_size))
        .bind(dec_to_string(&trade.leg_a_fill_price))
        .bind(dec_to_string(&trade.leg_a_fee))
        .bind(trade.leg_b_platform.to_string())
        .bind(trade.leg_b_side.to_string())
        .bind(dec_to_string(&trade.leg_b_price))
        .bind(dec_to_string(&trade.leg_b_size))
        .bind(dec_to_string(&trade.leg_b_fill_price))
        .bind(dec_to_string(&trade.leg_b_fee))
        .bind(dec_to_string(&trade.raw_spread))
        .bind(dec_to_string(&trade.net_spread))
        .bind(dec_to_string(&trade.profit))
        .bind(trade.status.to_string())
        .bind(trade.failure_reason.clone())
        .bind(trade.execution_ms as i64)
        .bind(dt_to_str(&trade.executed_at))
        .bind(dec_to_string(&trade.bankroll_after))
        .bind(dec_to_string(&trade.bankroll_change_pct))
        .execute(&self.pool).await?;

        Ok(result.last_insert_rowid())
    }

    async fn get_trades_since(&self, since: DateTime<Utc>) -> Result<Vec<TradeResult>> {
        let rows = sqlx::query(
            "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= $1 ORDER BY executed_at ASC"
        )
        .bind(dt_to_str(&since))
        .fetch_all(&self.pool).await?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row_to_trade(&row)?);
        }
        Ok(trades)
    }

    async fn get_trades_for_date(&self, date: NaiveDate) -> Result<Vec<TradeResult>> {
        let start = format!("{}T00:00:00+00:00", date);
        let end = format!("{}T00:00:00+00:00", date + Days::new(1));
        
        let rows = sqlx::query(
            "SELECT id, opp_id, market_id, market_question, leg_a_platform, leg_a_side, leg_a_price, leg_a_size, leg_a_fill_price, leg_a_fee, leg_b_platform, leg_b_side, leg_b_price, leg_b_size, leg_b_fill_price, leg_b_fee, raw_spread, net_spread, profit, status, failure_reason, execution_ms, executed_at, bankroll_after, bankroll_change_pct
             FROM trades WHERE executed_at >= $1 AND executed_at < $2 ORDER BY executed_at ASC"
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool).await?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row_to_trade(&row)?);
        }
        Ok(trades)
    }

    async fn get_trade_count(&self) -> Result<i64> {
        // LOW-6: Use MAX(id) to prevent duplicate IDs if trades are pruned
        let count: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM trades")
            .fetch_one(&self.pool).await?;
        Ok(count)
    }

    // -- Positions ------------------------------------------------------

    async fn upsert_position(&self, pos: &Position) -> Result<()> {
        upsert_position_on(&self.pool, pos).await
    }

    async fn upsert_position_pair(&self, pos_a: &Position, pos_b: &Position) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        // The sqlx Transaction inherently wraps both inserts in a single atomic transaction.
        // Removed nested BEGIN IMMEDIATE which causes undefined behavior in SQLite.
        upsert_position_on(&mut *tx, pos_a).await?;
        upsert_position_on(&mut *tx, pos_b).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get_open_positions(&self) -> Result<Vec<Position>> {
        let rows = sqlx::query(
            "SELECT id, market_id, platform, side, quantity, avg_entry_price, unrealized_pnl, opened_at, updated_at
             FROM positions WHERE closed = 0"
        ).fetch_all(&self.pool).await?;
        
        let mut positions = Vec::new();
        for row in rows {
            positions.push(row_to_position(&row)?);
        }
        Ok(positions)
    }

    async fn get_open_arb_count(&self) -> Result<usize> {
        // HIGH-6 FIX: Use DISTINCT market_id to accurately count open arbitrage positions
        // regardless of minor timestamp variations across multiple legs.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT market_id) FROM positions WHERE closed = 0"
        ).fetch_one(&self.pool).await?;
        Ok(count as usize)
    }

    async fn get_cumulative_profit(&self) -> Result<Decimal> {
        let profit_str: String = sqlx::query_scalar("SELECT COALESCE(SUM(profit), '0') FROM trades WHERE status = 'success'")
            .fetch_one(&self.pool).await?;
        dec(&profit_str)
    }

    async fn close_position(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE positions SET closed = 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Balances -------------------------------------------------------

    async fn update_balance(&self, bal: &PlatformBalance) -> Result<()> {
        sqlx::query(
            "INSERT INTO platform_balances (platform, available, reserved, pending_settlement, total, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT(platform) DO UPDATE SET
               available = excluded.available,
               reserved = excluded.reserved,
               pending_settlement = excluded.pending_settlement,
               total = excluded.total,
               updated_at = excluded.updated_at"
        )
        .bind(bal.platform.to_string())
        .bind(dec_to_string(&bal.available))
        .bind(dec_to_string(&bal.reserved))
        .bind(dec_to_string(&bal.pending_settlement))
        .bind(dec_to_string(&bal.total))
        .bind(dt_to_str(&bal.updated_at))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_balance(&self, platform: Platform) -> Result<Option<PlatformBalance>> {
        let row = sqlx::query(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM platform_balances WHERE platform = $1"
        )
        .bind(platform.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            Ok(Some(PlatformBalance {
                platform: platform_from_db(&row.try_get::<String, _>(0)?)?,
                available: dec(&row.try_get::<String, _>(1)?)?,
                reserved: dec(&row.try_get::<String, _>(2)?)?,
                pending_settlement: dec(&row.try_get::<String, _>(3)?)?,
                total: dec(&row.try_get::<String, _>(4)?)?,
                updated_at: parse_dt(&row.try_get::<String, _>(5)?),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_all_balances(&self) -> Result<Vec<PlatformBalance>> {
        let rows = sqlx::query(
            "SELECT platform, available, reserved, pending_settlement, total, updated_at
             FROM platform_balances"
        ).fetch_all(&self.pool).await?;

        let mut balances = Vec::new();
        for row in rows {
            balances.push(PlatformBalance {
                platform: platform_from_db(&row.try_get::<String, _>(0)?)?,
                available: dec(&row.try_get::<String, _>(1)?)?,
                reserved: dec(&row.try_get::<String, _>(2)?)?,
                pending_settlement: dec(&row.try_get::<String, _>(3)?)?,
                total: dec(&row.try_get::<String, _>(4)?)?,
                updated_at: parse_dt(&row.try_get::<String, _>(5)?),
            });
        }
        Ok(balances)
    }

    // -- Daily snapshots ------------------------------------------------

    async fn insert_daily_snapshot(&self, snap: &DailySnapshot) -> Result<()> {
        sqlx::query(
            "INSERT INTO daily_snapshots (date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT(date) DO UPDATE SET
               bankroll = excluded.bankroll,
               gross_pnl = excluded.gross_pnl,
               fees_paid = excluded.fees_paid,
               net_pnl = excluded.net_pnl,
               trades_count = excluded.trades_count,
               success_count = excluded.success_count,
               fail_count = excluded.fail_count,
               success_rate = excluded.success_rate,
               peak_bankroll = excluded.peak_bankroll,
               drawdown_pct = excluded.drawdown_pct,
               kelly_utilization = excluded.kelly_utilization"
        )
        .bind(snap.date.to_string())
        .bind(dec_to_string(&snap.bankroll))
        .bind(dec_to_string(&snap.gross_pnl))
        .bind(dec_to_string(&snap.fees_paid))
        .bind(dec_to_string(&snap.net_pnl))
        .bind(snap.trades_count)
        .bind(snap.success_count)
        .bind(snap.fail_count)
        .bind(dec_to_string(&snap.success_rate))
        .bind(dec_to_string(&snap.peak_bankroll))
        .bind(dec_to_string(&snap.drawdown_pct))
        .bind(dec_to_string(&snap.kelly_utilization))
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_daily_snapshot(&self, date: NaiveDate) -> Result<Option<DailySnapshot>> {
        let row = sqlx::query(
            "SELECT date, bankroll, gross_pnl, fees_paid, net_pnl, trades_count, success_count, fail_count, success_rate, peak_bankroll, drawdown_pct, kelly_utilization, report_sent
             FROM daily_snapshots WHERE date = $1"
        )
        .bind(date.to_string())
        .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            Ok(Some(DailySnapshot {
                date: parse_naive_date(&row.try_get::<String, _>(0)?),
                bankroll: dec(&row.try_get::<String, _>(1)?)?,
                gross_pnl: dec(&row.try_get::<String, _>(2)?)?,
                fees_paid: dec(&row.try_get::<String, _>(3)?)?,
                net_pnl: dec(&row.try_get::<String, _>(4)?)?,
                trades_count: row.try_get(5)?,
                success_count: row.try_get(6)?,
                fail_count: row.try_get(7)?,
                success_rate: dec(&row.try_get::<String, _>(8)?)?,
                peak_bankroll: dec(&row.try_get::<String, _>(9)?)?,
                drawdown_pct: dec(&row.try_get::<String, _>(10)?)?,
                kelly_utilization: dec(&row.try_get::<String, _>(11)?)?,
                report_sent: row.try_get::<i32, _>(12)? != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn mark_report_sent(&self, date: NaiveDate) -> Result<()> {
        sqlx::query("UPDATE daily_snapshots SET report_sent = 1 WHERE date = $1")
            .bind(date.to_string())
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Audit ----------------------------------------------------------

    async fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let data_str = serde_json::to_string(&entry.data)?;
        sqlx::query("INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES ($1,$2,$3,$4)")
            .bind(entry.timestamp_ns as i64)
            .bind(&entry.module)
            .bind(&entry.event_type)
            .bind(data_str)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn append_audit_batch(&self, entries: &[AuditEntry]) -> Result<()> {
        if entries.is_empty() { return Ok(()); }
        let mut tx = self.pool.begin().await?;
        for entry in entries {
            let data_str = serde_json::to_string(&entry.data)?;
            sqlx::query("INSERT INTO audit_log (timestamp_ns, module, event_type, data) VALUES ($1,$2,$3,$4)")
                .bind(entry.timestamp_ns as i64)
                .bind(&entry.module)
                .bind(&entry.event_type)
                .bind(data_str)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn log_config_change(&self, key: &str, old_val: &str, new_val: &str) -> Result<()> {
        sqlx::query("INSERT INTO config_history (changed_at, key, old_value, new_value) VALUES ($1,$2,$3,$4)")
            .bind(dt_to_str(&Utc::now()))
            .bind(key)
            .bind(old_val)
            .bind(new_val)
            .execute(&self.pool).await?;
        Ok(())
    }

    // -- Meta -----------------------------------------------------------

    async fn checkpoint_wal(&self) -> Result<()> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await?;
        Ok(())
    }

    async fn prune_audit_log(&self, keep_days: u32) -> Result<()> {
        let cutoff_ns = crate::types::now_ns() - (keep_days as u64 * 24 * 3600 * 1_000_000_000);
        sqlx::query("DELETE FROM audit_log WHERE timestamp_ns < $1")
            .bind(cutoff_ns as i64)
            .execute(&self.pool).await?;
        
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await?;
            
        let cutoff_dt = Utc::now() - chrono::Duration::days(keep_days as i64);
        sqlx::query("DELETE FROM config_history WHERE changed_at < $1")
            .bind(dt_to_str(&cutoff_dt))
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn db_size_bytes(&self) -> Result<u64> {
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count").fetch_one(&self.pool).await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size").fetch_one(&self.pool).await?;
        Ok((page_count * page_size) as u64)
    }

    async fn backup_to_file(&self, dest_path: &str) -> Result<()> {
        anyhow::ensure!(!dest_path.contains(".."), "Backup path contains directory traversal");
        // SECURITY: Single-quote rejection MUST remain first — it is the SQL injection barrier.
        // VACUUM INTO does not support bind parameters, so this is our only defense.
        anyhow::ensure!(!dest_path.contains('\''), "Backup path contains single quote — potential SQL injection");
        anyhow::ensure!(!dest_path.contains('\0'), "Backup path contains null byte");
        anyhow::ensure!(dest_path.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.'), "Backup path contains invalid characters");
        
        let query = format!("VACUUM INTO '{}'", dest_path);
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Row-to-struct helpers
// ---------------------------------------------------------------------------

fn row_to_trade(row: &sqlx::sqlite::SqliteRow) -> Result<TradeResult> {
    Ok(TradeResult {
        trade_id: row.try_get("id")?,
        opp_id: Uuid::parse_str(&row.try_get::<String, _>("opp_id")?).map_err(|e| anyhow::anyhow!("Invalid opp_id UUID in DB: {}", e))?,
        market_id: Uuid::parse_str(&row.try_get::<String, _>("market_id")?).map_err(|e| anyhow::anyhow!("Invalid market_id UUID in DB: {}", e))?,
        market_question: row.try_get("market_question")?,
        leg_a_platform: platform_from_db(&row.try_get::<String, _>("leg_a_platform")?)?,
        leg_a_side: side_from_db(&row.try_get::<String, _>("leg_a_side")?)?,
        leg_a_price: dec(&row.try_get::<String, _>("leg_a_price")?)?,
        leg_a_size: dec(&row.try_get::<String, _>("leg_a_size")?)?,
        leg_a_fill_price: dec(&row.try_get::<String, _>("leg_a_fill_price")?)?,
        leg_a_fee: dec(&row.try_get::<String, _>("leg_a_fee")?)?,
        leg_b_platform: platform_from_db(&row.try_get::<String, _>("leg_b_platform")?)?,
        leg_b_side: side_from_db(&row.try_get::<String, _>("leg_b_side")?)?,
        leg_b_price: dec(&row.try_get::<String, _>("leg_b_price")?)?,
        leg_b_size: dec(&row.try_get::<String, _>("leg_b_size")?)?,
        leg_b_fill_price: dec(&row.try_get::<String, _>("leg_b_fill_price")?)?,
        leg_b_fee: dec(&row.try_get::<String, _>("leg_b_fee")?)?,
        raw_spread: dec(&row.try_get::<String, _>("raw_spread")?)?,
        net_spread: dec(&row.try_get::<String, _>("net_spread")?)?,
        profit: dec(&row.try_get::<String, _>("profit")?)?,
        status: trade_status_from_db(&row.try_get::<String, _>("status")?),
        failure_reason: row.try_get("failure_reason")?,
        execution_ms: row.try_get::<i64, _>("execution_ms")? as u64,
        executed_at: parse_dt(&row.try_get::<String, _>("executed_at")?),
        bankroll_after: dec(&row.try_get::<String, _>("bankroll_after")?)?,
        bankroll_change_pct: dec(&row.try_get::<String, _>("bankroll_change_pct")?)?,
        approved_size: Decimal::ZERO,
    })
}

fn row_to_position(row: &sqlx::sqlite::SqliteRow) -> Result<Position> {
    Ok(Position {
        id: row.try_get("id")?,
        market_id: Uuid::parse_str(&row.try_get::<String, _>("market_id")?).map_err(|e| anyhow::anyhow!("Invalid market_id UUID in DB: {}", e))?,
        platform: platform_from_db(&row.try_get::<String, _>("platform")?)?,
        side: side_from_db(&row.try_get::<String, _>("side")?)?,
        quantity: dec(&row.try_get::<String, _>("quantity")?)?,
        avg_entry_price: dec(&row.try_get::<String, _>("avg_entry_price")?)?,
        unrealized_pnl: dec(&row.try_get::<String, _>("unrealized_pnl")?)?,
        opened_at: parse_dt(&row.try_get::<String, _>("opened_at")?),
        updated_at: parse_dt(&row.try_get::<String, _>("updated_at")?),
    })
}
```

## File: src/execution/executor.rs
```rust
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::db::Database;
use crate::types::*;
use super::polymarket_client::PolymarketClient;
use super::kalshi_client::KalshiClient;
use super::cdna_client::CdnaClient;
use super::forecastex_client::ForecastExClient;

#[derive(Debug, Clone)]
pub struct OrderResult {
    pub filled: bool,
    pub fill_price: Decimal,
    pub fill_size: Decimal,
    pub fee: Decimal,
    pub order_id: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderAction {
    Buy,
    Sell,
}

#[async_trait::async_trait]
pub trait PlatformOrderClient: Send + Sync {
    async fn submit_order(
        &self,
        market_id: &str,
        action: OrderAction,
        side: Side,
        price: Decimal,
        size: Decimal,
        fee_rate_bps: u32,
    ) -> Result<OrderResult>;

    async fn cancel_order(&self, order_id: &str) -> Result<()>;
}

pub struct ExecutionEngine {
    rx: mpsc::Receiver<ValidatedOpportunity>,
    trade_result_tx: mpsc::Sender<TradeResult>,
    alert_tx: mpsc::Sender<AlertMessage>,
    db: Arc<dyn Database>,
    uob: Arc<tokio::sync::RwLock<crate::engine::order_book::UnifiedOrderBook>>,
    polymarket_client: Option<PolymarketClient>,
    kalshi_client: Option<KalshiClient>,
    cdna_client: Option<CdnaClient>,
    forecastex_client: Option<ForecastExClient>,
    trade_counter: i64,
    executed_opps: lru::LruCache<uuid::Uuid, ()>,
}

impl ExecutionEngine {
    pub fn new(
        rx: mpsc::Receiver<ValidatedOpportunity>,
        trade_result_tx: mpsc::Sender<TradeResult>,
        alert_tx: mpsc::Sender<AlertMessage>,
        db: Arc<dyn Database>,
        uob: Arc<tokio::sync::RwLock<crate::engine::order_book::UnifiedOrderBook>>,
        polymarket_client: Option<PolymarketClient>,
        kalshi_client: Option<KalshiClient>,
        cdna_client: Option<CdnaClient>,
        forecastex_client: Option<ForecastExClient>,
        initial_trade_count: i64,
    ) -> Self {
        Self {
            rx,
            trade_result_tx,
            alert_tx,
            db,
            uob,
            polymarket_client,
            kalshi_client,
            cdna_client,
            forecastex_client,
            trade_counter: initial_trade_count,
            executed_opps: lru::LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
        }
    }

    pub async fn run(mut self) {
        info!("Execution engine started");
        while let Some(opp) = self.rx.recv().await {
            if let Err(e) = self.execute_arbitrage(opp).await {
                error!(error = %e, "Arbitrage execution error");
            }
        }
        info!("Execution engine stopped");
    }

    async fn execute_arbitrage(&mut self, validated: ValidatedOpportunity) -> Result<()> {
        let opp = &validated.opportunity;
        let start = Instant::now();
        
        let halves: Vec<&str> = opp.market_question.split(" / ").collect();
        if halves.len() == 2 {
            let nums_a: Vec<f64> = halves[0].split_whitespace().filter_map(|w| w.replace("$", "").replace(",", "").parse().ok()).collect();
            let nums_b: Vec<f64> = halves[1].split_whitespace().filter_map(|w| w.replace("$", "").replace(",", "").parse().ok()).collect();
            if nums_a != nums_b && (!nums_a.is_empty() || !nums_b.is_empty()) {
                tracing::error!(question = %opp.market_question, "Mismatched numerical targets in execution. Aborting trade.");
                return Ok(());
            }
        }
        
        // HIGH-4: Idempotency Guard
        if self.executed_opps.put(opp.opp_id, ()).is_some() {
            tracing::warn!(opp_id = %opp.opp_id, "Duplicate opportunity execution prevented");
            return Ok(());
        }

        // CRITICAL FIX: The TTL Guard
        // Drop the opportunity immediately if it sat in the async queue longer than its Time-To-Live.
        // Executing stale arbs guarantees negative PnL.
        let current_time_ns = crate::types::now_ns();
        let expiration_ns = opp.detected_at + (opp.ttl_ms as u64 * 1_000_000);
        
        let current_trade_id = self.trade_counter;
        self.trade_counter += 1;

        if current_time_ns > expiration_ns {
            let delay_ms = (current_time_ns - opp.detected_at) / 1_000_000;
            tracing::warn!(
                opp_id = %opp.opp_id, 
                delay_ms, 
                "Opportunity TTL expired in execution queue — dropping to prevent slippage"
            );
            
            let trade_result = TradeResult {
                trade_id: current_trade_id,
                opp_id: opp.opp_id,
                market_id: opp.market_id,
                market_question: opp.market_question.clone(),
                leg_a_platform: opp.leg_a.platform,
                leg_a_side: opp.leg_a.side,
                leg_a_price: opp.leg_a.price,
                leg_a_size: Decimal::ZERO,
                leg_a_fill_price: Decimal::ZERO,
                leg_a_fee: Decimal::ZERO,
                leg_b_platform: opp.leg_b.platform,
                leg_b_side: opp.leg_b.side,
                leg_b_price: opp.leg_b.price,
                leg_b_size: Decimal::ZERO,
                leg_b_fill_price: Decimal::ZERO,
                leg_b_fee: Decimal::ZERO,
                raw_spread: opp.raw_spread,
                net_spread: opp.net_spread,
                profit: Decimal::ZERO,
                status: TradeStatus::Fail,
                failure_reason: Some("Opportunity TTL expired in execution queue".into()),
                execution_ms: 0,
                executed_at: chrono::Utc::now(),
                bankroll_after: Decimal::ZERO,
                bankroll_change_pct: Decimal::ZERO,
                approved_size: validated.approved_size,
            };
            let _ = self.trade_result_tx.try_send(trade_result);
            return Ok(());
        }

        // H-1 FIX: Pre-execution price slippage guard
        {
            let uob_guard = self.uob.read().await;
            let book_a = uob_guard.get_book(&opp.market_id, &opp.leg_a.platform);
            let book_b = uob_guard.get_book(&opp.market_id, &opp.leg_b.platform);
            
            if let (Some(ba), Some(bb)) = (book_a, book_b) {
                let (current_price_a, current_price_b) = match (opp.leg_a.side, opp.leg_b.side) {
                    (Side::Yes, Side::No) => (ba.best_ask().map(|x| x.0), bb.best_bid().map(|x| Decimal::ONE - x.0)),
                    (Side::No, Side::Yes) => (ba.best_bid().map(|x| Decimal::ONE - x.0), bb.best_ask().map(|x| x.0)),
                    _ => (None, None),
                };

                if let (Some(pa), Some(pb)) = (current_price_a, current_price_b) {
                    let current_raw_spread = Decimal::ONE - pa - pb;
                    // If spread shrunk by over 50%, abort execution
                    if current_raw_spread < opp.raw_spread * rust_decimal_macros::dec!(0.5) {
                        tracing::warn!(
                            opp_id = %opp.opp_id,
                            old_spread = %opp.raw_spread,
                            new_spread = %current_raw_spread,
                            "Pre-execution slippage guard triggered: spread closed before execution. Aborting."
                        );
                        return Ok(());
                    }
                } else {
                    tracing::warn!("Pre-execution slippage guard: Order book missing depth. Aborting.");
                    return Ok(());
                }
            }
        }

        info!(
            opp_id = %opp.opp_id,
            market = %opp.market_question,
            net_spread = %opp.net_spread,
            size = %validated.approved_size,
            "Executing arbitrage"
        );

        let (first_leg, second_leg) = self.order_legs(opp);

        let first_result = self.execute_leg(
            &first_leg.platform,
            &first_leg.platform_market_id,
            OrderAction::Buy,
            first_leg.side,
            first_leg.price,
            validated.approved_size,
            first_leg.fee_rate_bps, // Pass actual BPS rate
        ).await;

        let (leg_a_result, leg_b_result) = match first_result {
            Ok(first_fill) if first_fill.filled => {
                let hedge_size = first_fill.fill_size; 
                let second_result = self.execute_leg(
                    &second_leg.platform,
                    &second_leg.platform_market_id,
                    OrderAction::Buy,
                    second_leg.side,
                    second_leg.price,
                    hedge_size,
                    second_leg.fee_rate_bps, // Pass actual BPS rate
                ).await;

                match second_result {
                    Ok(second_fill) if second_fill.filled => {
                        (first_fill, second_fill)
                    }
                    Ok(second_fill) => {
                        warn!(opp_id = %opp.opp_id, "Hedge leg failed, attempting unwind");
                        let mut final_first_fill = first_fill.clone();
                        
                        if let Ok(unwind_fill) = self.attempt_unwind(first_leg, &first_fill).await {
                            // Calculate exact realized loss from the round-trip FOK sell
                            let buy_cost = first_fill.fill_size * first_fill.fill_price + first_fill.fee;
                            let sell_revenue = unwind_fill.fill_size * unwind_fill.fill_price;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee;
                            
                            // Zero out size to prevent DB position tracking, but pack the loss into the fee
                            // so `compute_result` logs the exact financial hit.
                            final_first_fill.fill_size = Decimal::ZERO;
                            final_first_fill.fee = realized_loss;
                            final_first_fill.error = Some("Leg B failed, automated unwind successful".into());
                        }
                        
                        (final_first_fill, second_fill)
                    }
                    Err(e) => {
                        error!(error = %e, "Hedge leg error, attempting unwind");
                        let mut final_first_fill = first_fill.clone();
                        
                        if let Ok(unwind_fill) = self.attempt_unwind(first_leg, &first_fill).await {
                            let buy_cost = first_fill.fill_size * first_fill.fill_price + first_fill.fee;
                            let sell_revenue = unwind_fill.fill_size * unwind_fill.fill_price;
                            let realized_loss = (buy_cost - sell_revenue) + unwind_fill.fee;
                            
                            final_first_fill.fill_size = Decimal::ZERO;
                            final_first_fill.fee = realized_loss;
                            final_first_fill.error = Some("Leg B error, automated unwind successful".into());
                        }
                        
                        let failed2 = OrderResult {
                            filled: false,
                            fill_price: Decimal::ZERO,
                            fill_size: Decimal::ZERO,
                            fee: Decimal::ZERO,
                            order_id: String::new(),
                            error: Some(e.to_string()),
                        };
                        
                        // Return the stranded leg unmodified if unwind fails so the DB tracks it correctly
                        (final_first_fill, failed2)
                    }
                }
            }
            Ok(first_fill) => {
                let failed = OrderResult {
                    filled: false,
                    fill_price: Decimal::ZERO,
                    fill_size: Decimal::ZERO,
                    fee: Decimal::ZERO,
                    order_id: String::new(),
                    error: None,
                };
                (first_fill, failed)
            }
            Err(e) => {
                error!(error = %e, "First leg execution error");
                let failed = OrderResult {
                    filled: false,
                    fill_price: Decimal::ZERO,
                    fill_size: Decimal::ZERO,
                    fee: Decimal::ZERO,
                    order_id: String::new(),
                    error: Some(e.to_string()),
                };
                let failed2 = failed.clone();
                (failed, failed2)
            }
        };

    let execution_ms = start.elapsed().as_millis() as u64;

        let (status, profit, failure_reason) = self.compute_result(
            &leg_a_result, &leg_b_result, opp,
        );

        // CRITICAL FIX (4-B): The execution engine sorts legs by liquidity to reduce slippage risk.
        // We must map the execution results back to the original Opportunity's Leg A and Leg B
        // to prevent database/audit-trail cross-contamination.
        let (actual_leg_a_result, actual_leg_b_result) = if first_leg.platform == opp.leg_a.platform {
            (&leg_a_result, &leg_b_result)
        } else {
            (&leg_b_result, &leg_a_result)
        };

        let trade_result = TradeResult {
            trade_id: current_trade_id,
            opp_id: opp.opp_id,
            market_id: opp.market_id,
            market_question: opp.market_question.clone(),
            approved_size: validated.approved_size,
            leg_a_platform: opp.leg_a.platform,
            leg_a_side: opp.leg_a.side,
            leg_a_price: opp.leg_a.price,
            leg_a_size: actual_leg_a_result.fill_size,
            leg_a_fill_price: actual_leg_a_result.fill_price,
            leg_a_fee: actual_leg_a_result.fee,
            leg_b_platform: opp.leg_b.platform,
            leg_b_side: opp.leg_b.side,
            leg_b_price: opp.leg_b.price,
            leg_b_size: actual_leg_b_result.fill_size,
            leg_b_fill_price: actual_leg_b_result.fill_price,
            leg_b_fee: actual_leg_b_result.fee,
            raw_spread: opp.raw_spread,
            net_spread: opp.net_spread,
            profit,
            status,
            failure_reason,
            execution_ms,
            executed_at: Utc::now(),
            bankroll_after: Decimal::ZERO,
            bankroll_change_pct: Decimal::ZERO,
        };

        // Pass the result directly back to the orchestrator.
        // The executor MUST NOT dispatch to Telegram or SQLite. 
        if let Err(e) = self.trade_result_tx.try_send(trade_result) {
            tracing::error!(error = %e, "Trade result channel full — dropping notification");
        }

        Ok(())
    }

    fn order_legs<'a>(&self, opp: &'a ArbitrageOpportunity) -> (&'a LegDetail, &'a LegDetail) {
        let a_liquidity = opp.leg_a.available_size;
        let b_liquidity = opp.leg_b.available_size;

        if a_liquidity <= b_liquidity {
            (&opp.leg_a, &opp.leg_b) 
        } else {
            (&opp.leg_b, &opp.leg_a)
        }
    }

    async fn execute_leg(
        &self,
        platform: &Platform,
        market_id: &str,
        action: OrderAction,
        side: Side,
        price: Decimal,
        size: Decimal,
        fee_rate_bps: u32,
    ) -> Result<OrderResult> {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                if let Some(client) = &self.polymarket_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Polymarket client not configured")
                }
            }
            Platform::Kalshi => {
                if let Some(client) = &self.kalshi_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("Kalshi client not configured")
                }
            }
            Platform::Cdna => {
                if let Some(client) = &self.cdna_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("CDNA client not configured")
                }
            }
            Platform::ForecastEx => {
                if let Some(client) = &self.forecastex_client {
                    client.submit_order(market_id, action, side, price, size, fee_rate_bps).await
                } else {
                    anyhow::bail!("ForecastEx client not configured")
                }
            }
        }
    }


    /// Executes a synthetic automated unwind.
    /// By natively SELLING the stranded contracts back to the resting bids, 
    /// we cap our risk instantly and free up capital without locking collateral.
    async fn attempt_unwind(
        &self,
        stranded_leg: &LegDetail,
        original_fill: &OrderResult,
    ) -> Result<OrderResult> {
        // Sell at 0.01 to aggressively cross the spread and ensure the FOK SELL order
        // executes against whatever bids are resting on the book.
        let aggressive_sell_price = rust_decimal_macros::dec!(0.01);

        warn!(
            platform = %stranded_leg.platform,
            market_id = %stranded_leg.platform_market_id,
            stranded_side = %stranded_leg.side,
            size = %original_fill.fill_size,
            "Hedge failed. Executing aggressive FOK SELL unwind to dump inventory."
        );

        let unwind_result = self.execute_leg(
            &stranded_leg.platform,
            &stranded_leg.platform_market_id,
            OrderAction::Sell,
            stranded_leg.side, // Same side! We sell the exact inventory we hold.
            aggressive_sell_price,
            original_fill.fill_size,
            stranded_leg.fee_rate_bps,
        ).await;

        match unwind_result {
            Ok(fill) if fill.filled => {
                let msg = format!(
                    "⚠️ <b>AUTOMATED UNWIND SUCCESSFUL</b> ⚠️\n\n\
                     Platform: {}\nMarket: {}\nUnwound: {} {}\n\
                     <b>Delta exposure neutralized.</b>",
                     stranded_leg.platform, stranded_leg.platform_market_id, fill.fill_size, stranded_leg.side
                );
                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert { 
                    severity: "warning".into(), 
                    message: msg 
                });
                Ok(fill)
            }
            Ok(_) | Err(_) => {
                let err_msg = unwind_result.err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "FOK Unwind Rejected by matching engine".into());
                
                error!(error = %err_msg, "Automated unwind failed. Naked exposure remains.");
                
                let msg = format!(
                    "🚨 <b>CRITICAL: UNWIND FAILED - NAKED EXPOSURE</b> 🚨\n\n\
                     Platform: {}\nMarket: {}\nStranded Size: {} {}\n\
                     Error: {}\n\
                     <b>MANUAL INTERVENTION REQUIRED IMMEDIATELY.</b>",
                     stranded_leg.platform, stranded_leg.platform_market_id, 
                     original_fill.fill_size, stranded_leg.side, err_msg
                );
                
                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert { 
                    severity: "critical".into(), 
                    message: msg 
                });
                
                anyhow::bail!("Automated unwind failed: {}", err_msg)
            }
        }
    }

    fn compute_result(
        &self,
        leg_a: &OrderResult,
        leg_b: &OrderResult,
        _opp: &ArbitrageOpportunity,
    ) -> (TradeStatus, Decimal, Option<String>) {
        if leg_a.filled && leg_b.filled {
            let total_cost = leg_a.fill_price + leg_b.fill_price;
            let gross_profit = (Decimal::ONE - total_cost) * leg_a.fill_size.min(leg_b.fill_size);
            let net_profit = gross_profit - leg_a.fee - leg_b.fee;
            
            let status = if leg_a.fill_size == leg_b.fill_size {
                TradeStatus::Success
            } else {
                TradeStatus::Partial
            };
            
            (status, net_profit, None)
        } else if leg_a.filled && !leg_b.filled {
            if leg_a.fill_size == Decimal::ZERO && leg_a.fee > Decimal::ZERO {
                let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge failed, unwind successful".into());
                (TradeStatus::Fail, -leg_a.fee, Some(reason))
            } else {
                let filled_value = leg_a.fill_price * leg_a.fill_size;
                let dynamic_penalty_pct = _opp.raw_spread
                    .max(rust_decimal_macros::dec!(0.02))
                    .min(rust_decimal_macros::dec!(0.10));
                let dynamic_unwind_slippage = filled_value * dynamic_penalty_pct; 
                let estimated_loss = dynamic_unwind_slippage + leg_a.fee; 
                let reason = leg_b.error.clone().unwrap_or_else(|| "Hedge leg failed to fill".into());
                (TradeStatus::Fail, -estimated_loss, Some(reason))
            }
        } else {
            let reason = leg_a.error.clone().unwrap_or_else(|| "First leg failed to fill".into());
            (TradeStatus::Fail, Decimal::ZERO, Some(reason))
        }
    }
}
```

## File: src/execution/polymarket_client.rs
```rust
use anyhow::{Context, Result};
use alloy::primitives::{Address, U256};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::eip712::PolymarketSigner;
use crate::types::Side;

#[derive(Clone)]
pub struct PolymarketClient {
    http: reqwest::Client,
    rest_url: String,
    signer: PolymarketSigner,
    api_key: zeroize::Zeroizing<String>,
    api_secret: zeroize::Zeroizing<String>,
    api_passphrase: zeroize::Zeroizing<String>,
}

#[derive(Serialize)]
struct CreateOrderRequest {
    order: OrderPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    token_id: String,
    maker_amount: String,
    taker_amount: String,
    side: String,
    fee_rate_bps: String,
    nonce: String,
    expiration: String,
    signature: String,
    signature_type: u8,
    order_type: String,
}

#[derive(Deserialize)]
struct OrderResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    order_id: Option<String>,
    #[serde(default)]
    error_msg: Option<String>,
}

impl PolymarketClient {
    pub fn new(
        rest_url: String,
        signer: PolymarketSigner,
        api_key: String,
        api_secret: String,
        api_passphrase: String,
    ) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5));
        
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = reqwest::tls::Certificate::from_pem(&cert_pem) {
                builder = builder.add_root_certificate(cert);
            }
        }
            
        let http = builder.build().expect("failed to build Polymarket HTTP client");
        Self {
            http,
            rest_url,
            signer,
            api_key: zeroize::Zeroizing::new(api_key),
            api_secret: zeroize::Zeroizing::new(api_secret),
            api_passphrase: zeroize::Zeroizing::new(api_passphrase),
        }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for PolymarketClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, fee_rate_bps: u32) -> Result<OrderResult> {
        // MED-5 FIX: Rate limit Polymarket submissions
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // CRITICAL FIX: Polymarket CTF does not support naked short selling. To bet NO, we must trade 
        // the specific NO token ID. We parse the dual-token string provided by discovery.
        let tokens: Vec<&str> = market_id.split(',').collect();
        let target_token_id = if side == Side::Yes { tokens[0] } else { tokens.get(1).unwrap_or(&tokens[0]) };
        
        let (side_str, side_u8) = match action {
            OrderAction::Buy => ("BUY", 0u8),
            OrderAction::Sell => ("SELL", 1u8),
        };
        
        info!(target_token_id, action = side_str, side = ?side, price = %price, size = %size, fee_rate_bps, "Submitting Polymarket order");

        let scale = Decimal::from(1_000_000u64);
        
        // CRITICAL FIX: The `price` passed from the spread engine and unwind watchdog is 
        // already native to the specific Token ID we are trading. Do not invert it again.
        let pm_price = price;

        let (maker_amount_scaled, taker_amount_scaled) = match action {
            OrderAction::Buy => {
                // Buying: Maker gives USDC (price * size), Taker gets Tokens (size)
                ((size * pm_price * scale).floor(), (size * scale).floor())
            }
            OrderAction::Sell => {
                // Selling: Maker gives Tokens (size), Taker gets USDC (price * size)
                ((size * scale).floor(), (size * pm_price * scale).floor())
            }
        };

        let maker_amount_u256 = U256::from_str(&maker_amount_scaled.to_string()).unwrap_or(U256::ZERO);
        let taker_amount_u256 = U256::from_str(&taker_amount_scaled.to_string()).unwrap_or(U256::ZERO);
        
        // CRITICAL FIX: We must parse the specific `target_token_id` (e.g. "12345") 
        // into the EIP-712 signature, not the raw comma-separated `market_id` string.
        let token_id_u256 = U256::from_str(target_token_id)
            .map_err(|_| anyhow::anyhow!("Invalid Polymarket token ID: {}", target_token_id))?;

        let fee_rate_bps_u256 = U256::from(fee_rate_bps);

        let now = chrono::Utc::now();
        
        // MED-8: Enforce atomic uniqueness on sub-millisecond execution bursts
        static NONCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = NONCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // LOW-9: Use millis * 1M to avoid timestamp_nanos_opt None on overflow
        let nonce_val = (now.timestamp_millis() as u64 * 1_000_000) + counter;
        let nonce = U256::from(nonce_val);
        
        // Fix: 5 minutes is dangerously long for an HFT signature. 
        // 30 seconds caps our risk of resting order sniping on network lag.
        let expiration_u256 = U256::from((now.timestamp() + 30) as u64);

        let maker_addr = self.signer.address();

        let signature = self.signer.sign_order(
            nonce, maker_addr, maker_addr, Address::ZERO, token_id_u256,
            maker_amount_u256, taker_amount_u256, expiration_u256, nonce, 
            fee_rate_bps_u256, // Pass the actual fee rate so the signature matches the payload
            side_u8, 0,
        ).await.context("EIP-712 order signing failed")?;

        let url = format!("{}/order", self.rest_url);
        
        use rust_decimal::prelude::ToPrimitive;
        // Strip out any trailing decimals from the scaling operation to prevent HTTP 400s
        let maker_str = maker_amount_scaled.to_u64().unwrap_or(0).to_string();
        let taker_str = taker_amount_scaled.to_u64().unwrap_or(0).to_string();

        let payload = OrderPayload {
            // CRITICAL FIX: Use the specific YES or NO target_token_id instead of the raw market_id pair
            token_id: target_token_id.to_string(),
            maker_amount: maker_str,
            taker_amount: taker_str,
            side: side_str.to_string(),
            fee_rate_bps: fee_rate_bps.to_string(), 
            nonce: nonce_val.to_string(),
            expiration: expiration_u256.to_string(),
            signature,
            signature_type: 0,
            // CRITICAL FIX: 'IOC' permits partial fills. Because the fast-path assumes 100% execution,
            // a 50% partial fill leaves you 50% unhedged without triggering the Unwind Watchdog. 
            // 'FOK' (Fill Or Kill) guarantees binary success/failure.
            order_type: "FOK".to_string(),
        };

        let resp = self.http
            .post(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .json(&CreateOrderRequest { order: payload })
            .send()
            .await
            .context("Polymarket order submission failed")?;

        let status_code = resp.status();
        let body: OrderResponse = resp.json().await.unwrap_or(OrderResponse {
            success: false,
            order_id: None,
            error_msg: Some(format!("HTTP {}", status_code)),
        });

        if body.success {
            let order_id_str = body.order_id.unwrap_or_default();
            let mut fill_size = taker_amount_scaled / scale;
            let mut actual_price = price;
            let mut matched_confirmed = false;
            
            if !order_id_str.is_empty() {
                let fetch_url = format!("{}/orders/{}", self.rest_url, order_id_str);
                let mut retries = 0;
                while retries < 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(150 * (retries + 1))).await;
                    
                    let req = self.http.get(&fetch_url)
                        .header("POLY_API_KEY", self.api_key.as_str())
                        .header("POLY_SECRET", self.api_secret.as_str())
                        .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
                        .send();
                        
                    // MED-6 FIX: Apply a strict timeout to the confirmation loop to prevent blocking the executor
                    if let Ok(Ok(fetch_resp)) = tokio::time::timeout(std::time::Duration::from_secs(3), req).await {
                        if let Ok(order_data) = fetch_resp.json::<serde_json::Value>().await {
                            let mut updated = false;
                            if let Some(avg_price_str) = order_data.get("average_price").and_then(|v| v.as_str()) {
                                if let Ok(parsed_price) = Decimal::from_str(avg_price_str) {
                                    actual_price = parsed_price;
                                    updated = true;
                                }
                            }
                            if let Some(size_matched_str) = order_data.get("size_matched").and_then(|v| v.as_str()) {
                                if let Ok(parsed_size) = Decimal::from_str(size_matched_str) {
                                    fill_size = parsed_size;
                                    matched_confirmed = true;
                                    updated = true;
                                }
                            }
                            if updated { break; }
                        }
                    }
                    retries += 1;
                }
                
                if !matched_confirmed {
                    // CRIT-4 FIX: Assume order failed to avoid unhedged exposure on phantom fills
                    fill_size = Decimal::ZERO;
                    tracing::error!("CRITICAL: Polymarket order {} success but size_matched unconfirmed. Treating as unfilled.", order_id_str);
                }
            }
            
            let filled = fill_size > Decimal::ZERO;
            let estimated_fee = crate::feeds::normalizer::polymarket_fee(actual_price, fill_size, fee_rate_bps as u16);

            Ok(OrderResult {
                filled,
                fill_price: actual_price,
                fill_size,
                fee: estimated_fee,
                order_id: order_id_str,
                error: if !filled { Some("FOK order returned zero fill size".into()) } else { None },
            })
        } else {
            Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: body.error_msg.or_else(|| Some(format!("HTTP {}", status_code))),
            })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/order/{}", self.rest_url, order_id);
        let resp = self.http.delete(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .send()
            .await
            .context("Polymarket cancel failed")?;
        if !resp.status().is_success() {
            tracing::warn!(order_id, status = %resp.status(), "Polymarket cancel returned non-2xx");
        }
    // FOK orders fill entirely or not at all — cancel is best-effort for edge cases
        Ok(())
    }
}

impl PolymarketClient {
    /// Queries the Polymarket API for the actual live balance of the wallet
    pub async fn get_balance(&self) -> Result<Decimal> {
        let url = format!("{}/balance", self.rest_url);
        let resp = self.http.get(&url)
            .header("POLY_API_KEY", self.api_key.as_str())
            .header("POLY_SECRET", self.api_secret.as_str())
            .header("POLY_PASSPHRASE", self.api_passphrase.as_str())
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Polymarket get_balance failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let balance_str = body.get("usdcBalance").and_then(|v| v.as_str()).unwrap_or("0");
        Ok(Decimal::from_str(balance_str).unwrap_or(Decimal::ZERO))
    }
}
```

## File: src/execution/kalshi_client.rs
```rust
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::executor::{OrderResult, PlatformOrderClient};
use crate::crypto::jwt::KalshiAuth;
use crate::types::Side;

use std::sync::Arc;

#[derive(Clone)]
pub struct KalshiClient {
    http: reqwest::Client,
    rest_url: String,
    auth: Arc<KalshiAuth>,
}

#[derive(Serialize)]
struct KalshiOrderRequest {
    ticker: String,
    action: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    count: i64,
    yes_price: Option<i64>,
    no_price: Option<i64>,
    expiration_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<String>,
}

#[derive(Deserialize)]
struct KalshiOrderResponse {
    #[serde(default)]
    order: Option<KalshiOrder>,
    #[serde(default)]
    error: Option<KalshiError>,
}

#[derive(Deserialize)]
struct KalshiOrder {
    order_id: String,
    #[serde(default)]
    yes_price: i64,
    #[serde(default)]
    no_price: i64,
    #[serde(default)]
    count: i64,
    #[serde(default)]
    remaining_count: i64,
}

#[derive(Deserialize)]
struct KalshiError {
    message: String,
}

impl KalshiClient {
    pub fn new(rest_url: String, auth: KalshiAuth) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5));
            
        // M-8 FIX: Explicit TLS Cert Pinning
        if let Ok(cert_pem) = std::fs::read("/opt/mercury/keys/pinned_certs.pem") {
            if let Ok(cert) = reqwest::tls::Certificate::from_pem(&cert_pem) {
                builder = builder.add_root_certificate(cert);
            }
        }
            
        let http = builder.build().expect("failed to build Kalshi HTTP client");
        Self { http, rest_url, auth: Arc::new(auth) }
    }
}

use crate::execution::executor::OrderAction;

#[async_trait::async_trait]
impl PlatformOrderClient for KalshiClient {
    async fn submit_order(&self, market_id: &str, action: OrderAction, side: Side, price: Decimal, size: Decimal, _fee_rate_bps: u32) -> Result<OrderResult> {
        // Round to nearest cent before converting — avoids silent truncation (e.g. 50.5¢ → 50¢).

        // Avoid string parsing panics by directly safely converting rounded decimals
        let mut price_cents = (price * Decimal::from(100)).round().to_i64().unwrap_or(50);
        
        // CRITICAL FIX: Kalshi explicitly rejects prices of 0 or 100.
        price_cents = price_cents.clamp(1, 99);
        
        // CRIT-1 / MED-10 FIX: Prevent catastrophic fallback to i64::MAX on extreme size overflows
        let count = size.floor().to_i64().unwrap_or(0).clamp(1, 10_000);

        // MED-5: Basic rate limiting to prevent 429s on burst arbs
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (kalshi_side, yes_price, no_price) = match side {
            Side::Yes => ("yes".to_string(), Some(price_cents), None),
            Side::No => ("no".to_string(), None, Some(price_cents)),
        };

        let kalshi_action = match action {
            OrderAction::Buy => "buy".to_string(),
            OrderAction::Sell => "sell".to_string(),
        };

        info!(ticker = market_id, action = %kalshi_action, side = %kalshi_side, price_cents, count, "Submitting Kalshi order");

        let url = format!("{}/portfolio/orders", self.rest_url);
        let auth_header = self.auth.auth_header().await?;

        let req = KalshiOrderRequest {
            ticker: market_id.to_string(),
            action: kalshi_action,
            side: kalshi_side,
            order_type: "limit".to_string(),
            count,
            yes_price,
            no_price,
            expiration_ts: None,
            client_order_id: None,
            // CRITICAL FIX: Ensure the order is Fill-or-Kill so it doesn't rest on the book
            time_in_force: Some("fok".to_string()), 
        };

        let resp = self.http
            .post(&url)
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .context("Kalshi order submission failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!("Kalshi rate limited: {}", body);
            }
            return Ok(OrderResult {
                filled: false,
                fill_price: Decimal::ZERO,
                fill_size: Decimal::ZERO,
                fee: Decimal::ZERO,
                order_id: String::new(),
                error: Some(format!("HTTP {}: {}", status, body)),
            });
        }

        let body: KalshiOrderResponse = resp.json().await.unwrap_or(KalshiOrderResponse {
            order: None,
            error: Some(KalshiError { message: "Failed to decode response".to_string() }),
        });

        if let Some(order) = body.order {
            let filled_count = order.count - order.remaining_count;
            let filled = filled_count > 0;
            let fill_price = Decimal::from(order.yes_price.max(order.no_price)) / Decimal::from(100);
            // CRIT-5 FIX: Centralize fee math to ensure consistency with spread engine
            let total_fee = crate::feeds::normalizer::kalshi_fee(fill_price, Decimal::from(filled_count));
            Ok(OrderResult {
                filled,
                fill_price,
                fill_size: Decimal::from(filled_count),
                fee: total_fee,
                order_id: order.order_id,
                error: if !filled { Some("Order not filled".into()) } else { None },
            })
        } else {
            let error_msg = body.error.map(|e| e.message).unwrap_or_else(|| "Unknown error".into());
            Ok(OrderResult { filled: false, fill_price: Decimal::ZERO, fill_size: Decimal::ZERO, fee: Decimal::ZERO, order_id: String::new(), error: Some(error_msg) })
        }
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/portfolio/orders/{}", self.rest_url, order_id);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.delete(&url)
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("Kalshi cancel failed")?;
        let status = resp.status();
        if !status.is_success() {
            tracing::warn!("Kalshi cancel_order failed: HTTP {}", status);
        }
        Ok(())
    }
}

impl KalshiClient {
    /// Queries the Kalshi API for the actual revenue generated by a settled market position.
    pub async fn fetch_settlement_payout(&self, ticker: &str, quantity: Decimal, avg_entry: Decimal) -> Result<Decimal> {
        let url = format!("{}/portfolio/settlements?ticker={}", self.rest_url, ticker);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("Kalshi settlement fetch failed")?;
            
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        
        let mut total_revenue_cents: i64 = 0;
        let mut total_contracts: i64 = 0;
        
        // FIX: Compute the average payout per contract to prevent multi-arb double counting
        if let Some(settlements) = body.get("settlements").and_then(|v| v.as_array()) {
            for settlement in settlements {
                total_revenue_cents += settlement.get("revenue").and_then(|v| v.as_i64()).unwrap_or(0);
                total_contracts += settlement.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }
        
        let revenue_per_contract = if total_contracts > 0 {
            Decimal::from(total_revenue_cents) / Decimal::from(total_contracts) / Decimal::from(100)
        } else {
            Decimal::ZERO
        };
        
        let total_revenue = quantity * revenue_per_contract;
        let cost_basis = quantity * avg_entry;
        let realized_pnl = total_revenue - cost_basis;
        
        Ok(realized_pnl)
    }

    /// Queries the Kalshi API for the actual live balance of the wallet
    pub async fn get_balance(&self) -> Result<Decimal> {
        let url = format!("{}/portfolio/balance", self.rest_url);
        let auth_header = self.auth.auth_header().await?;
        let resp = self.http.get(&url)
            .header("Authorization", &auth_header)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Kalshi get_balance failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let balance_cents = body.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(Decimal::from(balance_cents) / Decimal::from(100))
    }
}
```

## File: src/main.rs
```rust
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
    let (tick_tx, _) = broadcast::channel::<NormalizedTick>(50_000); // MED-1 FIX: Increased to 50k to survive lag spikes
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
            .unwrap_or_else(|_| env_vars.get("POLYMARKET_CHAIN_ID").cloned().unwrap_or_else(|| "137".to_string()))
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
                    cached_open_positions = count + in_flight_trades;
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
```

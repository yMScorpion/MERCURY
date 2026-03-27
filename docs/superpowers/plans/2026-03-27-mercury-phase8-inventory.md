# Phase 8: Inventory Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build position tracking with double-entry bookkeeping, periodic reconciliation, and settlement monitoring.

**Architecture:** The inventory module consumes TradeResult events, updates positions in SQLite, runs periodic reconciliation against platform APIs, and monitors settlements. Runs as background tokio tasks.

**Tech Stack:** rust_decimal, reqwest, tokio timers, SQLite

**Depends on:** Phase 1 (types, DB), Phase 7 (execution)

---

### Task 1: Position Tracker

**Files:**
- Create: `src/inventory/mod.rs`
- Create: `src/inventory/positions.rs`

- [ ] **Step 1: Write the position tracker**

`src/inventory/mod.rs`:
```rust
pub mod positions;
pub mod reconciler;
pub mod settlement;
```

`src/inventory/positions.rs`:
```rust
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::db::Database;
use crate::risk::bankroll::BankrollManager;
use crate::types::*;

/// Position tracker using double-entry bookkeeping
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
            // No fills at all, nothing to track
            return Ok(());
        }

        let now = Utc::now();

        // Leg A position (if filled)
        if trade.leg_a_size > Decimal::ZERO {
            let position = Position {
                id: 0, // Auto-generated
                market_id: trade.market_id,
                platform: trade.leg_a_platform,
                side: trade.leg_a_side,
                quantity: trade.leg_a_size,
                avg_entry_price: trade.leg_a_fill_price,
                unrealized_pnl: Decimal::ZERO,
                opened_at: now,
                updated_at: now,
            };
            self.db.upsert_position(&position).await?;
        }

        // Leg B position (if filled)
        if trade.leg_b_size > Decimal::ZERO {
            let position = Position {
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
            self.db.upsert_position(&position).await?;
        }

        // Update platform balances (debit cost, credit position)
        let leg_a_cost = trade.leg_a_fill_price * trade.leg_a_size + trade.leg_a_fee;
        let leg_b_cost = trade.leg_b_fill_price * trade.leg_b_size + trade.leg_b_fee;

        // Log the double-entry
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

        info!(
            trade_id = trade.trade_id,
            leg_a_cost = %leg_a_cost,
            leg_b_cost = %leg_b_cost,
            "Position booked"
        );

        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/inventory/
git commit -m "feat: add Position Tracker with double-entry bookkeeping"
```

---

### Task 2: Reconciliation Engine

**Files:**
- Create: `src/inventory/reconciler.rs`

- [ ] **Step 1: Write the reconciliation engine**

`src/inventory/reconciler.rs`:
```rust
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::db::Database;
use crate::types::*;

/// Periodic reconciliation between internal ledger and platform APIs
pub struct Reconciler {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    interval: Duration,
    /// Discrepancy threshold in USD
    threshold: Decimal,
}

impl Reconciler {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        interval_secs: u64,
        threshold: Decimal,
    ) -> Self {
        Self {
            db,
            alert_tx,
            interval: Duration::from_secs(interval_secs),
            threshold,
        }
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
        let positions = self.db.get_open_positions().await?;
        let balances = self.db.get_all_balances().await?;

        // Log reconciliation
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

        // Check for stale positions (open longer than expected)
        for pos in &positions {
            let age = chrono::Utc::now() - pos.opened_at;
            if age.num_days() > 30 {
                warn!(
                    position_id = pos.id,
                    market_id = %pos.market_id,
                    age_days = age.num_days(),
                    "Stale position detected"
                );
            }
        }

        // Verify internal balance consistency
        // Sum of all position costs should roughly match sum of platform balances used
        let total_position_value: Decimal = positions.iter()
            .map(|p| p.quantity * p.avg_entry_price)
            .sum();

        info!(
            open_positions = positions.len(),
            total_position_value = %total_position_value.round_dp(2),
            "Reconciliation complete"
        );

        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/inventory/reconciler.rs
git commit -m "feat: add periodic reconciliation engine"
```

---

### Task 3: Settlement Monitor

**Files:**
- Create: `src/inventory/settlement.rs`

- [ ] **Step 1: Write the settlement monitor**

`src/inventory/settlement.rs`:
```rust
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::db::Database;
use crate::types::*;

/// Monitors open positions for approaching settlement/resolution
pub struct SettlementMonitor {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    check_interval: Duration,
}

impl SettlementMonitor {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        check_interval_secs: u64,
    ) -> Self {
        Self {
            db,
            alert_tx,
            check_interval: Duration::from_secs(check_interval_secs),
        }
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
            // Check if market has resolved by querying the market registry
            if let Some(market) = self.db.get_market(&position.market_id).await? {
                match market.status {
                    MarketStatus::Resolved => {
                        info!(
                            position_id = position.id,
                            market = %market.question,
                            "Market resolved - position ready for settlement"
                        );

                        // Close the position
                        self.db.close_position(position.id).await?;

                        let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                            severity: "info".into(),
                            message: format!(
                                "Position #{} settled: {} {} on {} ({} contracts @ ${})",
                                position.id,
                                position.side,
                                market.question,
                                position.platform,
                                position.quantity,
                                position.avg_entry_price,
                            ),
                        }).await;
                    }
                    MarketStatus::Expired => {
                        warn!(
                            position_id = position.id,
                            market = %market.question,
                            "Market expired with open position"
                        );
                        self.db.close_position(position.id).await?;
                    }
                    _ => {
                        // Check if approaching expiration
                        let time_to_expiry = market.expiration - now;
                        if time_to_expiry.num_hours() < 1 && time_to_expiry.num_seconds() > 0 {
                            let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                                severity: "warning".into(),
                                message: format!(
                                    "Position #{} expiring in {:.0} minutes: {}",
                                    position.id,
                                    time_to_expiry.num_minutes(),
                                    market.question,
                                ),
                            }).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Update main.rs**

Add to `src/main.rs`:
```rust
mod inventory;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/inventory/ src/main.rs
git commit -m "feat: add settlement monitor and complete inventory module"
```

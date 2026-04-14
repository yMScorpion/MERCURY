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
                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                    severity: "critical".into(),
                    message: format!(
                        "🚨 STALE POSITION: Position #{} on {} has been open for {} days. Manual resolution required. Market ID: {}",
                        pos.id, pos.platform, age.num_days(), pos.market_id
                    ),
                });
            }
        }

        // Active API Reconciliation (CRITICAL FIX 3-C)
        if let Some(kalshi) = &self.kalshi_client {
            if let Ok(live) = kalshi.get_balance().await {
                let db_bal = balances.iter().find(|b| b.platform == Platform::Kalshi).map(|b| b.total).unwrap_or(Decimal::ZERO);
                if (live - db_bal).abs() > self.threshold {
                    warn!(live = %live, db = %db_bal, "Kalshi balance mismatch");
                    let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("Kalshi Balance Mismatch: API=${live}, DB=${db_bal}"),
                    });
                }
                // Always persist the live balance to DB so reports are accurate
                let _ = self.db.update_balance(&crate::types::PlatformBalance {
                    platform: Platform::Kalshi,
                    available: live,
                    reserved: Decimal::ZERO,
                    pending_settlement: Decimal::ZERO,
                    total: live,
                    updated_at: chrono::Utc::now(),
                }).await;
            }
        }

        if let Some(poly) = &self.polymarket_client {
            if let Ok(live) = poly.get_balance().await {
                let db_bal = balances.iter().find(|b| b.platform == Platform::Polymarket).map(|b| b.total).unwrap_or(Decimal::ZERO);
                if (live - db_bal).abs() > self.threshold {
                    warn!(live = %live, db = %db_bal, "Polymarket balance mismatch");
                    let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("Polymarket Balance Mismatch: API=${live}, DB=${db_bal}"),
                    });
                }
                // Always persist the live balance to DB so reports are accurate
                let _ = self.db.update_balance(&crate::types::PlatformBalance {
                    platform: Platform::Polymarket,
                    available: live,
                    reserved: Decimal::ZERO,
                    pending_settlement: Decimal::ZERO,
                    total: live,
                    updated_at: chrono::Utc::now(),
                }).await;
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
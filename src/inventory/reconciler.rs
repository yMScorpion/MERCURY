use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::db::Database;
use crate::types::*;

pub struct Reconciler {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    interval: Duration,
    threshold: Decimal,
}

impl Reconciler {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        interval_secs: u64,
        threshold: Decimal,
    ) -> Self {
        Self { db, alert_tx, interval: Duration::from_secs(interval_secs), threshold }
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
            }
        }

        let total_position_value: Decimal = positions.iter()
            .map(|p| p.quantity * p.avg_entry_price)
            .sum();

        info!(open_positions = positions.len(), total_position_value = %total_position_value.round_dp(2), "Reconciliation complete");
        Ok(())
    }
}

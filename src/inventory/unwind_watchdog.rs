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

pub struct UnwindWatchdog {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    check_interval: Duration,
}

impl UnwindWatchdog {
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

                    let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                        severity: "critical".into(),
                        message: format!(
                            "🚨 ORPHANED POSITION IMBALANCE: {} contracts unhedged \
                             open for {} minutes. MANUAL CLOSE REQUIRED. \
                             Market ID: {}",
                            unhedged_diff,
                            age.num_minutes(),
                            market_id,
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}
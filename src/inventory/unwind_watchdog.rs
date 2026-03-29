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
use tracing::{error, info, warn};
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
            // A healthy arb has exactly 2 positions (YES on one platform, NO on another).
            // A single leg means the hedge failed — this is an orphan.
            if legs.len() == 1 {
                let orphan = &legs[0];
                let age = chrono::Utc::now() - orphan.opened_at;

                if age.num_minutes() > 5 {
                    error!(
                        market_id = %market_id,
                        platform = %orphan.platform,
                        side = %orphan.side,
                        quantity = %orphan.quantity,
                        age_minutes = age.num_minutes(),
                        "ORPHANED POSITION DETECTED — unhedged for {} minutes",
                        age.num_minutes()
                    );

                    let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                        severity: "critical".into(),
                        message: format!(
                            "🚨 ORPHANED POSITION: {} {} on {} ({} contracts @ ${}) \
                             open for {} minutes with no hedge. MANUAL CLOSE REQUIRED. \
                             Market ID: {}",
                            orphan.side,
                            orphan.platform,
                            orphan.platform,
                            orphan.quantity,
                            orphan.avg_entry_price,
                            age.num_minutes(),
                            market_id,
                        ),
                    });
                }
            }

            // Also flag positions where both legs exist but quantities are mismatched.
            if legs.len() == 2 {
                let diff = (legs[0].quantity - legs[1].quantity).abs();
                if diff > Decimal::ZERO {
                    warn!(
                        market_id = %market_id,
                        leg_a_qty = %legs[0].quantity,
                        leg_b_qty = %legs[1].quantity,
                        "Asymmetric position pair — {} contracts unhedged",
                        diff
                    );
                }
            }
        }

        Ok(())
    }
}
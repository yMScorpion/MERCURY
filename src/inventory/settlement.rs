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
    check_interval: Duration,
}

impl SettlementMonitor {
    pub fn new(db: Arc<dyn Database>, alert_tx: mpsc::Sender<AlertMessage>, check_interval_secs: u64) -> Self {
        Self { db, alert_tx, check_interval: Duration::from_secs(check_interval_secs) }
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
                            }),
                        };
                        if let Err(e) = self.db.append_audit(&audit).await {
                            warn!(error = %e, position_id = position.id, "Failed to write settlement audit entry");
                        }
                        self.db.close_position(position.id).await?;
                        let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                            severity: "info".into(),
                            message: format!(
                                "Position #{} settled: {} {} on {} ({} contracts @ ${})",
                                position.id, position.side, market.question,
                                position.platform, position.quantity, position.avg_entry_price,
                            ),
                        }).await;
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
                            let _ = self.alert_tx.send(AlertMessage::SystemAlert {
                                severity: "warning".into(),
                                message: format!(
                                    "Position #{} expiring in {:.0} minutes: {}",
                                    position.id, time_to_expiry.num_minutes(), market.question,
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

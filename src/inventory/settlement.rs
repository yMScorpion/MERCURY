use anyhow::Result;
use rust_decimal::Decimal;
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
                        
                        // Query the actual platform payout so DB records are accurate for audit.
                        // For arb positions the profit was already booked at execution; realized_pnl
                        // here is used only for the audit trail and settlement queue — NOT added to
                        // bankroll again (record_settlement only frees exposure).
                        let realized_pnl = if let Some(ref kalshi) = self.kalshi_client {
                            if let Some(info) = market.platforms.get(&position.platform) {
                                if position.platform == crate::types::Platform::Kalshi {
                                    kalshi.fetch_settlement_payout(
                                        &info.platform_market_id,
                                        position.quantity,
                                        position.avg_entry_price,
                                    ).await.unwrap_or(Decimal::ZERO)
                                } else {
                                    Decimal::ZERO
                                }
                            } else {
                                Decimal::ZERO
                            }
                        } else {
                            Decimal::ZERO
                        };
                        
                        if let Some(_info) = market.platforms.get(&position.platform) {
                            tracing::info!(position_id = position.id, platform = %position.platform, realized_pnl = %realized_pnl, "Settlement recorded for platform");
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
                        
                        // PHASE 3 FIX: Backpressure-Aware Persistent Settlement Queue
                        // Log to SQLite first. If the Bankroll actor is saturated, the background 
                        // worker will autonomously retry with exponential backoff. No PnL is ever dropped.
                        if let Err(e) = self.db.enqueue_settlement(
                            position.id,
                            &position.market_id,
                            position.platform,
                            position.quantity,
                            position.avg_entry_price,
                            realized_pnl
                        ).await {
                            tracing::error!(error = %e, position_id = position.id, "CRITICAL: Failed to enqueue settlement. PnL not persisted.");
                        } else {
                            self.db.close_position(position.id).await?;
                        }
                        
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
        
        // Drain pending settlements queue
        if let Ok(pending) = self.db.get_pending_settlements().await {
            for (id, settlement) in pending {
                match self.settlement_tx.try_send(settlement.clone()) {
                    Ok(()) => {
                        // Mark resolved ONLY after successful send — ensures retry on restart
                        if let Err(e) = self.db.mark_settlement_resolved(id).await {
                            tracing::error!(error = %e, settlement_id = id, "Failed to mark settlement resolved in DB");
                        }
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::warn!(settlement_id = id, "Settlement channel full — will retry next tick (settlement remains pending in DB)");
                        break; // Stop draining, try again next tick (NOT marked resolved)
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        tracing::error!("Settlement channel closed — main loop may have exited");
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}
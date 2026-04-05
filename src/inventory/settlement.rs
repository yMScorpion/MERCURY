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
                        
                        let mut realized_pnl = -(position.avg_entry_price * position.quantity); // Assume total loss by default
                        
                        if let Some(info) = market.platforms.get(&position.platform) {
                            if position.platform == Platform::Kalshi {
                                if let Some(client) = &self.kalshi_client {
                                    // 150ms grace period: Kalshi settlement webhooks are asynchronous.
                                    // The market resolution event arrives before the settlement record is
                                    // written to their API. Without this delay, fetch_settlement_payout
                                    // returns zero contracts and we book a $0 PnL instead of the real win.
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    if let Ok(pnl) = client.fetch_settlement_payout(&info.platform_market_id, position.quantity, position.avg_entry_price).await {
                                        realized_pnl = pnl;
                                    }
                                }
                            } else if position.platform == Platform::Polymarket || position.platform == Platform::PolymarketUs || position.platform == Platform::Cdna || position.platform == Platform::ForecastEx {
                                tracing::warn!(position_id = position.id, platform = %position.platform, "Settlement for this platform requires manual verification");
                                // Credit a conservative zero-PnL settlement so the locked exposure is freed.
                                // The actual PnL (win/loss) must be manually adjusted by the operator.
                                // Without this, exposure is permanently locked and the bankroll is understated.
                                realized_pnl = Decimal::ZERO;
                                tracing::error!(
                                    position_id = position.id,
                                    platform = %position.platform,
                                    quantity = %position.quantity,
                                    avg_entry = %position.avg_entry_price,
                                    "SETTLEMENT PLACEHOLDER: Bankroll PnL will be incorrect until manually adjusted. \
                                     Expected settlement: +${} (win) or -${} (loss)",
                                    position.quantity,
                                    position.quantity * position.avg_entry_price
                                );
                                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                    severity: "critical".into(),
                                    message: format!(
                                        "Position #{} on {} resolved. Exposure freed with $0 PnL placeholder. \
                                         MANUAL PnL ADJUSTMENT REQUIRED — check if position won ($1/contract) or lost ($0).\n\
                                         Expected: +${:.2} (win) or -${:.2} (loss)",
                                        position.id, position.platform,
                                        position.quantity,
                                        position.quantity * position.avg_entry_price,
                                    ),
                                });
                                // Don't continue — fall through to the settlement_tx send and close_position below
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
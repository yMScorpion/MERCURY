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

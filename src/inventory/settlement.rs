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
    settlement_tx: mpsc::Sender<Decimal>,
    check_interval: Duration,
    kalshi_client: Option<crate::execution::kalshi_client::KalshiClient>,
}

impl SettlementMonitor {
    pub fn new(
        db: Arc<dyn Database>, 
        alert_tx: mpsc::Sender<AlertMessage>, 
        settlement_tx: mpsc::Sender<Decimal>,
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
                                    if let Ok(pnl) = client.fetch_settlement_payout(&info.platform_market_id, position.quantity, position.avg_entry_price).await {
                                        realized_pnl = pnl;
                                    }
                                }
                            }
                            // Note: Polymarket settlements are processed on-chain via USDC redemption.
                            // Assuming total loss here until the Web3 provider tracks the specific ERC1155 burn event.
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
                        
                        let _ = self.settlement_tx.try_send(realized_pnl);
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

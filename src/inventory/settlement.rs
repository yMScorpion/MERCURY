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
                                let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                    severity: "critical".into(),
                                    message: format!(
                                        "Position #{} on {} resolved. Exposure freed with $0 PnL placeholder. \
                                         MANUAL PnL ADJUSTMENT REQUIRED — check if position won ($1/contract) or lost ($0).",
                                        position.id, position.platform
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
                        
                        // CRIT-2 FIX: Use blocking send with timeout instead of try_send.
                        // If the channel is full, we MUST NOT silently drop settlement PnL — 
                        // that permanently corrupts the bankroll. Retry with backoff.
                        let settlement_result = SettlementResult {
                            realized_pnl,
                            platform: position.platform,
                            market_id: position.market_id,
                            quantity: position.quantity,
                            avg_entry_price: position.avg_entry_price,
                        };
                        
                        let mut send_attempts = 0;
                        loop {
                            match self.settlement_tx.try_send(settlement_result.clone()) {
                                Ok(()) => break,
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    send_attempts += 1;
                                    if send_attempts >= 10 {
                                        tracing::error!(
                                            position_id = position.id,
                                            realized_pnl = %realized_pnl,
                                            "CRITICAL: Settlement channel full after 10 retries. \
                                             PnL ${} NOT credited to bankroll. MANUAL INTERVENTION REQUIRED.",
                                            realized_pnl
                                        );
                                        let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                            severity: "critical".into(),
                                            message: format!(
                                                "🚨 SETTLEMENT PnL LOST: Position #{} PnL ${} could not be \
                                                 delivered to bankroll manager. Channel saturated. \
                                                 MANUAL BANKROLL ADJUSTMENT REQUIRED.",
                                                position.id, realized_pnl
                                            ),
                                        });
                                        break;
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(100 * send_attempts)).await;
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    tracing::error!("Settlement channel closed — engine shutting down");
                                    break;
                                }
                            }
                        }
                        
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
use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::db::Database;
use crate::types::*;

pub struct PositionTracker {
    db: Arc<dyn Database>,
    rx: mpsc::Receiver<TradeResult>,
}

impl PositionTracker {
    pub fn new(db: Arc<dyn Database>, rx: mpsc::Receiver<TradeResult>) -> Self {
        Self { db, rx }
    }

    pub async fn run(mut self) {
        info!("Position tracker started");
        while let Some(trade) = self.rx.recv().await {
            if let Err(e) = self.process_trade(&trade).await {
                error!(error = %e, "Failed to process trade for position tracking");
            }
        }
        info!("Position tracker stopped");
    }

    async fn process_trade(&self, trade: &TradeResult) -> Result<()> {
        if trade.status == TradeStatus::Fail && trade.leg_a_size == Decimal::ZERO {
            return Ok(());
        }

        let now = Utc::now();

        if trade.leg_a_size > Decimal::ZERO {
            let position = Position {
                id: 0,
                market_id: trade.market_id,
                platform: trade.leg_a_platform,
                side: trade.leg_a_side,
                quantity: trade.leg_a_size,
                avg_entry_price: trade.leg_a_fill_price,
                unrealized_pnl: Decimal::ZERO,
                opened_at: now,
                updated_at: now,
            };
            self.db.upsert_position(&position).await?;
        }

        if trade.leg_b_size > Decimal::ZERO {
            let position = Position {
                id: 0,
                market_id: trade.market_id,
                platform: trade.leg_b_platform,
                side: trade.leg_b_side,
                quantity: trade.leg_b_size,
                avg_entry_price: trade.leg_b_fill_price,
                unrealized_pnl: Decimal::ZERO,
                opened_at: now,
                updated_at: now,
            };
            self.db.upsert_position(&position).await?;
        }

        let leg_a_cost = trade.leg_a_fill_price * trade.leg_a_size + trade.leg_a_fee;
        let leg_b_cost = trade.leg_b_fill_price * trade.leg_b_size + trade.leg_b_fee;

        let audit = AuditEntry {
            timestamp_ns: now_ns(),
            module: "inventory".into(),
            event_type: "trade_booked".into(),
            data: serde_json::json!({
                "trade_id": trade.trade_id,
                "leg_a_debit": leg_a_cost.to_string(),
                "leg_b_debit": leg_b_cost.to_string(),
                "total_debit": (leg_a_cost + leg_b_cost).to_string(),
                "profit": trade.profit.to_string(),
            }),
        };
        self.db.append_audit(&audit).await?;

        info!(trade_id = trade.trade_id, leg_a_cost = %leg_a_cost, leg_b_cost = %leg_b_cost, "Position booked");
        Ok(())
    }
}

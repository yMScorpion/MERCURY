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
use crate::execution::executor::{PlatformOrderClient, OrderAction};
use crate::execution::polymarket_client::PolymarketClient;
use crate::execution::kalshi_client::KalshiClient;
use crate::execution::cdna_client::CdnaClient;
use crate::execution::forecastex_client::ForecastExClient;
use crate::engine::order_book::UnifiedOrderBook;
use std::sync::RwLock;

pub struct UnwindWatchdog {
    db: Arc<dyn Database>,
    alert_tx: mpsc::Sender<AlertMessage>,
    check_interval: Duration,
    polymarket: Option<PolymarketClient>,
    kalshi: Option<KalshiClient>,
    cdna: Option<CdnaClient>,
    forecastex: Option<ForecastExClient>,
    uob: Option<Arc<RwLock<UnifiedOrderBook>>>,
}

impl UnwindWatchdog {
    pub fn new(
        db: Arc<dyn Database>,
        alert_tx: mpsc::Sender<AlertMessage>,
        check_interval_secs: u64,
        polymarket: Option<PolymarketClient>,
        kalshi: Option<KalshiClient>,
        cdna: Option<CdnaClient>,
        forecastex: Option<ForecastExClient>,
        uob: Option<Arc<RwLock<UnifiedOrderBook>>>,
    ) -> Self {
        Self {
            db,
            alert_tx,
            check_interval: Duration::from_secs(check_interval_secs),
            polymarket,
            kalshi,
            cdna,
            forecastex,
            uob,
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
                let oldest = legs.iter().map(|p| p.opened_at).min().unwrap_or_else(chrono::Utc::now);
                let age = chrono::Utc::now() - oldest;
                
                // 1.2 EXECUTION SAFETY: Stop-Loss via UOB
                let mut loss_exceeds_threshold = false;
                if let Some(uob_ref) = &self.uob {
                    if let Ok(uob) = uob_ref.read() {
                        if let Some(market_book) = uob.get_market(market_id) {
                            let mut unrealized_pnl = Decimal::ZERO;
                            let mut cost_basis = Decimal::ZERO;
                            for pos in legs {
                                if let Some(pb) = market_book.platforms.get(&pos.platform) {
                                    let current_price = pb.mid_price();
                                    cost_basis += pos.quantity * pos.avg_entry_price;
                                    unrealized_pnl += pos.quantity * (current_price - pos.avg_entry_price);
                                }
                            }
                            if cost_basis > Decimal::ZERO && (unrealized_pnl < Decimal::ZERO) && (unrealized_pnl.abs() / cost_basis) > rust_decimal_macros::dec!(0.05) {
                                loss_exceeds_threshold = true;
                            }
                        }
                    }
                }

                if age.num_minutes() > 5 || loss_exceeds_threshold {
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
                             open for {} minutes. Attempting automated liquidation. \
                             Market ID: {}",
                            unhedged_diff,
                            age.num_minutes(),
                            market_id,
                        ),
                    });

                    // L-3 FIX: Automated liquidation attempt
                    if let Ok(Some(market)) = self.db.get_market(market_id).await {
                        let (target_platform, target_side) = if total_yes > total_no {
                            (legs.iter().find(|p| p.side == Side::Yes).map(|p| p.platform), Side::Yes)
                        } else {
                            (legs.iter().find(|p| p.side == Side::No).map(|p| p.platform), Side::No)
                        };

                        if let Some(plat) = target_platform {
                            if let Some(info) = market.platforms.get(&plat) {
                                let result = match plat {
                                    Platform::Polymarket | Platform::PolymarketUs => {
                                        if let Some(c) = &self.polymarket {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, Usd(rust_decimal_macros::dec!(0.01)), Contracts(unhedged_diff), BasisPoints(info.fee_rate_bps as u32)).await)
                                        } else { None }
                                    },
                                    Platform::Kalshi => {
                                        if let Some(c) = &self.kalshi {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, Usd(rust_decimal_macros::dec!(0.01)), Contracts(unhedged_diff), BasisPoints(info.fee_rate_bps as u32)).await)
                                        } else { None }
                                    },
                                    Platform::Cdna => {
                                        if let Some(c) = &self.cdna {
                                            Some(c.submit_order(&info.platform_market_id, OrderAction::Sell, target_side, Usd(rust_decimal_macros::dec!(0.01)), Contracts(unhedged_diff), BasisPoints(info.fee_rate_bps as u32)).await)
                                        } else { None }
                                    },
                                    Platform::ForecastEx => None,
                                };

                                if let Some(Ok(res)) = result {
                                    if res.filled {
                                        let _ = self.alert_tx.try_send(AlertMessage::SystemAlert {
                                            severity: "warning".into(),
                                            message: format!("✅ Automated liquidation successful. Filled {} contracts on {}.", res.fill_size, plat),
                                        });
                                        for pos in legs {
                                            let _ = self.db.close_position(pos.id).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
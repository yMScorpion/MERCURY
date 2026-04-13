use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tracing::info;
use uuid::Uuid;

use crate::types::*;

pub struct BankrollManager {
    total_bankroll: Decimal,
    peak_bankroll: Decimal,
    platform_balances: HashMap<Platform, Decimal>,
    platform_exposure: HashMap<Platform, Decimal>,
    market_exposure: HashMap<Uuid, Decimal>,
    daily_pnl: Decimal,
    daily_start_bankroll: Decimal,
    daily_fees: Decimal,
    day_start: DateTime<Utc>,
    trades_today: i32,
    success_today: i32,
    fail_today: i32,
    exec_success_rate: Decimal,
    exec_history: std::collections::VecDeque<(chrono::DateTime<chrono::Utc>, bool)>,
    exec_success_count: usize,
}

impl BankrollManager {
    pub fn new(initial_bankroll: Decimal) -> Self {
        Self {
            total_bankroll: initial_bankroll,
            peak_bankroll: initial_bankroll,
            platform_balances: HashMap::new(),
            platform_exposure: HashMap::new(),
            market_exposure: HashMap::new(),
            daily_pnl: Decimal::ZERO,
            daily_start_bankroll: initial_bankroll,
            daily_fees: Decimal::ZERO,
            day_start: Utc::now(),
            trades_today: 0,
            success_today: 0,
            fail_today: 0,
            exec_success_rate: dec!(0.90),
            exec_history: std::collections::VecDeque::new(),
            exec_success_count: 0,
        }
    }

    pub fn total_bankroll(&self) -> Decimal { self.total_bankroll }
    pub fn peak_bankroll(&self) -> Decimal { self.peak_bankroll }
    pub fn daily_pnl(&self) -> Decimal { self.daily_pnl }
    pub fn daily_fees(&self) -> Decimal { self.daily_fees }
    pub fn trades_today(&self) -> i32 { self.trades_today }
    pub fn success_today(&self) -> i32 { self.success_today }
    pub fn fail_today(&self) -> i32 { self.fail_today }
    pub fn exec_success_rate(&self) -> Decimal { self.exec_success_rate }

    pub fn drawdown_pct(&self) -> Decimal {
        if self.peak_bankroll > Decimal::ZERO {
            (self.peak_bankroll - self.total_bankroll) / self.peak_bankroll * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    pub fn daily_loss_pct(&self) -> Decimal {
        // Only trigger the loss percentage calculation if PnL is actually negative
        if self.daily_start_bankroll > Decimal::ZERO && self.daily_pnl < Decimal::ZERO {
            (self.daily_pnl.abs() / self.daily_start_bankroll) * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    pub fn platform_exposure(&self, platform: &Platform) -> Decimal {
        self.platform_exposure.get(platform).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn platform_exposure_pct(&self, platform: &Platform) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.platform_exposure(platform) / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn total_exposure(&self) -> Decimal {
        self.platform_exposure.values().sum()
    }

    pub fn total_exposure_pct(&self) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.total_exposure() / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn market_exposure(&self, market_id: &Uuid) -> Decimal {
        self.market_exposure.get(market_id).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn market_exposure_pct(&self, market_id: &Uuid) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.market_exposure(market_id) / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    pub fn add_market_exposure(&mut self, market_id: Uuid, amount: Decimal) {
        *self.market_exposure.entry(market_id).or_insert(Decimal::ZERO) += amount;
    }

    pub fn remove_market_exposure(&mut self, market_id: Uuid, amount: Decimal) {
        if let Some(exp) = self.market_exposure.get_mut(&market_id) {
            *exp = (*exp - amount).max(Decimal::ZERO);
            if *exp == Decimal::ZERO {
                self.market_exposure.remove(&market_id);
            }
        }
    }

    /// Credits the bankroll with realized PnL from an expired/settled market.
    /// Winning legs pay $1.00 per contract; losing legs pay $0.00. 
    pub fn record_settlement(&mut self, settlement: &crate::types::SettlementResult) {
        // CRITICAL FIX: Do NOT add realized_pnl to total_bankroll here.
        // The arbitrage profit was already added to total_bankroll in `record_trade`.
        // Adding settlement PnL double-counts (or incorrectly subtracts) the legs.
        
        let exposure_freed = settlement.quantity * settlement.avg_entry_price;
        self.remove_exposure(settlement.platform, exposure_freed);
        self.remove_market_exposure(settlement.market_id, exposure_freed);
        
        tracing::info!(realized_pnl = %settlement.realized_pnl, exposure_freed = %exposure_freed, bankroll = %self.total_bankroll, "Settlement processed and exposure freed");
    }

    pub fn record_trade(&mut self, result: &TradeResult) {
        self.daily_pnl += result.profit;
        self.daily_fees += result.leg_a_fee + result.leg_b_fee;
        self.total_bankroll += result.profit;
        self.trades_today += 1;

        match result.status {
            TradeStatus::Success => {
                self.success_today += 1;
                self.exec_history.push_back((chrono::Utc::now(), true));
                self.exec_success_count += 1;
            }
            TradeStatus::Fail | TradeStatus::Partial => {
                self.fail_today += 1;
                self.exec_history.push_back((chrono::Utc::now(), false));
            }
        }

        if self.total_bankroll > self.peak_bankroll {
            self.peak_bankroll = self.total_bankroll;
        }

        // Evict stale execution history entries, tracking success count decrements
        let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
        while let Some(&(time, was_success)) = self.exec_history.front() {
            if time < cutoff || self.exec_history.len() > 10000 {
                self.exec_history.pop_front();
                if was_success { self.exec_success_count = self.exec_success_count.saturating_sub(1); }
            } else {
                break;
            }
        }
        
        // O(1) success rate calculation using running counter
        if !self.exec_history.is_empty() {
            self.exec_success_rate = Decimal::from(self.exec_success_count as u64) / Decimal::from(self.exec_history.len() as u64);
        }

        info!(profit = %result.profit, bankroll = %self.total_bankroll, daily_pnl = %self.daily_pnl, "Trade recorded");
    }

    pub fn add_exposure(&mut self, platform: Platform, amount: Decimal) {
        *self.platform_exposure.entry(platform).or_insert(Decimal::ZERO) += amount;
    }

    pub fn remove_exposure(&mut self, platform: Platform, amount: Decimal) {
        if let Some(exp) = self.platform_exposure.get_mut(&platform) {
            *exp = (*exp - amount).max(Decimal::ZERO);
        }
    }

    pub fn reset_daily(&mut self) {
        self.daily_pnl = Decimal::ZERO;
        self.daily_fees = Decimal::ZERO;
        self.daily_start_bankroll = self.total_bankroll;
        self.trades_today = 0;
        self.success_today = 0;
        self.fail_today = 0;
        self.day_start = Utc::now();
        // REMOVED: peak_bankroll reset. 
        // The Kelly calculator in src/risk/kelly.rs already handles recovery 
        // smoothly by restoring the fraction when drawdown < 5%. Resetting peak 
        // here causes lethal over-sizing during multi-day losing streaks.
        info!(bankroll = %self.total_bankroll, "Daily counters reset");
    }

    pub fn restore_state(&mut self, cumulative_profit: Decimal) {
        self.total_bankroll += cumulative_profit;
        self.peak_bankroll = self.total_bankroll.max(self.peak_bankroll);
        tracing::info!(cumulative_profit = %cumulative_profit, "Bankroll state restored from DB");
    }

    pub fn daily_snapshot(&self, kelly_utilization: Decimal) -> DailySnapshot {
        let success_rate = if self.trades_today > 0 {
            Decimal::from(self.success_today) / Decimal::from(self.trades_today)
        } else {
            Decimal::ZERO
        };

        DailySnapshot {
            date: Utc::now().date_naive(),
            bankroll: self.total_bankroll,
            gross_pnl: self.daily_pnl + self.daily_fees,
            fees_paid: self.daily_fees,
            net_pnl: self.daily_pnl,
            trades_count: self.trades_today,
            success_count: self.success_today,
            fail_count: self.fail_today,
            success_rate,
            peak_bankroll: self.peak_bankroll,
            drawdown_pct: self.drawdown_pct(),
            kelly_utilization,
            report_sent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_daily_reset_preserves_peak() {
        let mut bm = BankrollManager::new(dec!(1000));
        bm.record_settlement(dec!(500)); // total is 1500, peak is 1500
        assert_eq!(bm.peak_bankroll(), dec!(1500));
        bm.reset_daily();
        // M-10 FIX: Enforce via unit test that peak bankroll persists across daily resets
        assert_eq!(bm.peak_bankroll(), dec!(1500));
    }

    #[test]
    fn test_bankroll_record_trade_and_settlement() {
        let mut bm = BankrollManager::new(dec!(1000));
        
        let trade = TradeResult {
            trade_id: 1, opp_id: Uuid::new_v4(), market_id: Uuid::new_v4(),
            market_question: "".into(), leg_a_platform: Platform::Polymarket, leg_a_side: Side::Yes,
            leg_a_price: dec!(0.4), leg_a_size: dec!(10), leg_a_fill_price: dec!(0.4), leg_a_fee: dec!(0.1),
            leg_b_platform: Platform::Kalshi, leg_b_side: Side::No, leg_b_price: dec!(0.5), leg_b_size: dec!(10),
            leg_b_fill_price: dec!(0.5), leg_b_fee: dec!(0.1), raw_spread: dec!(0.1), net_spread: dec!(0.08),
            profit: dec!(0.8), status: TradeStatus::Success, failure_reason: None, execution_ms: 50,
            executed_at: chrono::Utc::now(), bankroll_after: dec!(0), bankroll_change_pct: dec!(0), approved_size: dec!(10)
        };
        
        bm.record_trade(&trade);
        assert_eq!(bm.total_bankroll(), dec!(1000.8));
        assert_eq!(bm.success_today(), 1);
        
        bm.record_settlement(&crate::types::SettlementResult {
            realized_pnl: dec!(10),
            platform: Platform::Polymarket,
            market_id: Uuid::new_v4(),
            quantity: dec!(10),
            avg_entry_price: dec!(0.5),
        }); 
        assert_eq!(bm.total_bankroll(), dec!(1010.8));
        assert_eq!(bm.peak_bankroll(), dec!(1010.8));
    }
}

use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct RiskState {
    pub bankroll: Decimal,
    pub daily_loss_pct: Decimal,
    pub drawdown_pct: Decimal,
    pub platform_a_exposure_pct: Decimal,
    pub platform_b_exposure_pct: Decimal,
    pub market_exposure_pct: Decimal,
    pub exec_success_rate: Decimal,
    /// Total aggregate exposure across all platforms as a fraction of bankroll
    pub total_exposure_pct: Decimal,
}

pub enum BankrollMsg {
    ReserveCapital {
        leg_a_exposure: Decimal,
        leg_b_exposure: Decimal,
        platform_a: Platform,
        platform_b: Platform,
        market_id: Uuid,
        reply: oneshot::Sender<bool>,
    },
    ReleaseCapital {
        leg_a_exposure: Decimal,
        leg_b_exposure: Decimal,
        platform_a: Platform,
        platform_b: Platform,
        market_id: Uuid,
    },
    ProcessTrade(TradeResult, oneshot::Sender<TradeResult>),
    RecordSettlement(SettlementResult),
    GetSnapshot(Decimal, oneshot::Sender<DailySnapshot>),
    GetRiskState {
        platform_a: Platform,
        platform_b: Platform,
        market_id: Uuid,
        reply: oneshot::Sender<RiskState>,
    },
}

#[derive(Clone)]
pub struct BankrollHandle {
    pub tx: mpsc::Sender<BankrollMsg>,
}

impl BankrollHandle {
    pub fn new(mut manager: BankrollManager) -> Self {
        let (tx, mut rx) = mpsc::channel(10_000);
        
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    BankrollMsg::ReserveCapital { leg_a_exposure, leg_b_exposure, platform_a, platform_b, market_id, reply } => {
                        let total_amount = leg_a_exposure + leg_b_exposure;
                        let available = (manager.total_bankroll() - manager.total_exposure()).max(Decimal::ZERO);
                        if available >= total_amount {
                            manager.add_exposure(platform_a, leg_a_exposure);
                            manager.add_exposure(platform_b, leg_b_exposure);
                            manager.add_market_exposure(market_id, total_amount);
                            let _ = reply.send(true);
                        } else {
                            let _ = reply.send(false);
                        }
                    }
                    BankrollMsg::ProcessTrade(mut trade, reply) => {
                        manager.record_trade(&trade);
                        
                        trade.bankroll_after = manager.total_bankroll();
                        let pre_trade = manager.total_bankroll() - trade.profit;
                        trade.bankroll_change_pct = if pre_trade > rust_decimal::Decimal::ZERO {
                            (trade.profit / pre_trade) * rust_decimal_macros::dec!(100.0)
                        } else {
                            rust_decimal::Decimal::ZERO
                        };

                        if trade.status == TradeStatus::Fail {
                            let leg_a_exp = trade.approved_size * trade.leg_a_price;
                            let leg_b_exp = trade.approved_size * trade.leg_b_price;
                            manager.remove_exposure(trade.leg_a_platform, leg_a_exp);
                            manager.remove_exposure(trade.leg_b_platform, leg_b_exp);
                            manager.remove_market_exposure(trade.market_id, leg_a_exp + leg_b_exp);
                        } else {
                            let reserved_a = trade.approved_size * trade.leg_a_price;
                            let actual_a = trade.leg_a_size * trade.leg_a_fill_price;
                            if reserved_a > actual_a { manager.remove_exposure(trade.leg_a_platform, reserved_a - actual_a); }
                            else if actual_a > reserved_a { manager.add_exposure(trade.leg_a_platform, actual_a - reserved_a); }

                            let reserved_b = trade.approved_size * trade.leg_b_price;
                            let actual_b = trade.leg_b_size * trade.leg_b_fill_price;
                            if reserved_b > actual_b { manager.remove_exposure(trade.leg_b_platform, reserved_b - actual_b); }
                            else if actual_b > reserved_b { manager.add_exposure(trade.leg_b_platform, actual_b - reserved_b); }
                        }
                        let _ = reply.send(trade);
                    }
                    BankrollMsg::ReleaseCapital { leg_a_exposure, leg_b_exposure, platform_a, platform_b, market_id } => {
                        manager.remove_exposure(platform_a, leg_a_exposure);
                        manager.remove_exposure(platform_b, leg_b_exposure);
                        manager.remove_market_exposure(market_id, leg_a_exposure + leg_b_exposure);
                    }
                    BankrollMsg::RecordSettlement(settlement) => manager.record_settlement(&settlement),
                    BankrollMsg::GetSnapshot(kelly_frac, reply) => {
                        let _ = reply.send(manager.daily_snapshot(kelly_frac));
                    }
                    BankrollMsg::GetRiskState { platform_a, platform_b, market_id, reply } => {
                        let state = RiskState {
                            bankroll: manager.total_bankroll(),
                            daily_loss_pct: manager.daily_loss_pct(),
                            drawdown_pct: manager.drawdown_pct(),
                            platform_a_exposure_pct: manager.platform_exposure_pct(&platform_a),
                            platform_b_exposure_pct: manager.platform_exposure_pct(&platform_b),
                            market_exposure_pct: manager.market_exposure_pct(&market_id),
                            exec_success_rate: manager.exec_success_rate(),
                            total_exposure_pct: manager.total_exposure_pct(),
                        };
                        let _ = reply.send(state);
                    }
                }
            }
        });
        Self { tx }
    }

    pub async fn process_trade(&self, trade: TradeResult) -> TradeResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(BankrollMsg::ProcessTrade(trade, reply_tx)).await;
        reply_rx.await.expect("Bankroll actor died")
    }
    
    pub async fn record_settlement(&self, settlement: SettlementResult) {
        let _ = self.tx.send(BankrollMsg::RecordSettlement(settlement)).await;
    }

    pub async fn get_snapshot(&self, kelly_frac: Decimal) -> DailySnapshot {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(BankrollMsg::GetSnapshot(kelly_frac, reply_tx)).await;
        reply_rx.await.expect("Bankroll actor died")
    }

    pub async fn get_risk_state(&self, platform_a: Platform, platform_b: Platform, market_id: Uuid) -> RiskState {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(BankrollMsg::GetRiskState { platform_a, platform_b, market_id, reply: reply_tx }).await;
        reply_rx.await.expect("Bankroll actor died")
    }
}
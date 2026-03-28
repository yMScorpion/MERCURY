use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tracing::info;

use crate::types::*;

pub struct BankrollManager {
    total_bankroll: Decimal,
    peak_bankroll: Decimal,
    platform_balances: HashMap<Platform, Decimal>,
    platform_exposure: HashMap<Platform, Decimal>,
    daily_pnl: Decimal,
    daily_start_bankroll: Decimal,
    daily_fees: Decimal,
    day_start: DateTime<Utc>,
    trades_today: i32,
    success_today: i32,
    fail_today: i32,
    exec_success_rate: Decimal,
    exec_history: Vec<bool>,
}

impl BankrollManager {
    pub fn new(initial_bankroll: Decimal) -> Self {
        Self {
            total_bankroll: initial_bankroll,
            peak_bankroll: initial_bankroll,
            platform_balances: HashMap::new(),
            platform_exposure: HashMap::new(),
            daily_pnl: Decimal::ZERO,
            daily_start_bankroll: initial_bankroll,
            daily_fees: Decimal::ZERO,
            day_start: Utc::now(),
            trades_today: 0,
            success_today: 0,
            fail_today: 0,
            exec_success_rate: dec!(0.90),
            exec_history: Vec::new(),
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
        if self.daily_start_bankroll > Decimal::ZERO {
            // Only report a loss percentage when daily_pnl is negative; profitable
            // days must return 0 so the circuit breaker never trips on good days.
            (-self.daily_pnl / self.daily_start_bankroll * Decimal::from(100)).max(Decimal::ZERO)
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

    pub fn record_trade(&mut self, result: &TradeResult) {
        self.daily_pnl += result.profit;
        self.daily_fees += result.leg_a_fee + result.leg_b_fee;
        self.total_bankroll += result.profit;
        self.trades_today += 1;

        match result.status {
            TradeStatus::Success => {
                self.success_today += 1;
                self.exec_history.push(true);
            }
            TradeStatus::Fail | TradeStatus::Partial => {
                self.fail_today += 1;
                self.exec_history.push(false);
            }
        }

        if self.total_bankroll > self.peak_bankroll {
            self.peak_bankroll = self.total_bankroll;
        }

        if self.exec_history.len() > 100 {
            self.exec_history.drain(0..self.exec_history.len() - 100);
        }
        if !self.exec_history.is_empty() {
            let successes = self.exec_history.iter().filter(|&&s| s).count();
            self.exec_success_rate = Decimal::from(successes as u64) / Decimal::from(self.exec_history.len() as u64);
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
        // Reset the peak so drawdown is measured within the current day only.
        // Without this, a single historical loss permanently suppresses Kelly
        // sizing even after recovery ("fear lock").
        self.peak_bankroll = self.total_bankroll;
        info!(bankroll = %self.total_bankroll, "Daily counters reset");
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

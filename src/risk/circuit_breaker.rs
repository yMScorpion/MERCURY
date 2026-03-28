use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use tracing::{info, warn};


#[derive(Debug, Clone)]
pub struct BreakerTrip {
    pub breaker_type: String,
    pub details: String,
    pub action: String,
    pub resume_at: Option<DateTime<Utc>>,
}

pub struct CircuitBreakers {
    max_single_trade_pct: Decimal,
    max_daily_loss_pct: Decimal,
    max_drawdown_pct: Decimal,
    max_platform_exposure_pct: Decimal,
    max_exec_failure_rate: Decimal,
    gas_price_max_gwei: u64,
    stale_feed_timeout_secs: u64,
    max_open_positions: usize,
    trading_halted: bool,
    halt_resume_at: Option<DateTime<Utc>>,
    exec_failures: VecDeque<DateTime<Utc>>,
    exec_successes: VecDeque<DateTime<Utc>>,
    current_gas_gwei: u64,
}

impl CircuitBreakers {
    pub fn new(
        max_single_trade_pct: Decimal,
        max_daily_loss_pct: Decimal,
        max_drawdown_pct: Decimal,
        max_platform_exposure_pct: Decimal,
        gas_price_max_gwei: u64,
        stale_feed_timeout_secs: u64,
        max_open_positions: usize,
    ) -> Self {
        Self {
            max_single_trade_pct,
            max_daily_loss_pct,
            max_drawdown_pct,
            max_platform_exposure_pct,
            max_exec_failure_rate: dec!(0.15),
            gas_price_max_gwei,
            stale_feed_timeout_secs,
            max_open_positions,
            trading_halted: false,
            halt_resume_at: None,
            exec_failures: VecDeque::new(),
            exec_successes: VecDeque::new(),
            current_gas_gwei: 50,
        }
    }

    pub fn is_trading_halted(&mut self) -> bool {
        if !self.trading_halted {
            return false;
        }
        if let Some(resume) = self.halt_resume_at {
            if Utc::now() >= resume {
                self.trading_halted = false;
                self.halt_resume_at = None;
                return false;
            }
        }
        true
    }

    pub fn update_gas_price(&mut self, gwei: u64) {
        self.current_gas_gwei = gwei;
    }

    pub fn record_execution(&mut self, success: bool) {
        let now = Utc::now();
        if success {
            self.exec_successes.push_back(now);
        } else {
            self.exec_failures.push_back(now);
        }
        let cutoff = now - Duration::hours(1);
        while self.exec_failures.front().map(|t| *t < cutoff).unwrap_or(false) {
            self.exec_failures.pop_front();
        }
        while self.exec_successes.front().map(|t| *t < cutoff).unwrap_or(false) {
            self.exec_successes.pop_front();
        }
    }

    /// Check all circuit breakers and return any tripped breakers.
    ///
    /// # Parameter scales
    /// - `trade_size`: absolute notional value (same currency as `bankroll`)
    /// - `bankroll`: total capital in the same currency as `trade_size`
    /// - `daily_loss_pct`: **percent-scale** (0–100); e.g. 5.0 means 5 % loss today
    /// - `drawdown_pct`: **percent-scale** (0–100); e.g. 12.0 means 12 % drawdown from peak
    /// - `platform_exposure_pct`: **fraction-scale** (0–1); e.g. 0.30 means 30 % exposure
    /// - `open_positions`: current number of open positions
    /// - `involves_polymarket`: whether the trade touches an on-chain Polymarket leg
    ///
    /// Note: `max_daily_loss_pct` and `max_drawdown_pct` stored internally are
    /// fraction-scale (0–1) and are multiplied by 100 for comparison with the
    /// percent-scale `daily_loss_pct` / `drawdown_pct` arguments.
    /// `max_platform_exposure_pct` is stored fraction-scale and compared directly
    /// with the fraction-scale `platform_exposure_pct` argument.
    pub fn check_all(
        &mut self,
        trade_size: Decimal,
        bankroll: Decimal,
        daily_loss_pct: Decimal,
        drawdown_pct: Decimal,
        platform_exposure_pct: Decimal,
        open_positions: usize,
        involves_polymarket: bool,
    ) -> Vec<BreakerTrip> {
        let mut trips = Vec::new();

        // CB1: Max Single Trade Size
        if bankroll > Decimal::ZERO {
            let trade_pct = trade_size / bankroll;
            if trade_pct > self.max_single_trade_pct {
                trips.push(BreakerTrip {
                    breaker_type: "Max Single Trade Size".into(),
                    details: format!("Trade {}% of bankroll exceeds {}% limit",
                        (trade_pct * Decimal::from(100)).round_dp(2),
                        (self.max_single_trade_pct * Decimal::from(100)).round_dp(2)),
                    action: "Hard reject, no override".into(),
                    resume_at: None,
                });
            }
        }

        // CB2: Max Daily Loss
        if daily_loss_pct > self.max_daily_loss_pct * Decimal::from(100) {
            let resume = Utc::now() + Duration::hours(24);
            self.trading_halted = true;
            self.halt_resume_at = Some(resume);
            trips.push(BreakerTrip {
                breaker_type: "Max Daily Loss".into(),
                details: format!("Daily loss {:.2}% exceeds {:.2}% limit",
                    daily_loss_pct, self.max_daily_loss_pct * Decimal::from(100)),
                action: "Trading halted for 24h".into(),
                resume_at: Some(resume),
            });
        }

        // CB3: Max Drawdown from Peak
        if drawdown_pct > self.max_drawdown_pct * Decimal::from(100) {
            trips.push(BreakerTrip {
                breaker_type: "Max Drawdown".into(),
                details: format!("Drawdown {:.2}% exceeds {:.2}% limit",
                    drawdown_pct, self.max_drawdown_pct * Decimal::from(100)),
                action: "Reduce Kelly fraction to 0.10, alert".into(),
                resume_at: None,
            });
        }

        // CB4: Max Platform Exposure
        if platform_exposure_pct > self.max_platform_exposure_pct {
            trips.push(BreakerTrip {
                breaker_type: "Max Platform Exposure".into(),
                details: format!("Platform exposure {:.2}% exceeds {:.2}% limit",
                    (platform_exposure_pct * Decimal::from(100)).round_dp(2),
                    (self.max_platform_exposure_pct * Decimal::from(100)).round_dp(2)),
                action: "Reject new orders on overweight platform".into(),
                resume_at: None,
            });
        }

        // CB6: Execution Failure Rate
        let total_execs = self.exec_failures.len() + self.exec_successes.len();
        if total_execs > 5 {
            let failure_rate = Decimal::from(self.exec_failures.len() as u64)
                / Decimal::from(total_execs as u64);
            if failure_rate > self.max_exec_failure_rate {
                trips.push(BreakerTrip {
                    breaker_type: "Execution Failure Rate".into(),
                    details: format!("Failure rate {:.1}% in last hour ({} failures / {} total)",
                        (failure_rate * Decimal::from(100)).round_dp(1),
                        self.exec_failures.len(), total_execs),
                    action: "Pause trading, diagnose connectivity".into(),
                    resume_at: None,
                });
            }
        }

        // CB8: Gas Price Spike
        if involves_polymarket && self.current_gas_gwei > self.gas_price_max_gwei {
            trips.push(BreakerTrip {
                breaker_type: "Gas Price Spike".into(),
                details: format!("Polygon gas {} gwei exceeds {} gwei limit",
                    self.current_gas_gwei, self.gas_price_max_gwei),
                action: "Pause Polymarket-leg trades".into(),
                resume_at: None,
            });
        }

        // CB10: Max Open Positions
        if open_positions >= self.max_open_positions {
            trips.push(BreakerTrip {
                breaker_type: "Max Open Positions".into(),
                details: format!("{} positions >= {} limit", open_positions, self.max_open_positions),
                action: "Queue new opportunities until positions close".into(),
                resume_at: None,
            });
        }

        if !trips.is_empty() {
            warn!(count = trips.len(), "Circuit breakers tripped");
        }

        trips
    }

    pub fn reset_halt(&mut self) {
        self.trading_halted = false;
        self.halt_resume_at = None;
        info!("Trading halt reset");
    }
}

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

/// Parameters for circuit breaker evaluation. Bundles the 9 separate arguments
/// into a single struct for readability and future extensibility.
pub struct CheckParams {
    pub trade_size: Decimal,
    pub bankroll: Decimal,
    pub daily_loss_pct: Decimal,
    pub drawdown_pct: Decimal,
    pub platform_exposure_pct: Decimal,
    pub open_positions: usize,
    pub involves_polymarket: bool,
    pub ms_since_last_tick: u64,
    pub market_exposure_pct: Decimal,
    /// CB11: total aggregate exposure across all platforms as fraction of bankroll
    pub total_exposure_pct: Decimal,
}

pub struct CircuitBreakers {
    max_single_trade_pct: Decimal,
    max_daily_loss_pct: Decimal,
    max_drawdown_pct: Decimal,
    max_platform_exposure_pct: Decimal,
    max_exec_failure_rate: Decimal,
    gas_price_max_gwei: u64,
    stale_feed_timeout_ms: u64,
    max_open_positions: usize,
    /// CB5: max fraction of bankroll on any single market across all platforms
    max_single_market_exposure_pct: Decimal,
    /// CB11: max total aggregate exposure across all platforms
    max_total_exposure_pct: Decimal,
    /// CB7: halt after N consecutive failed trades
    max_consecutive_failures: usize,

    trading_halted: bool,
    halt_resume_at: Option<DateTime<Utc>>,
    exec_failures: VecDeque<DateTime<Utc>>,
    exec_successes: VecDeque<DateTime<Utc>>,
    current_gas_gwei: u64,
    /// CB7 state: running count of consecutive failures (reset on success)
    consecutive_failures: usize,
    engine_start_time: DateTime<Utc>,
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
            stale_feed_timeout_ms: stale_feed_timeout_secs * 1000,
            max_open_positions,
            max_single_market_exposure_pct: dec!(0.20),
            max_total_exposure_pct: dec!(0.70),
            max_consecutive_failures: 5,
            trading_halted: false,
            halt_resume_at: None,
            exec_failures: VecDeque::new(),
            exec_successes: VecDeque::new(),
            current_gas_gwei: 50,
            consecutive_failures: 0,
            engine_start_time: Utc::now(),
        }
    }

    pub fn update_limits(
        &mut self,
        max_single_trade_pct: Decimal,
        max_daily_loss_pct: Decimal,
        max_drawdown_pct: Decimal,
        max_platform_exposure_pct: Decimal,
        gas_price_max_gwei: u64,
        stale_feed_timeout_secs: u64,
        max_open_positions: usize,
    ) {
        self.max_single_trade_pct = max_single_trade_pct;
        self.max_daily_loss_pct = max_daily_loss_pct;
        self.max_drawdown_pct = max_drawdown_pct;
        self.max_platform_exposure_pct = max_platform_exposure_pct;
        self.gas_price_max_gwei = gas_price_max_gwei;
        self.stale_feed_timeout_ms = stale_feed_timeout_secs * 1000;
        self.max_open_positions = max_open_positions;
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
            self.consecutive_failures = 0;
        } else {
            self.exec_failures.push_back(now);
            self.consecutive_failures += 1;
        }
        let cutoff = now - Duration::hours(1);
        // HIGH-2: Add a hard cap of 500 to prevent unbounded growth during failure storms
        while self.exec_failures.front().map(|t| *t < cutoff).unwrap_or(false) || self.exec_failures.len() > 500 {
            self.exec_failures.pop_front();
        }
        while self.exec_successes.front().map(|t| *t < cutoff).unwrap_or(false) || self.exec_successes.len() > 500 {
            self.exec_successes.pop_front();
        }
    }

    /// Run all circuit breaker checks.
    pub fn check_all(&mut self, p: &CheckParams) -> Vec<BreakerTrip> {
        let mut trips = Vec::new();

        // CB1: Max Single Trade Size
        if p.bankroll > Decimal::ZERO {
            let trade_pct = p.trade_size / p.bankroll;
            if trade_pct > self.max_single_trade_pct {
                trips.push(BreakerTrip {
                    breaker_type: "CB1: Max Single Trade Size".into(),
                    details: format!("Trade {}% of bankroll exceeds {}% limit",
                        (trade_pct * Decimal::from(100)).round_dp(2),
                        (self.max_single_trade_pct * Decimal::from(100)).round_dp(2)),
                    action: "Hard reject, no override".into(),
                    resume_at: None,
                });
            }
        }

        // CB2: Max Daily Loss
        let max_daily_loss_percent = self.max_daily_loss_pct * Decimal::from(100);
        if p.daily_loss_pct > max_daily_loss_percent {
            let resume = Utc::now() + Duration::hours(24);
            self.trading_halted = true;
            self.halt_resume_at = Some(resume);
            trips.push(BreakerTrip {
                breaker_type: "CB2: Max Daily Loss".into(),
                details: format!("Daily loss {:.2}% exceeds {:.2}% limit",
                    p.daily_loss_pct, max_daily_loss_percent),
                action: "Trading halted for 24h".into(),
                resume_at: Some(resume),
            });
        }

        // CB3: Max Drawdown from Peak
        let max_drawdown_percent = self.max_drawdown_pct * Decimal::from(100);
        if p.drawdown_pct > max_drawdown_percent {
            trips.push(BreakerTrip {
                breaker_type: "CB3: Max Drawdown".into(),
                details: format!("Drawdown {:.2}% exceeds {:.2}% limit",
                    p.drawdown_pct, max_drawdown_percent),
                action: "Reduce Kelly fraction to 0.10, alert".into(),
                resume_at: None,
            });
        }

        // CB4: Max Platform Exposure
        if p.platform_exposure_pct > self.max_platform_exposure_pct {
            trips.push(BreakerTrip {
                breaker_type: "CB4: Max Platform Exposure".into(),
                details: format!("Platform exposure {:.2}% exceeds {:.2}% limit",
                    (p.platform_exposure_pct * Decimal::from(100)).round_dp(2),
                    (self.max_platform_exposure_pct * Decimal::from(100)).round_dp(2)),
                action: "Reject new orders on overweight platform".into(),
                resume_at: None,
            });
        }

        // CB5: Max Single-Market Correlated Exposure
        if p.market_exposure_pct > self.max_single_market_exposure_pct {
            trips.push(BreakerTrip {
                breaker_type: "CB5: Correlated Market Exposure".into(),
                details: format!("Market exposure {:.2}% exceeds {:.2}% limit",
                    (p.market_exposure_pct * Decimal::from(100)).round_dp(2),
                    (self.max_single_market_exposure_pct * Decimal::from(100)).round_dp(2)),
                action: "Skip — too much capital on one question".into(),
                resume_at: None,
            });
        }

        // CB6: Execution Failure Rate (>15% in 1h window)
        let total_execs = self.exec_failures.len() + self.exec_successes.len();
        if total_execs > 5 {
            let failure_rate = Decimal::from(self.exec_failures.len() as u64)
                / Decimal::from(total_execs as u64);
            if failure_rate > self.max_exec_failure_rate {
                trips.push(BreakerTrip {
                    breaker_type: "CB6: Execution Failure Rate".into(),
                    details: format!("Failure rate {:.1}% in last hour ({} failures / {} total)",
                        (failure_rate * Decimal::from(100)).round_dp(1),
                        self.exec_failures.len(), total_execs),
                    action: "Pause trading, diagnose connectivity".into(),
                    resume_at: None,
                });
            }
        }

        // CB7: Consecutive Failure Streak
        if self.consecutive_failures >= self.max_consecutive_failures {
            let resume = Utc::now() + Duration::minutes(10);
            self.trading_halted = true;
            self.halt_resume_at = Some(resume);
            trips.push(BreakerTrip {
                breaker_type: "CB7: Consecutive Failures".into(),
                details: format!("{} consecutive failed trades (limit {})",
                    self.consecutive_failures, self.max_consecutive_failures),
                action: "Pause 10 minutes — possible systemic issue".into(),
                resume_at: Some(resume),
            });
        }

        // CB8: Gas Price Spike (Polygon)
        if p.involves_polymarket && self.current_gas_gwei > self.gas_price_max_gwei {
            trips.push(BreakerTrip {
                breaker_type: "CB8: Gas Price Spike".into(),
                details: format!("Polygon gas {} gwei exceeds {} gwei limit",
                    self.current_gas_gwei, self.gas_price_max_gwei),
                action: "Pause Polymarket-leg trades".into(),
                resume_at: None,
            });
        }

        // CB9: Stale Feed Data (CRITICAL FIX 3-E)
        let uptime_ms = (Utc::now() - self.engine_start_time).num_milliseconds() as u64;
        if uptime_ms > self.stale_feed_timeout_ms {
            if p.ms_since_last_tick > self.stale_feed_timeout_ms && p.ms_since_last_tick != u64::MAX {
                trips.push(BreakerTrip {
                    breaker_type: "CB9: Stale Feed".into(),
                    details: format!("No tick received for {}ms (limit {}ms)",
                        p.ms_since_last_tick, self.stale_feed_timeout_ms),
                    action: "Halt trading — market data may be stale".into(),
                    resume_at: None,
                });
            }
        }

        // CB10: Max Open Positions
        if p.open_positions >= self.max_open_positions {
            trips.push(BreakerTrip {
                breaker_type: "CB10: Max Open Positions".into(),
                details: format!("{} positions >= {} limit", p.open_positions, self.max_open_positions),
                action: "Queue new opportunities until positions close".into(),
                resume_at: None,
            });
        }

        // CB11: Total Aggregate Exposure Kill Switch
        if p.total_exposure_pct > self.max_total_exposure_pct {
            self.trading_halted = true;
            trips.push(BreakerTrip {
                breaker_type: "CB11: Total Exposure Kill Switch".into(),
                details: format!("Total exposure {:.1}% exceeds {:.1}% kill-switch limit",
                    p.total_exposure_pct * Decimal::from(100),
                    self.max_total_exposure_pct * Decimal::from(100)),
                action: "Trading halted until positions close".into(),
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
        self.consecutive_failures = 0;
        info!("Trading halt reset");
    }

    /// Explicitly pause or resume trading via manual Telegram command
    pub fn manual_halt(&mut self, halt: bool) {
        self.trading_halted = halt;
        if halt {
            // Effectively permanent halt until manually restarted
            self.halt_resume_at = Some(Utc::now() + Duration::days(365));
            info!("System manually HALTED via Telegram command.");
        } else {
            self.halt_resume_at = None;
            info!("System manually RESUMED via Telegram command.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_circuit_breaker_trade_size() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05), // 5% max single trade
            dec!(0.10),
            dec!(0.20),
            dec!(0.50),
            100,
            5,
            10,
        );

        // Trade size 600 out of 10000 bankroll is 6%, which exceeds 5% limit
        let trips = cb.check_all(&CheckParams {
            trade_size: dec!(600),
            bankroll: dec!(10000),
            daily_loss_pct: dec!(0),
            drawdown_pct: dec!(0),
            platform_exposure_pct: dec!(0),
            open_positions: 0,
            involves_polymarket: false,
            ms_since_last_tick: 100,
            market_exposure_pct: dec!(0),
            total_exposure_pct: dec!(0),
        });
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].breaker_type, "CB1: Max Single Trade Size");
    }

    #[test]
    fn test_circuit_breaker_platform_exposure() {
        let mut cb = CircuitBreakers::new(
            dec!(0.05),
            dec!(0.10),
            dec!(0.20),
            dec!(0.30), // 30% max platform exposure
            100,
            5,
            10,
        );

        // Platform exposure is 35% (0.35)
        let trips = cb.check_all(&CheckParams {
            trade_size: dec!(100),
            bankroll: dec!(10000),
            daily_loss_pct: dec!(0),
            drawdown_pct: dec!(0),
            platform_exposure_pct: dec!(0.35),
            open_positions: 0,
            involves_polymarket: false,
            ms_since_last_tick: 100,
            market_exposure_pct: dec!(0),
            total_exposure_pct: dec!(0),
        });
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].breaker_type, "CB4: Max Platform Exposure");
    }
}
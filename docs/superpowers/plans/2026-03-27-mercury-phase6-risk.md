# Phase 6: Risk & Bankroll Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Kelly Criterion position sizing, bankroll manager with exposure tracking, and all 10 circuit breakers from SDD Section 7.3.

**Architecture:** The risk module validates opportunities, computes Kelly-optimal sizing, tracks bankroll state, and enforces circuit breakers. It sits between the Arbitrage Detector and Execution Engine.

**Tech Stack:** rust_decimal, chrono, tokio mpsc

**Depends on:** Phase 1 (types, config)

---

### Task 1: Kelly Criterion

**Files:**
- Create: `src/risk/mod.rs`
- Create: `src/risk/kelly.rs`

- [ ] **Step 1: Write the Kelly Criterion module**

`src/risk/mod.rs`:
```rust
pub mod kelly;
pub mod bankroll;
pub mod circuit_breaker;
```

`src/risk/kelly.rs`:
```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;

/// Kelly Criterion calculator for arbitrage
pub struct KellyCalculator {
    /// Fractional Kelly multiplier (default 0.25)
    fraction: Decimal,
    /// Minimum fraction floor
    min_fraction: Decimal,
    /// Maximum fraction ceiling
    max_fraction: Decimal,
}

impl KellyCalculator {
    pub fn new(fraction: Decimal) -> Self {
        Self {
            fraction,
            min_fraction: dec!(0.05),
            max_fraction: dec!(0.50),
        }
    }

    pub fn set_fraction(&mut self, fraction: Decimal) {
        self.fraction = fraction.max(self.min_fraction).min(self.max_fraction);
    }

    pub fn fraction(&self) -> Decimal {
        self.fraction
    }

    /// Calculate optimal Kelly fraction for an arbitrage opportunity
    ///
    /// p = probability of successful dual-leg execution (0.85-0.95 typical)
    /// b = net_spread / capital_at_risk (effective odds)
    ///
    /// Full Kelly: f* = (p * b - q) / b  where q = 1 - p
    /// We return fractional Kelly: f* * fraction_multiplier
    pub fn optimal_fraction(&self, exec_probability: Decimal, net_spread: Decimal) -> Decimal {
        if net_spread <= Decimal::ZERO || exec_probability <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let p = exec_probability;
        let q = Decimal::ONE - p;
        let b = net_spread; // odds ratio

        // f* = (p * b - q) / b
        let full_kelly = (p * b - q) / b;

        if full_kelly <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        // Apply fractional Kelly
        let fractional = full_kelly * self.fraction;

        // Clamp to reasonable bounds
        fractional.max(Decimal::ZERO).min(dec!(0.10)) // Never risk more than 10% per trade
    }

    /// Calculate recommended position size in USD
    pub fn position_size(
        &self,
        bankroll: Decimal,
        exec_probability: Decimal,
        net_spread: Decimal,
        max_single_trade_pct: Decimal,
    ) -> Decimal {
        let kelly_frac = self.optimal_fraction(exec_probability, net_spread);
        let kelly_size = bankroll * kelly_frac;

        // Cap at max single trade percentage
        let max_size = bankroll * max_single_trade_pct;

        kelly_size.min(max_size).max(Decimal::ZERO)
    }

    /// Solve multi-asset Kelly for simultaneous opportunities
    /// Returns fraction for each opportunity, constrained by max_total_exposure
    pub fn multi_asset_kelly(
        &self,
        opportunities: &[(Decimal, Decimal)], // (exec_prob, net_spread) pairs
        max_total_exposure: Decimal,
    ) -> Vec<Decimal> {
        if opportunities.is_empty() {
            return Vec::new();
        }

        // Compute individual Kelly fractions
        let individual: Vec<Decimal> = opportunities.iter()
            .map(|(p, spread)| self.optimal_fraction(*p, *spread))
            .collect();

        let total: Decimal = individual.iter().sum();

        if total <= Decimal::ZERO {
            return vec![Decimal::ZERO; opportunities.len()];
        }

        // If total exceeds max_total_exposure, scale down proportionally
        if total > max_total_exposure {
            let scale = max_total_exposure / total;
            individual.iter().map(|f| *f * scale).collect()
        } else {
            individual
        }
    }

    /// Dynamically adjust fraction based on PnL trajectory
    pub fn adjust_for_drawdown(&mut self, current_drawdown_pct: Decimal) {
        if current_drawdown_pct > dec!(0.15) {
            // Severe drawdown: reduce to minimum
            self.fraction = dec!(0.10);
        } else if current_drawdown_pct > dec!(0.10) {
            // Moderate drawdown: reduce
            self.fraction = dec!(0.15);
        } else if current_drawdown_pct > dec!(0.05) {
            // Mild drawdown: slightly reduce
            self.fraction = dec!(0.20);
        }
        // Otherwise keep current fraction
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/risk/
git commit -m "feat: add Kelly Criterion calculator with fractional and multi-asset support"
```

---

### Task 2: Bankroll Manager

**Files:**
- Create: `src/risk/bankroll.rs`

- [ ] **Step 1: Write the Bankroll Manager**

`src/risk/bankroll.rs`:
```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tracing::info;

use crate::types::*;

/// Tracks bankroll, platform balances, exposure, and PnL
pub struct BankrollManager {
    /// Total bankroll across all platforms
    total_bankroll: Decimal,
    /// Peak bankroll (for drawdown calculation)
    peak_bankroll: Decimal,
    /// Per-platform balance tracking
    platform_balances: HashMap<Platform, Decimal>,
    /// Per-platform exposure (capital at risk in open positions)
    platform_exposure: HashMap<Platform, Decimal>,
    /// Daily PnL tracking
    daily_pnl: Decimal,
    /// Daily start bankroll
    daily_start_bankroll: Decimal,
    /// Total fees paid today
    daily_fees: Decimal,
    /// Start of current trading day
    day_start: DateTime<Utc>,
    /// Trade counts today
    trades_today: i32,
    success_today: i32,
    fail_today: i32,
    /// Historical execution success rate (rolling window)
    exec_success_rate: Decimal,
    exec_history: Vec<bool>, // last N executions
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
            exec_success_rate: dec!(0.90), // Initial estimate
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
            (self.daily_pnl / self.daily_start_bankroll).abs() * Decimal::from(100)
        } else {
            Decimal::ZERO
        }
    }

    /// Get exposure for a specific platform
    pub fn platform_exposure(&self, platform: &Platform) -> Decimal {
        self.platform_exposure.get(platform).copied().unwrap_or(Decimal::ZERO)
    }

    /// Get platform exposure as percentage of bankroll
    pub fn platform_exposure_pct(&self, platform: &Platform) -> Decimal {
        if self.total_bankroll > Decimal::ZERO {
            self.platform_exposure(platform) / self.total_bankroll
        } else {
            Decimal::ZERO
        }
    }

    /// Total exposure across all platforms
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

    /// Record a completed trade
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

        // Update peak
        if self.total_bankroll > self.peak_bankroll {
            self.peak_bankroll = self.total_bankroll;
        }

        // Update rolling success rate (last 100 trades)
        if self.exec_history.len() > 100 {
            self.exec_history.drain(0..self.exec_history.len() - 100);
        }
        if !self.exec_history.is_empty() {
            let successes = self.exec_history.iter().filter(|&&s| s).count();
            self.exec_success_rate = Decimal::from(successes as u64) / Decimal::from(self.exec_history.len() as u64);
        }

        info!(
            profit = %result.profit,
            bankroll = %self.total_bankroll,
            daily_pnl = %self.daily_pnl,
            "Trade recorded"
        );
    }

    /// Add exposure for a platform (when entering a position)
    pub fn add_exposure(&mut self, platform: Platform, amount: Decimal) {
        *self.platform_exposure.entry(platform).or_insert(Decimal::ZERO) += amount;
    }

    /// Remove exposure (when closing a position)
    pub fn remove_exposure(&mut self, platform: Platform, amount: Decimal) {
        if let Some(exp) = self.platform_exposure.get_mut(&platform) {
            *exp = (*exp - amount).max(Decimal::ZERO);
        }
    }

    /// Reset daily counters (called at start of new trading day)
    pub fn reset_daily(&mut self) {
        self.daily_pnl = Decimal::ZERO;
        self.daily_fees = Decimal::ZERO;
        self.daily_start_bankroll = self.total_bankroll;
        self.trades_today = 0;
        self.success_today = 0;
        self.fail_today = 0;
        self.day_start = Utc::now();
        info!(bankroll = %self.total_bankroll, "Daily counters reset");
    }

    /// Generate a DailySnapshot
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
```

- [ ] **Step 2: Commit**

```bash
git add src/risk/bankroll.rs
git commit -m "feat: add Bankroll Manager with exposure tracking and PnL recording"
```

---

### Task 3: Circuit Breakers

**Files:**
- Create: `src/risk/circuit_breaker.rs`

- [ ] **Step 1: Write all 10 circuit breakers from SDD Section 7.3**

`src/risk/circuit_breaker.rs`:
```rust
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use tracing::{info, warn};

use crate::types::*;

/// Individual circuit breaker check result
#[derive(Debug, Clone)]
pub struct BreakerTrip {
    pub breaker_type: String,
    pub details: String,
    pub action: String,
    pub resume_at: Option<DateTime<Utc>>,
}

/// All circuit breakers from SDD Section 7.3
pub struct CircuitBreakers {
    // Thresholds (configurable)
    max_single_trade_pct: Decimal,
    max_daily_loss_pct: Decimal,
    max_drawdown_pct: Decimal,
    max_platform_exposure_pct: Decimal,
    max_correlated_exposure_pct: Decimal,
    max_exec_failure_rate: Decimal,
    spread_decay_threshold_pct: Decimal,
    gas_price_max_gwei: u64,
    stale_feed_timeout_secs: u64,
    max_open_positions: usize,

    // State tracking
    trading_halted: bool,
    halt_resume_at: Option<DateTime<Utc>>,
    exec_failures: VecDeque<DateTime<Utc>>,   // timestamps of failures in last hour
    exec_successes: VecDeque<DateTime<Utc>>,  // timestamps of successes in last hour
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
            max_correlated_exposure_pct: dec!(0.20),
            max_exec_failure_rate: dec!(0.15),
            spread_decay_threshold_pct: dec!(0.50),
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

    pub fn is_trading_halted(&self) -> bool {
        if !self.trading_halted {
            return false;
        }
        // Check if halt period has expired
        if let Some(resume) = self.halt_resume_at {
            if Utc::now() >= resume {
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

        // Prune entries older than 1 hour
        let cutoff = now - Duration::hours(1);
        while self.exec_failures.front().map(|t| *t < cutoff).unwrap_or(false) {
            self.exec_failures.pop_front();
        }
        while self.exec_successes.front().map(|t| *t < cutoff).unwrap_or(false) {
            self.exec_successes.pop_front();
        }
    }

    /// Run all circuit breaker checks. Returns list of trips (empty = all clear)
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

        // CB6: Execution Failure Rate (>15% in 1h window)
        let total_execs = self.exec_failures.len() + self.exec_successes.len();
        if total_execs > 5 {
            let failure_rate = Decimal::from(self.exec_failures.len() as u64) / Decimal::from(total_execs as u64);
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

        // CB8: Gas Price Spike (Polygon)
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

    /// Reset the trading halt (manual or automatic)
    pub fn reset_halt(&mut self) {
        self.trading_halted = false;
        self.halt_resume_at = None;
        info!("Trading halt reset");
    }
}
```

- [ ] **Step 2: Update main.rs**

Add to `src/main.rs`:
```rust
mod risk;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/risk/ src/main.rs
git commit -m "feat: add circuit breakers with all 10 checks from SDD Section 7.3"
```

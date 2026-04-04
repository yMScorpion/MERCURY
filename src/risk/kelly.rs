use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Kelly Criterion calculator for arbitrage
pub struct KellyCalculator {
    fraction: Decimal,
    min_fraction: Decimal,
    max_fraction: Decimal,
    config_max_fraction: Decimal,
    pub arb_loss_fraction: Decimal, // LOW-10
    hard_cap: Decimal,
}

impl KellyCalculator {
    pub fn new(fraction: Decimal, hard_cap: Decimal) -> Self {
        Self {
            fraction,
            min_fraction: dec!(0.05),
            max_fraction: fraction,
            config_max_fraction: fraction,
            arb_loss_fraction: dec!(0.005),
            hard_cap,
        }
    }

    pub fn update_fraction(&mut self, fraction: Decimal, hard_cap: Decimal) {
        // MED-9: Do not reset current fraction immediately if in drawdown
        self.config_max_fraction = fraction;
        self.max_fraction = fraction;
        self.hard_cap = hard_cap;
    }

    pub fn set_fraction(&mut self, fraction: Decimal) {
        self.fraction = fraction.max(self.min_fraction).min(self.max_fraction);
    }

    pub fn fraction(&self) -> Decimal {
        self.fraction
    }

    pub fn optimal_fraction(&self, exec_probability: Decimal, net_spread: Decimal) -> Decimal {
        if net_spread <= Decimal::ZERO || exec_probability <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let p = exec_probability;
        let q = Decimal::ONE - p;
        // Arb-aware Kelly: on execution failure we lose only the fees paid on the
        // failed leg (~0.5% of notional), not the full notional.  Using net_spread
        // as `b` in the standard formula (which assumes full-notional loss) produces
        // near-zero fractions for any realistic spread and kills all trading.
        let arb_loss_fraction = self.arb_loss_fraction; // LOW-10
        let full_kelly = (p * net_spread - q * arb_loss_fraction) / (net_spread + arb_loss_fraction);
        if full_kelly <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (full_kelly * self.fraction).max(Decimal::ZERO).min(self.hard_cap)
    }

    pub fn position_size(
        &self,
        bankroll: Decimal,
        exec_probability: Decimal,
        net_spread: Decimal,
        max_single_trade_pct: Decimal,
    ) -> Decimal {
        let kelly_frac = self.optimal_fraction(exec_probability, net_spread);
        let kelly_size = bankroll * kelly_frac;
        let max_size = bankroll * max_single_trade_pct;
        kelly_size.min(max_size).max(Decimal::ZERO)
    }

    pub fn multi_asset_kelly(
        &self,
        opportunities: &[(Decimal, Decimal)],
        max_total_exposure: Decimal,
    ) -> Vec<Decimal> {
        if opportunities.is_empty() {
            return Vec::new();
        }
        let individual: Vec<Decimal> = opportunities.iter()
            .map(|(p, spread)| self.optimal_fraction(*p, *spread))
            .collect();
        let total: Decimal = individual.iter().sum();
        if total <= Decimal::ZERO {
            return vec![Decimal::ZERO; opportunities.len()];
        }
        if total > max_total_exposure {
            let scale = max_total_exposure / total;
            individual.iter().map(|f| *f * scale).collect()
        } else {
            individual
        }
    }

    /// Adjust the Kelly fraction in response to drawdown.
    ///
    /// `current_drawdown_pct` must be **percent-scale** (0–100);
    /// e.g. pass `15.0` for a 15 % drawdown from peak.
    ///
    /// Drawdown tiers reduce sizing; full recovery (≤ 5 %) restores `max_fraction`
    /// so the system does not permanently under-trade after recovering from any loss.
    pub fn adjust_for_drawdown(&mut self, current_drawdown_pct: Decimal) {
        if current_drawdown_pct > dec!(15) {
            self.set_fraction(dec!(0.10));
        } else if current_drawdown_pct > dec!(10) {
            self.set_fraction(dec!(0.15));
        } else if current_drawdown_pct > dec!(5) {
            self.set_fraction(dec!(0.20));
        } else {
            // Drawdown ≤ 5 %: fully recovered — restore to max fraction.
            self.max_fraction = self.config_max_fraction; // MED-9: Restore config ceiling
            self.set_fraction(self.max_fraction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_optimal_fraction() {
        let calc = KellyCalculator::new(dec!(0.10), dec!(0.10));
        // High win prob, positive spread => > 0
        let frac = calc.optimal_fraction(dec!(0.90), dec!(0.05));
        assert!(frac > Decimal::ZERO);
        assert!(frac <= dec!(0.10));

        // Low win prob, negative spread => 0
        let frac_bad = calc.optimal_fraction(dec!(0.10), dec!(-0.05));
        assert_eq!(frac_bad, Decimal::ZERO);
    }
}

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Kelly Criterion calculator for arbitrage
pub struct KellyCalculator {
    fraction: Decimal,
    min_fraction: Decimal,
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

    pub fn optimal_fraction(&self, exec_probability: Decimal, net_spread: Decimal) -> Decimal {
        if net_spread <= Decimal::ZERO || exec_probability <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let p = exec_probability;
        let q = Decimal::ONE - p;
        let b = net_spread;
        let full_kelly = (p * b - q) / b;
        if full_kelly <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (full_kelly * self.fraction).max(Decimal::ZERO).min(dec!(0.10))
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

    pub fn adjust_for_drawdown(&mut self, current_drawdown_pct: Decimal) {
        if current_drawdown_pct > dec!(0.15) {
            self.fraction = dec!(0.10);
        } else if current_drawdown_pct > dec!(0.10) {
            self.fraction = dec!(0.15);
        } else if current_drawdown_pct > dec!(0.05) {
            self.fraction = dec!(0.20);
        }
    }
}

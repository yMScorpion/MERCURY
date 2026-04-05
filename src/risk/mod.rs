//! Risk management: Kelly sizing, bankroll actor, and circuit breakers.
pub mod kelly;
pub mod bankroll;
pub mod circuit_breaker;

#[cfg(test)]
mod bankroll_tests;

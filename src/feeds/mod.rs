//! Market data feeds: WebSocket and FIX connections to all supported platforms.
pub mod base;
pub mod commons;
// Re-export as `common` so all feed handlers can use `super::common::LocalBookOps`
pub use commons as common;
pub mod polymarket;
pub mod kalshi;
pub mod cdna;
pub mod forecastex;
pub mod normalizer;
pub mod discovery;

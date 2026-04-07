#![allow(dead_code, clippy::too_many_arguments, clippy::large_enum_variant, clippy::needless_range_loop, clippy::unnecessary_get_then_check)]

pub mod config;
pub mod crypto;
pub mod db;
pub mod engine;
pub mod execution;
pub mod feeds;
pub mod inventory;
pub mod monitoring;
pub mod risk;
pub mod telegram;
pub mod types;

#[cfg(test)]
mod integration;

pub use types::*;

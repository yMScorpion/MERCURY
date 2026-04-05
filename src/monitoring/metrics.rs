use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use crate::types::Platform;

#[derive(Debug)]
pub struct Metrics {
    pub ticks_received: AtomicU64,
    pub ticks_per_platform: std::sync::RwLock<HashMap<Platform, AtomicU64>>,
    pub last_tick_ns_per_platform: std::sync::RwLock<HashMap<Platform, AtomicU64>>,
    pub reconnects_per_platform: std::sync::RwLock<HashMap<Platform, AtomicU32>>,
    pub spreads_evaluated: AtomicU64,
    pub opportunities_detected: AtomicU64,
    pub opportunities_executed: AtomicU64,
    pub trades_success: AtomicU64,
    pub trades_failed: AtomicU64,
    pub ws_reconnects: AtomicU32,
    pub api_errors: AtomicU32,
    pub start_time: Instant,
    pub last_tick_ns: AtomicU64,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        let platforms = vec![Platform::Polymarket, Platform::Kalshi, Platform::Cdna, Platform::ForecastEx];
        let mut ticks = HashMap::new();
        let mut last_ticks = HashMap::new();
        let mut reconnects = HashMap::new();
        for p in platforms {
            ticks.insert(p, AtomicU64::new(0));
            last_ticks.insert(p, AtomicU64::new(0));
            reconnects.insert(p, AtomicU32::new(0));
        }

        Arc::new(Self {
            ticks_received: AtomicU64::new(0),
            ticks_per_platform: std::sync::RwLock::new(ticks),
            last_tick_ns_per_platform: std::sync::RwLock::new(last_ticks),
            reconnects_per_platform: std::sync::RwLock::new(reconnects),
            spreads_evaluated: AtomicU64::new(0),
            opportunities_detected: AtomicU64::new(0),
            opportunities_executed: AtomicU64::new(0),
            trades_success: AtomicU64::new(0),
            trades_failed: AtomicU64::new(0),
            ws_reconnects: AtomicU32::new(0),
            api_errors: AtomicU32::new(0),
            start_time: Instant::now(),
            last_tick_ns: AtomicU64::new(0),
        })
    }

    pub fn uptime_secs(&self) -> u64 { self.start_time.elapsed().as_secs() }
    pub fn inc_ticks(&self) {
        self.ticks_received.fetch_add(1, Ordering::Relaxed);
        self.last_tick_ns.store(crate::types::now_ns(), Ordering::Relaxed);
    }
    pub fn inc_spreads(&self) { self.spreads_evaluated.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_detected(&self) { self.opportunities_detected.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_executed(&self) { self.opportunities_executed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_success(&self) { self.trades_success.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_failed(&self) { self.trades_failed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_reconnects(&self) { self.ws_reconnects.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_api_errors(&self) { self.api_errors.fetch_add(1, Ordering::Relaxed); }

    /// Returns milliseconds since the last tick was received, or u64::MAX if none received yet.
    pub fn ms_since_last_tick(&self) -> u64 {
        let last = self.last_tick_ns.load(Ordering::Relaxed);
        if last == 0 {
            return u64::MAX;
        }
        let now = crate::types::now_ns();
        now.saturating_sub(last) / 1_000_000
    }
}
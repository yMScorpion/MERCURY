use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug)]
pub struct Metrics {
    pub ticks_received: AtomicU64,
    pub spreads_evaluated: AtomicU64,
    pub opportunities_detected: AtomicU64,
    pub opportunities_executed: AtomicU64,
    pub trades_success: AtomicU64,
    pub trades_failed: AtomicU64,
    pub ws_reconnects: AtomicU32,
    pub api_errors: AtomicU32,
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ticks_received: AtomicU64::new(0),
            spreads_evaluated: AtomicU64::new(0),
            opportunities_detected: AtomicU64::new(0),
            opportunities_executed: AtomicU64::new(0),
            trades_success: AtomicU64::new(0),
            trades_failed: AtomicU64::new(0),
            ws_reconnects: AtomicU32::new(0),
            api_errors: AtomicU32::new(0),
            start_time: Instant::now(),
        })
    }

    pub fn uptime_secs(&self) -> u64 { self.start_time.elapsed().as_secs() }
    pub fn inc_ticks(&self) { self.ticks_received.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_spreads(&self) { self.spreads_evaluated.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_detected(&self) { self.opportunities_detected.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_executed(&self) { self.opportunities_executed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_success(&self) { self.trades_success.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_failed(&self) { self.trades_failed.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_reconnects(&self) { self.ws_reconnects.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_api_errors(&self) { self.api_errors.fetch_add(1, Ordering::Relaxed); }
}

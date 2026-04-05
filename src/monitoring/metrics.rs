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
    pub fn inc_ticks_for_platform(&self, platform: crate::types::Platform) {
        self.ticks_received.fetch_add(1, Ordering::Relaxed);
        let now = crate::types::now_ns();
        self.last_tick_ns.store(now, Ordering::Relaxed);
        if let Ok(map) = self.last_tick_ns_per_platform.read() {
            if let Some(counter) = map.get(&platform) {
                counter.store(now, Ordering::Relaxed);
            }
        }
        if let Ok(map) = self.ticks_per_platform.read() {
            if let Some(counter) = map.get(&platform) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
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

/// Background task that checks metric thresholds every 60 seconds and sends alerts.
pub struct MetricAlertChecker {
    metrics: Arc<Metrics>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
    stale_timeout_ms: u64,
    db_size_alert_bytes: u64,
}

impl MetricAlertChecker {
    pub fn new(
        metrics: Arc<Metrics>,
        alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
        stale_timeout_ms: u64,
    ) -> Self {
        Self {
            metrics,
            alert_tx,
            stale_timeout_ms,
            db_size_alert_bytes: 500 * 1024 * 1024, // 500MB
        }
    }

    pub async fn run(self, db: Arc<dyn crate::db::Database>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut prev_reconnects: u32 = 0;
        let mut reconnect_window_start = tokio::time::Instant::now();

        loop {
            interval.tick().await;

            let success = self.metrics.trades_success.load(Ordering::Relaxed);
            let failed = self.metrics.trades_failed.load(Ordering::Relaxed);
            let total = success + failed;

            // Check: high failure rate (>20%)
            if total > 10 && failed * 5 > total {
                let rate = failed as f64 / total as f64 * 100.0;
                let _ = self.alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                    severity: "warning".into(),
                    message: format!("⚠️ High trade failure rate: {:.1}% ({}/{} trades failed)", rate, failed, total),
                });
            }

            // Check: excessive reconnects (>5 in last hour)
            let current_reconnects = self.metrics.ws_reconnects.load(Ordering::Relaxed);
            let elapsed_secs = reconnect_window_start.elapsed().as_secs();
            if elapsed_secs >= 3600 {
                let delta = current_reconnects.saturating_sub(prev_reconnects);
                if delta > 5 {
                    let _ = self.alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("⚠️ Excessive WS reconnects: {} in the last hour", delta),
                    });
                }
                prev_reconnects = current_reconnects;
                reconnect_window_start = tokio::time::Instant::now();
            }

            // Check: feed staleness
            let ms_since_tick = self.metrics.ms_since_last_tick();
            if ms_since_tick != u64::MAX && ms_since_tick > self.stale_timeout_ms {
                let _ = self.alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                    severity: "warning".into(),
                    message: format!("⚠️ Feed staleness: no tick received for {}ms (limit {}ms)", ms_since_tick, self.stale_timeout_ms),
                });
            }

            // Check: DB size
            if let Ok(size) = db.db_size_bytes().await {
                if size > self.db_size_alert_bytes {
                    let mb = size / 1_048_576;
                    let _ = self.alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                        severity: "warning".into(),
                        message: format!("⚠️ Database growing large: {}MB (alert threshold: 500MB)", mb),
                    });
                }
            }
        }
    }
}
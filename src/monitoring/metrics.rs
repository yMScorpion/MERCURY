use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use crate::types::Platform;

/// Per-platform flat atomics — zero lock contention on the hot tick path.
#[derive(Debug)]
pub struct PerPlatformCounters {
    pub ticks_polymarket: AtomicU64,
    pub ticks_kalshi: AtomicU64,
    pub ticks_cdna: AtomicU64,
    pub ticks_forecastex: AtomicU64,
    pub last_ns_polymarket: AtomicU64,
    pub last_ns_kalshi: AtomicU64,
    pub last_ns_cdna: AtomicU64,
    pub last_ns_forecastex: AtomicU64,
}

impl PerPlatformCounters {
    fn new() -> Self {
        Self {
            ticks_polymarket: AtomicU64::new(0),
            ticks_kalshi: AtomicU64::new(0),
            ticks_cdna: AtomicU64::new(0),
            ticks_forecastex: AtomicU64::new(0),
            last_ns_polymarket: AtomicU64::new(0),
            last_ns_kalshi: AtomicU64::new(0),
            last_ns_cdna: AtomicU64::new(0),
            last_ns_forecastex: AtomicU64::new(0),
        }
    }

    #[inline]
    fn inc_ticks(&self, platform: Platform, now_ns: u64) {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => {
                self.ticks_polymarket.fetch_add(1, Ordering::Relaxed);
                self.last_ns_polymarket.store(now_ns, Ordering::Relaxed);
            }
            Platform::Kalshi => {
                self.ticks_kalshi.fetch_add(1, Ordering::Relaxed);
                self.last_ns_kalshi.store(now_ns, Ordering::Relaxed);
            }
            Platform::Cdna => {
                self.ticks_cdna.fetch_add(1, Ordering::Relaxed);
                self.last_ns_cdna.store(now_ns, Ordering::Relaxed);
            }
            Platform::ForecastEx => {
                self.ticks_forecastex.fetch_add(1, Ordering::Relaxed);
                self.last_ns_forecastex.store(now_ns, Ordering::Relaxed);
            }
        }
    }

    pub fn last_ns(&self, platform: Platform) -> u64 {
        match platform {
            Platform::Polymarket | Platform::PolymarketUs => self.last_ns_polymarket.load(Ordering::Relaxed),
            Platform::Kalshi => self.last_ns_kalshi.load(Ordering::Relaxed),
            Platform::Cdna => self.last_ns_cdna.load(Ordering::Relaxed),
            Platform::ForecastEx => self.last_ns_forecastex.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct Metrics {
    pub ticks_received: AtomicU64,
    pub per_platform: PerPlatformCounters,
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
        Arc::new(Self {
            ticks_received: AtomicU64::new(0),
            per_platform: PerPlatformCounters::new(),
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

    #[inline]
    pub fn inc_ticks_for_platform(&self, platform: Platform) {
        self.ticks_received.fetch_add(1, Ordering::Relaxed);
        let now = crate::types::now_ns();
        self.last_tick_ns.store(now, Ordering::Relaxed);
        self.per_platform.inc_ticks(platform, now);
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
        if last == 0 { return u64::MAX; }
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
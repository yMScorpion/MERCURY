use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::types::{NormalizedTick, Platform};

#[async_trait]
pub trait FeedHandler: Send + Sync + 'static {
    fn platform(&self) -> Platform;

    /// Connect and start processing. Should run until disconnected.
    /// Returns Err on fatal error, Ok(()) on clean disconnect.
    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()>;

    /// Clear all local order book state.
    ///
    /// Called immediately on every disconnect (clean or error) before the reconnect
    /// backoff begins. This prevents stale book data from being visible to the spread
    /// engine during the reconnect window. The first snapshot received after reconnect
    /// will repopulate the books from authoritative exchange state.
    fn clear_books(&mut self);
}

/// Run a feed handler with automatic reconnection.
/// The loop exits cleanly when `token` is cancelled.
pub async fn run_with_reconnect(
    mut handler: Box<dyn FeedHandler>,
    tick_tx: broadcast::Sender<NormalizedTick>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
    liveness_tx: tokio::sync::mpsc::Sender<(crate::types::Platform, bool)>,
    metrics: std::sync::Arc<crate::monitoring::metrics::Metrics>,
    token: CancellationToken,
) {
    let platform = handler.platform();
    let mut backoff_secs = 1u64;
    // Progressive max backoff: start with 5s, escalate to 30s after repeated failures
    // to avoid IP bans during extended outages while staying responsive for brief glitches.
    let mut max_backoff = 5u64;
    let mut consecutive_failures: u32 = 0;

    loop {
        info!(%platform, "Connecting feed handler");
        let _ = liveness_tx.send((platform, true)).await; // Mark Alive
        
        let _connect_time = std::time::Instant::now();

        // Run the connection until cancelled or terminated
        let result = tokio::select! {
            _ = token.cancelled() => {
                info!(%platform, "Feed handler cancelled");
                Ok(())
            }
            res = handler.connect_and_run(tick_tx.clone()) => {
                res
            }
        };

        handler.clear_books();
        let _ = liveness_tx.send((platform, false)).await; // Mark Dead

        if token.is_cancelled() {
            info!(%platform, "Feed handler exited cleanly after cancellation");
            break;
        }

        match result {
            Ok(()) => {
                info!(%platform, "Feed handler disconnected cleanly — books cleared");
            }
            Err(e) => {
                error!(%platform, error = %e, "Feed handler error — books cleared");
                let _ = alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                    severity: "critical".into(),
                    message: format!(
                        "{platform} feed DISCONNECTED — market data is stale, \
                            trading halted on this platform until reconnect and book rebuild: {e}"
                    ),
                });
            }
        }

        // Check again before sleeping so a cancellation during backoff exits promptly.
        tokio::select! {
            _ = token.cancelled() => {
                info!(%platform, "Feed handler cancelled during backoff");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
        }

        consecutive_failures += 1;
        // After 10 consecutive failures, escalate max backoff to 30s to avoid IP bans
        if consecutive_failures > 10 {
            max_backoff = 30;
        }
        warn!(%platform, backoff_secs, consecutive_failures, "Reconnecting after backoff");
        metrics.inc_reconnects();
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}

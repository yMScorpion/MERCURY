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
}

/// Run a feed handler with automatic reconnection.
/// The loop exits cleanly when `token` is cancelled.
pub async fn run_with_reconnect(
    mut handler: Box<dyn FeedHandler>,
    tick_tx: broadcast::Sender<NormalizedTick>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
    token: CancellationToken,
) {
    let platform = handler.platform();
    let mut backoff_secs = 1u64;
    // 5 s maximum backoff — 60 s is an eternity for an HFT engine.
    // During a 60 s blind window, resting limit orders become stale and
    // can be sniped by other bots or filled at unfavorable prices.
    let max_backoff = 5u64;

    loop {
        info!(%platform, "Connecting feed handler");

        tokio::select! {
            _ = token.cancelled() => {
                info!(%platform, "Feed handler cancelled via token");
                break;
            }
            result = handler.connect_and_run(tick_tx.clone()) => {
                match result {
                    Ok(()) => {
                        info!(%platform, "Feed handler disconnected cleanly");
                        backoff_secs = 1;
                    }
                    Err(e) => {
                        error!(%platform, error = %e, "Feed handler error");
                        // Alert IMMEDIATELY on disconnect — the orchestrator must
                        // halt trading on this platform until the book is rebuilt.
                        // Use try_send (non-blocking) to avoid blocking the reconnect loop.
                        let _ = alert_tx.try_send(crate::types::AlertMessage::SystemAlert {
                            severity: "critical".into(),
                            message: format!(
                                "{platform} feed DISCONNECTED — market data is stale, \
                                 trading halted on this platform until reconnect and book rebuild: {e}"
                            ),
                        });
                    }
                }
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

        warn!(%platform, backoff_secs, "Reconnecting after backoff");
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}

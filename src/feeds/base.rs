use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::types::{NormalizedTick, Platform};

#[async_trait]
pub trait FeedHandler: Send + Sync + 'static {
    fn platform(&self) -> Platform;

    /// Connect and start processing. Should run until disconnected.
    /// Returns Err on fatal error, Ok(()) on clean disconnect.
    async fn connect_and_run(&mut self, tick_tx: broadcast::Sender<NormalizedTick>) -> Result<()>;
}

/// Run a feed handler with automatic reconnection
pub async fn run_with_reconnect(
    mut handler: Box<dyn FeedHandler>,
    tick_tx: broadcast::Sender<NormalizedTick>,
    alert_tx: tokio::sync::mpsc::Sender<crate::types::AlertMessage>,
) {
    let platform = handler.platform();
    let mut backoff_secs = 1u64;
    let max_backoff = 60u64;

    loop {
        info!(%platform, "Connecting feed handler");
        match handler.connect_and_run(tick_tx.clone()).await {
            Ok(()) => {
                info!(%platform, "Feed handler disconnected cleanly");
                backoff_secs = 1;
            }
            Err(e) => {
                error!(%platform, error = %e, "Feed handler error");
                let _ = alert_tx.send(crate::types::AlertMessage::SystemAlert {
                    severity: "warning".into(),
                    message: format!("{} feed disconnected: {}", platform, e),
                }).await;
            }
        }

        warn!(%platform, backoff_secs, "Reconnecting after backoff");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}

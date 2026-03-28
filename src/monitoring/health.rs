use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tracing::{error, info};

use super::metrics::Metrics;

pub async fn run_health_server(port: u16, metrics: Arc<Metrics>) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, addr, "Failed to bind health server");
            return;
        }
    };

    info!(addr, "Health server listening");

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let body = format!(
                    r#"{{"status":"ok","uptime_secs":{},"ticks":{},"spreads_evaluated":{},"opportunities_detected":{},"executed":{},"success":{},"failed":{},"ws_reconnects":{},"api_errors":{}}}"#,
                    metrics.uptime_secs(),
                    metrics.ticks_received.load(Ordering::Relaxed),
                    metrics.spreads_evaluated.load(Ordering::Relaxed),
                    metrics.opportunities_detected.load(Ordering::Relaxed),
                    metrics.opportunities_executed.load(Ordering::Relaxed),
                    metrics.trades_success.load(Ordering::Relaxed),
                    metrics.trades_failed.load(Ordering::Relaxed),
                    metrics.ws_reconnects.load(Ordering::Relaxed),
                    metrics.api_errors.load(Ordering::Relaxed),
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
            Err(e) => error!(error = %e, "Health server accept error"),
        }
    }
}

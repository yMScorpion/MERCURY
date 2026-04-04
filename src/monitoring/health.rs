use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncBufReadExt};
use tokio::net::TcpListener;
use tracing::{error, info};

use super::metrics::Metrics;

/// Simple HTTP health check endpoint.
/// Returns 200 when feeds are live, 503 when data is stale.
pub async fn run_health_server(port: u16, metrics: Arc<Metrics>, stale_timeout_ms: u64) {
    let addr = format!("127.0.0.1:{}", port); // CRIT-2: Bind to localhost to prevent external info leaks
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
                // L-1 FIX: Robust HTTP header parsing using BufReader instead of fixed buffer
                let mut req_line = String::new();
                let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    let mut reader = tokio::io::BufReader::new(&mut stream);
                    let _ = reader.read_line(&mut req_line).await;
                    let mut header_line = String::new();
                    while let Ok(n) = reader.read_line(&mut header_line).await {
                        if n <= 2 { break; } // \r\n or \n
                        header_line.clear();
                    }
                }).await;

                if !req_line.starts_with("GET ") {
                    let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
                    continue;
                }

                let ms_since_tick = metrics.ms_since_last_tick();
                let is_healthy = ms_since_tick < stale_timeout_ms || (ms_since_tick == u64::MAX && metrics.uptime_secs() < 60);

                let status_text = if is_healthy { "ok" } else { "degraded" };
                let http_status = if is_healthy { "200 OK" } else { "503 Service Unavailable" };

                let body = format!(
                    r#"{{"status":"{}","uptime_secs":{},"ms_since_last_tick":{},"ticks":{},"spreads_evaluated":{},"opportunities_detected":{},"executed":{},"success":{},"failed":{},"ws_reconnects":{},"api_errors":{}}}"#,
                    status_text,
                    metrics.uptime_secs(),
                    ms_since_tick,
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
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    http_status, body.len(), body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
            Err(e) => error!(error = %e, "Health server accept error"),
        }
    }
}
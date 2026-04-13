use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, AsyncBufReadExt};
use tokio::net::TcpListener;
use tracing::{error, info};

use super::metrics::Metrics;

/// HTTP server exposing both /health (JSON) and /metrics (Prometheus text format).
/// Binds to localhost only (CRIT-2 security).
pub async fn run_health_server(port: u16, metrics: Arc<Metrics>, stale_timeout_ms: u64) {
    // Bind to localhost by default for security. Set HEALTH_BIND_ADDR env var to
    // override (e.g., "0.0.0.0" for Prometheus scraping from a remote host).
    let bind_host = std::env::var("HEALTH_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{}:{}", bind_host, port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, addr, "Failed to bind health server");
            return;
        }
    };
    info!(addr, "Health + Prometheus metrics server listening");

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    handle_health_request(stream, &metrics, stale_timeout_ms).await;
                });
            }
            Err(e) => error!(error = %e, "Health server accept error"),
        }
    }
}

async fn handle_health_request(
    mut stream: tokio::net::TcpStream,
    metrics: &Metrics,
    stale_timeout_ms: u64,
) {
    let mut req_line = String::new();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut reader = tokio::io::BufReader::new(&mut stream);
        let _ = reader.read_line(&mut req_line).await;
        // Drain remaining headers
        let mut header_line = String::new();
        while let Ok(n) = reader.read_line(&mut header_line).await {
            if n <= 2 { break; }
            header_line.clear();
        }
    }).await;

    if !req_line.starts_with("GET ") {
        let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
        return;
    }

    // Extract path from "GET /path HTTP/1.1"
    let path = req_line.split_whitespace().nth(1).unwrap_or("/");

    match path {
        "/metrics" => {
            let body = render_prometheus(metrics, stale_timeout_ms);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
        "/health" | "/" => {
            let ms_since_tick = metrics.ms_since_last_tick();
            let is_healthy = ms_since_tick < stale_timeout_ms
                || (ms_since_tick == u64::MAX && metrics.uptime_secs() < 60);

            let status_text = if is_healthy { "ok" } else { "degraded" };
            let http_status = if is_healthy { "200 OK" } else { "503 Service Unavailable" };

            // Per-platform health breakdown — collect all data while holding the lock,
            // then drop the guard BEFORE any .await so the future stays Send.
            use crate::types::Platform;
            let platforms = [Platform::Polymarket, Platform::Kalshi, Platform::Cdna, Platform::ForecastEx];
            let platform_json = {
                let now_ns = crate::types::now_ns();
                let mut json = String::from("{");
                for (i, plat) in platforms.iter().enumerate() {
                    let last_val = metrics.per_platform.last_ns(*plat);
                    let ms = if last_val == 0 { u64::MAX } else { now_ns.saturating_sub(last_val) / 1_000_000 };
                    let plat_status = if ms < stale_timeout_ms || ms == u64::MAX { "healthy" } else { "degraded" };
                    let ms_str = if ms == u64::MAX { "null".to_string() } else { ms.to_string() };
                    let plat_name = format!("{}", plat).to_lowercase().replace(' ', "_");
                    if i > 0 { json.push(','); }
                    json.push_str(&format!(
                        r#""{}": {{"status": "{}", "ms_since_tick": {}}}"#,
                        plat_name, plat_status, ms_str
                    ));
                }
                json.push('}');
                json
            };

            let body = format!(
                r#"{{"status":"{}","uptime_secs":{},"ms_since_last_tick":{},"platforms":{}}}"#,
                status_text,
                metrics.uptime_secs(),
                ms_since_tick,
                platform_json,
            );
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                http_status, body.len(), body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
        }
    }
}

/// Render all metrics in Prometheus text exposition format.
fn render_prometheus(m: &Metrics, stale_timeout_ms: u64) -> String {
    let ms_raw = m.ms_since_last_tick();
    // Clamp u64::MAX (no-tick sentinel) to 0 for Prometheus — dashboards handle "no data" differently
    let ms_since_tick = if ms_raw == u64::MAX { 0u64 } else { ms_raw };
    let is_healthy: u8 = if ms_since_tick < stale_timeout_ms
        || (ms_since_tick == u64::MAX && m.uptime_secs() < 60)
    { 1 } else { 0 };

    let ticks = m.ticks_received.load(Ordering::Relaxed);
    let spreads = m.spreads_evaluated.load(Ordering::Relaxed);
    let detected = m.opportunities_detected.load(Ordering::Relaxed);
    let executed = m.opportunities_executed.load(Ordering::Relaxed);
    let success = m.trades_success.load(Ordering::Relaxed);
    let failed = m.trades_failed.load(Ordering::Relaxed);
    let reconnects = m.ws_reconnects.load(Ordering::Relaxed);
    let api_errors = m.api_errors.load(Ordering::Relaxed);
    let uptime = m.uptime_secs();

    format!(
        "# HELP mercury_up Whether the engine is healthy (1=up, 0=degraded).\n\
         # TYPE mercury_up gauge\n\
         mercury_up {is_healthy}\n\
         # HELP mercury_uptime_seconds Engine uptime in seconds.\n\
         # TYPE mercury_uptime_seconds gauge\n\
         mercury_uptime_seconds {uptime}\n\
         # HELP mercury_ms_since_last_tick Milliseconds since last feed tick.\n\
         # TYPE mercury_ms_since_last_tick gauge\n\
         mercury_ms_since_last_tick {ms_since_tick}\n\
         # HELP mercury_ticks_received_total Total feed ticks received.\n\
         # TYPE mercury_ticks_received_total counter\n\
         mercury_ticks_received_total {ticks}\n\
         # HELP mercury_spreads_evaluated_total Total spread evaluations.\n\
         # TYPE mercury_spreads_evaluated_total counter\n\
         mercury_spreads_evaluated_total {spreads}\n\
         # HELP mercury_opportunities_detected_total Arb opportunities detected.\n\
         # TYPE mercury_opportunities_detected_total counter\n\
         mercury_opportunities_detected_total {detected}\n\
         # HELP mercury_opportunities_executed_total Arb opportunities sent to executor.\n\
         # TYPE mercury_opportunities_executed_total counter\n\
         mercury_opportunities_executed_total {executed}\n\
         # HELP mercury_trades_success_total Successful trades.\n\
         # TYPE mercury_trades_success_total counter\n\
         mercury_trades_success_total {success}\n\
         # HELP mercury_trades_failed_total Failed trades.\n\
         # TYPE mercury_trades_failed_total counter\n\
         mercury_trades_failed_total {failed}\n\
         # HELP mercury_ws_reconnects_total WebSocket reconnection count.\n\
         # TYPE mercury_ws_reconnects_total counter\n\
         mercury_ws_reconnects_total {reconnects}\n\
         # HELP mercury_api_errors_total API error count.\n\
         # TYPE mercury_api_errors_total counter\n\
         mercury_api_errors_total {api_errors}\n"
    )
}
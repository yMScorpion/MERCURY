use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";
const MAX_MESSAGE_LENGTH: usize = 4096;
const RATE_LIMIT_PER_SECOND: u32 = 1;

#[derive(Clone)]
pub struct TelegramBot {
    client: reqwest::Client,
    token: String,
    last_send: Arc<Mutex<Instant>>,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
}

impl TelegramBot {
    pub fn new(token: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            token,
            last_send: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2))),
        }
    }

    /// Send a message to a specific chat, respecting rate limits
    pub async fn send_message(&self, chat_id: &str, text: &str) -> Result<()> {
        // Rate limiting
        {
            let mut last = self.last_send.lock().await;
            let elapsed = last.elapsed();
            let min_interval = Duration::from_millis(1000 / RATE_LIMIT_PER_SECOND as u64);
            if elapsed < min_interval {
                tokio::time::sleep(min_interval - elapsed).await;
            }
            *last = Instant::now();
        }

        // Truncate if too long
        let text = if text.len() > MAX_MESSAGE_LENGTH {
            &text[..MAX_MESSAGE_LENGTH - 20]
        } else {
            text
        };

        let url = format!("{}{}/sendMessage", TELEGRAM_API_BASE, self.token);
        let body = SendMessageRequest {
            chat_id,
            text,
            parse_mode: "HTML",
            disable_web_page_preview: true,
        };

        // Retry with exponential backoff (max 3 attempts)
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let resp_body: TelegramResponse = resp.json().await
                        .unwrap_or(TelegramResponse { ok: false, description: Some("Failed to parse response".into()) });

                    if resp_body.ok {
                        return Ok(());
                    }

                    let desc = resp_body.description.unwrap_or_default();
                    if status.as_u16() == 429 {
                        warn!(attempt, "Telegram rate limited, backing off");
                        if attempt >= 3 {
                            anyhow::bail!("Telegram rate limited after {} attempts: {}", attempt, desc);
                        }
                        tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                        continue;
                    }

                    anyhow::bail!("Telegram API error ({}): {}", status, desc);
                }
                Err(e) => {
                    if attempt >= 3 {
                        return Err(e).context("Failed to send Telegram message after 3 attempts");
                    }
                    warn!(attempt, error = %e, "Telegram send failed, retrying");
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
    }

    /// Escape HTML special characters for Telegram HTML parse mode
    pub fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

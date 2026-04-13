use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::warn;

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";
const MAX_MESSAGE_LENGTH: usize = 4096;
const RATE_LIMIT_PER_SECOND: u32 = 1;

#[derive(Clone)]
pub struct TelegramBot {
    client: reqwest::Client,
    token: zeroize::Zeroizing<String>, // HIGH-3: Secure token from memory dumps
    last_send: Arc<Mutex<Instant>>,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
    disable_web_page_preview: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_markup: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct AnswerCallbackQueryRequest<'a> {
    callback_query_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
    #[serde(default)]
    parameters: Option<TelegramParameters>,
}

#[derive(Deserialize, Default)]
struct TelegramParameters {
    #[serde(default)]
    retry_after: Option<u64>,
}

impl TelegramBot {

    /// SECURITY NOTE (CRIT-6): Telegram's API inherently requires the bot token to be
    /// passed in the URL path. Ensure that proxy servers, load balancers, or standard output
    /// do not log plain HTTP request URLs for api.telegram.org to prevent token leakage.
    pub fn new(token: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            token: zeroize::Zeroizing::new(token),
            last_send: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2))),
        }
    }

    /// Send a message to a specific chat, respecting rate limits
    pub async fn send_message(&self, chat_id: &str, text: &str) -> Result<()> {
        self.send_message_with_markup(chat_id, text, None).await
    }

    /// Send a message with an optional inline keyboard markup
    pub async fn send_message_with_markup(&self, chat_id: &str, text: &str, reply_markup: Option<serde_json::Value>) -> Result<()> {
        // Rate limiting: compute sleep duration while holding the lock,
        // then release the lock before sleeping so other callers aren't blocked.
        {
            let sleep_for = {
                let mut last = self.last_send.lock().await;
                let now = Instant::now();
                let min_interval = Duration::from_millis(1000 / RATE_LIMIT_PER_SECOND as u64);
                let next_allowed = *last + min_interval;
                
                if now < next_allowed {
                    let wait = next_allowed - now;
                    // Pre-emptively advance the timer for the NEXT concurrent caller
                    *last = next_allowed;
                    Some(wait)
                } else {
                    *last = now;
                    None
                }
            };
            if let Some(dur) = sleep_for {
                tokio::time::sleep(dur).await;
            }
        }

        // Truncate if too long, respecting UTF-8 boundaries
        let text = if text.len() > MAX_MESSAGE_LENGTH {
            let mut end = MAX_MESSAGE_LENGTH - 20;
            while end > 0 && !text.is_char_boundary(end) { end -= 1; }
            &text[..end]
        } else {
            text
        };

        let url = format!("{}{}/sendMessage", TELEGRAM_API_BASE, self.token.as_str());
        let body = SendMessageRequest {
            chat_id,
            text,
            parse_mode: "HTML",
            disable_web_page_preview: true,
            reply_markup,
        };

        // Retry with exponential backoff (max 3 attempts)
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let resp_body: TelegramResponse = resp.json().await
                        .unwrap_or(TelegramResponse { ok: false, description: Some("Failed to parse response".into()), parameters: None });

                    if resp_body.ok {
                        return Ok(());
                    }

                    let desc = resp_body.description.unwrap_or_default();
                    if status.as_u16() == 429 {
                        let retry_after = resp_body.parameters.and_then(|p| p.retry_after).unwrap_or(5);
                        warn!(attempt, retry_after, "Telegram rate limited, backing off");
                        if attempt >= 5 {
                            anyhow::bail!("Telegram rate limited after {} attempts: {}", attempt, desc);
                        }
                        tokio::time::sleep(Duration::from_secs(retry_after)).await;
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

    /// Long-poll the Telegram API to fetch new commands
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<crate::types::TelegramUpdate>> {
        // LOW-7: More robust URL construction
        let url = format!("{}{}/getUpdates?offset={}&timeout=5", 
            TELEGRAM_API_BASE, 
            self.token.as_str(), 
            offset
        );
        let resp = self.client.get(&url).send().await?
            .json::<crate::types::TelegramUpdatesResponse>().await?;
        
        if resp.ok {
            Ok(resp.result)
        } else {
            Ok(vec![])
        }
    }

    /// Escape HTML special characters for Telegram HTML parse mode
    pub fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    
/// Acknowledge a callback query to remove the loading state from Telegram buttons
    pub async fn answer_callback_query(&self, query_id: &str, text: Option<&str>) -> Result<()> {
        let url = format!("{}{}/answerCallbackQuery", TELEGRAM_API_BASE, self.token.as_str());
        let body = AnswerCallbackQueryRequest { callback_query_id: query_id, text };
        self.client.post(&url).json(&body).send().await?;
        Ok(())
    }
}


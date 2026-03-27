# Phase 2: Telegram Notification System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Telegram notification system with two channels: real-time trade alerts and daily summary reports.

**Architecture:** Direct HTTP calls to Telegram Bot API via `reqwest`. Messages queued via `mpsc` channels with rate limiting (30 msg/min) and exponential backoff retry. Two separate chat IDs for alerts vs reports.

**Tech Stack:** reqwest, tokio mpsc, chrono, rust_decimal, serde_json

**Depends on:** Phase 1 (types, config, DB)

---

### Task 1: Telegram Bot Client

**Files:**
- Create: `src/telegram/mod.rs`
- Create: `src/telegram/bot.rs`

- [ ] **Step 1: Write the Telegram bot HTTP client**

`src/telegram/mod.rs`:
```rust
pub mod bot;
pub mod alerts;
pub mod reports;
```

`src/telegram/bot.rs`:
```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";
const MAX_MESSAGE_LENGTH: usize = 4096;
const RATE_LIMIT_PER_SECOND: u32 = 1; // Conservative: 1 msg/sec (60/min, well under 30/min limit)

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
                        // Rate limited by Telegram
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
```

- [ ] **Step 2: Commit**

```bash
git add src/telegram/
git commit -m "feat: add Telegram bot HTTP client with rate limiting and retry"
```

---

### Task 2: Trade Alert Formatter

**Files:**
- Create: `src/telegram/alerts.rs`

- [ ] **Step 1: Write the alert message formatter and runner**

`src/telegram/alerts.rs`:
```rust
use crate::types::*;
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AlertService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<AlertMessage>,
}

impl AlertService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<AlertMessage>) -> Self {
        Self { bot, chat_id, rx }
    }

    pub async fn run(mut self) {
        info!("Telegram alert service started");
        while let Some(msg) = self.rx.recv().await {
            let text = match &msg {
                AlertMessage::TradeComplete(trade) => format_trade_alert(trade),
                AlertMessage::CircuitBreaker { breaker_type, details, action, resume_at } => {
                    format_circuit_breaker(breaker_type, details, action, resume_at.as_ref())
                }
                AlertMessage::SystemAlert { severity, message } => {
                    format_system_alert(severity, message)
                }
            };

            if let Err(e) = self.bot.send_message(&self.chat_id, &text).await {
                error!(error = %e, "Failed to send Telegram alert");
            }
        }
        info!("Telegram alert service stopped");
    }
}

fn format_trade_alert(trade: &TradeResult) -> String {
    let icon = match trade.status {
        TradeStatus::Success => "✅",
        TradeStatus::Fail => "❌",
        TradeStatus::Partial => "⚠️",
    };

    let label = match trade.status {
        TradeStatus::Success => "PROFIT",
        TradeStatus::Fail => "LOSS",
        TradeStatus::Partial => "PARTIAL",
    };

    let profit_icon = if trade.profit >= Decimal::ZERO { "💰" } else { "💸" };
    let profit_sign = if trade.profit >= Decimal::ZERO { "+" } else { "" };
    let pct_sign = if trade.bankroll_change_pct >= Decimal::ZERO { "+" } else { "" };

    let question = TelegramBot::escape_html(&trade.market_question);

    let mut msg = format!(
        "{icon} <b>ARBITRAGE #{id} — {label}</b>\n\
         \n\
         <b>Market:</b> \"{question}\"\n\
         Leg A: BUY {side_a} @ ${price_a} on {plat_a} ({size_a} contracts)\n\
         Leg B: BUY {side_b} @ ${price_b} on {plat_b} ({size_b} contracts)\n\
         \n\
         Raw Spread: {raw}% | Net Spread: {net}%\n\
         Fees: ${fee_a} + ${fee_b} = ${fees_total}",
        id = trade.trade_id,
        side_a = trade.leg_a_side,
        price_a = trade.leg_a_fill_price,
        plat_a = trade.leg_a_platform,
        size_a = trade.leg_a_size,
        side_b = trade.leg_b_side,
        price_b = trade.leg_b_fill_price,
        plat_b = trade.leg_b_platform,
        size_b = trade.leg_b_size,
        raw = (trade.raw_spread * Decimal::from(100)).round_dp(2),
        net = (trade.net_spread * Decimal::from(100)).round_dp(2),
        fee_a = trade.leg_a_fee,
        fee_b = trade.leg_b_fee,
        fees_total = trade.leg_a_fee + trade.leg_b_fee,
    );

    msg.push_str(&format!(
        "\n\n{profit_icon} <b>Profit: {profit_sign}${profit}</b>\n\
         📊 Bankroll: ${bankroll} ({pct_sign}{pct}%)\n\
         ⏱️ Execution: {exec_ms}ms",
        profit = trade.profit.round_dp(2),
        bankroll = trade.bankroll_after.round_dp(2),
        pct = trade.bankroll_change_pct.round_dp(4),
        exec_ms = trade.execution_ms,
    ));

    if let Some(reason) = &trade.failure_reason {
        msg.push_str(&format!("\n🔍 Reason: {}", TelegramBot::escape_html(reason)));
    }

    msg
}

fn format_circuit_breaker(
    breaker_type: &str,
    details: &str,
    action: &str,
    resume_at: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
    let mut msg = format!(
        "🚨 <b>CIRCUIT BREAKER TRIGGERED</b>\n\
         \n\
         <b>Type:</b> {}\n\
         <b>Details:</b> {}\n\
         <b>Action:</b> {}",
        TelegramBot::escape_html(breaker_type),
        TelegramBot::escape_html(details),
        TelegramBot::escape_html(action),
    );

    if let Some(resume) = resume_at {
        msg.push_str(&format!("\n<b>Resume:</b> {} UTC", resume.format("%Y-%m-%d %H:%M")));
    }

    msg
}

fn format_system_alert(severity: &str, message: &str) -> String {
    let icon = match severity.to_lowercase().as_str() {
        "critical" | "p1" => "🔴",
        "warning" | "p2" => "🟡",
        "info" | "p3" => "🔵",
        _ => "⚪",
    };

    format!(
        "{} <b>SYSTEM ALERT [{}]</b>\n\n{}",
        icon,
        TelegramBot::escape_html(severity),
        TelegramBot::escape_html(message),
    )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/telegram/alerts.rs
git commit -m "feat: add Telegram trade alert formatter with profit/loss notifications"
```

---

### Task 3: Daily Report Formatter

**Files:**
- Create: `src/telegram/reports.rs`

- [ ] **Step 1: Write the daily report formatter and runner**

`src/telegram/reports.rs`:
```rust
use crate::types::*;
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ReportService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<DailyReport>,
}

impl ReportService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<DailyReport>) -> Self {
        Self { bot, chat_id, rx }
    }

    pub async fn run(mut self) {
        info!("Telegram report service started");
        while let Some(report) = self.rx.recv().await {
            let text = format_daily_report(&report);
            if let Err(e) = self.bot.send_message(&self.chat_id, &text).await {
                error!(error = %e, "Failed to send daily report");
            }
        }
        info!("Telegram report service stopped");
    }
}

fn format_daily_report(report: &DailyReport) -> String {
    let s = &report.snapshot;
    let roi = if s.bankroll > Decimal::ZERO {
        (s.net_pnl / s.bankroll * Decimal::from(100)).round_dp(2)
    } else {
        Decimal::ZERO
    };

    let pnl_sign = if s.net_pnl >= Decimal::ZERO { "+" } else { "" };

    let mut msg = format!(
        "📈 <b>MERCURY DAILY REPORT — {date}</b>\n\
         \n\
         ═══ P&amp;L Summary ═══\n\
         Gross Profit:    {pnl_sign}${gross}\n\
         Total Fees:      -${fees}\n\
         <b>Net Profit:      {pnl_sign}${net}</b>\n\
         ROI Today:       {pnl_sign}{roi}%\n\
         \n\
         ═══ Trading Activity ═══\n\
         Opportunities Executed:  {total}\n\
         Successful Trades:       {success} ({rate}%)\n\
         Failed/Loss Trades:      {fail}\n",
        date = s.date,
        gross = s.gross_pnl.round_dp(2).abs(),
        fees = s.fees_paid.round_dp(2),
        net = s.net_pnl.round_dp(2).abs(),
        total = s.trades_count,
        success = s.success_count,
        fail = s.fail_count,
        rate = (s.success_rate * Decimal::from(100)).round_dp(1),
    );

    // Platform breakdown
    msg.push_str("\n═══ Platform Breakdown ═══\n");
    for (platform, stats) in &report.platform_breakdown {
        let p_sign = if stats.pnl >= Decimal::ZERO { "+" } else { "" };
        msg.push_str(&format!(
            "{}: ${} exposed │ {} trades │ {}${}\n",
            platform,
            stats.exposure.round_dp(2),
            stats.trade_count,
            p_sign,
            stats.pnl.round_dp(2),
        ));
    }

    // Risk metrics
    msg.push_str(&format!(
        "\n═══ Risk Metrics ═══\n\
         Bankroll:         ${bankroll}\n\
         Peak Bankroll:    ${peak}\n\
         Drawdown:         {dd}%\n\
         Kelly Utilization: {kelly}\n",
        bankroll = s.bankroll.round_dp(2),
        peak = s.peak_bankroll.round_dp(2),
        dd = s.drawdown_pct.round_dp(2),
        kelly = s.kelly_utilization.round_dp(2),
    ));

    // Top trades
    if !report.top_trades.is_empty() {
        msg.push_str("\n═══ Top Trades ═══\n");
        for (i, t) in report.top_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            msg.push_str(&format!(
                "{}. {}${} │ \"{}\" │ {}↔{}\n",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                t.leg_a_platform,
                t.leg_b_platform,
            ));
        }
    }

    if !report.worst_trades.is_empty() {
        msg.push_str("\n═══ Worst Trades ═══\n");
        for (i, t) in report.worst_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            let reason = t.failure_reason.as_deref().unwrap_or("n/a");
            msg.push_str(&format!(
                "{}. {}${} │ \"{}\" │ {}\n",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                reason,
            ));
        }
    }

    // System health
    let hours = report.uptime_secs / 3600;
    let mins = (report.uptime_secs % 3600) / 60;
    let db_mb = report.db_size_bytes as f64 / 1_048_576.0;
    msg.push_str(&format!(
        "\n═══ System Health ═══\n\
         Uptime: {}h {}m\n\
         WS Reconnects: {}\n\
         API Errors: {}\n\
         DB Size: {:.1}MB",
        hours, mins,
        report.ws_reconnects,
        report.api_errors,
        db_mb,
    ));

    msg
}

fn truncate_question(q: &str, max_len: usize) -> String {
    let escaped = TelegramBot::escape_html(q);
    if escaped.len() <= max_len {
        escaped
    } else {
        format!("{}...", &escaped[..max_len - 3])
    }
}
```

- [ ] **Step 2: Update main.rs module declarations**

Add to `src/main.rs`:
```rust
mod telegram;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/telegram/ src/main.rs
git commit -m "feat: add Telegram daily report formatter and report service"
```

use crate::types::{DailyReport, SystemCommand, Platform};
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};
use std::fmt::Write;
use std::sync::Arc;
use crate::db::Database;

pub struct ReportService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<DailyReport>,
    cmd_tx: mpsc::Sender<SystemCommand>,
    db: Arc<dyn Database>,
}

impl ReportService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<DailyReport>, cmd_tx: mpsc::Sender<SystemCommand>, db: Arc<dyn Database>) -> Self {
        Self { bot, chat_id, rx, cmd_tx, db }
    }

    pub async fn run(mut self) {
        info!("Telegram report & command service started");
        let mut offset = 0;
        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(3)); 
        
        // M-5 FIX: Read TELEGRAM_ADMIN_ID once outside the polling loop
        let admin_id = std::env::var("TELEGRAM_ADMIN_ID").unwrap_or_default();

        loop {
            tokio::select! {
                Some(report) = self.rx.recv() => {
                    let text = format_daily_report(&report);
                    if let Err(e) = self.bot.send_message(&self.chat_id, &text).await {
                        error!(error = %e, "Failed to send daily report");
                    }
                }
                _ = poll_interval.tick() => {
                    if let Ok(updates) = self.bot.get_updates(offset).await {
                        for update in updates {
                            offset = offset.max(update.update_id + 1);
                            
                            // Handle Interactive Button Clicks
                            if let Some(cb) = update.callback_query {
                                let _ = self.bot.answer_callback_query(&cb.id, None).await;
                                
                                if admin_id.is_empty() || cb.from.id.to_string() != admin_id {
                                    continue; 
                                }
                                
                                let data = cb.data.unwrap_or_default();
                                if data == "cmd_start" {
                                    let _ = self.cmd_tx.send(SystemCommand::StartTrading).await;
                                    let _ = self.bot.send_message(&self.chat_id, "▶️ Trading Engine: <b>RESUMED</b>").await;
                                } else if data == "cmd_stop" {
                                    let _ = self.cmd_tx.send(SystemCommand::StopTrading).await;
                                    let _ = self.bot.send_message(&self.chat_id, "⏸️ Trading Engine: <b>HALTED</b>").await;
                                } else if data == "req_report" {
                                    let _ = self.cmd_tx.send(SystemCommand::RequestDailyReport).await;
                                } else if data == "req_suspended" {
                                    if let Ok(suspended) = self.db.get_suspended_markets().await {
                                        if suspended.is_empty() {
                                            let _ = self.bot.send_message(&self.chat_id, "✅ No suspended markets requiring manual review.").await;
                                        } else {
                                            for m in suspended.into_iter().take(5) {
                                                let msg = format!("<b>Suspended Market:</b>\n{}\nConfidence: {}", TelegramBot::escape_html(&m.question), m.confidence);
                                                let markup = serde_json::json!({
                                                    "inline_keyboard": [[{"text": "⚡ Activate Market", "callback_data": format!("activate_{}", m.unified_id)}]]
                                                });
                                                let _ = self.bot.send_message_with_markup(&self.chat_id, &msg, Some(markup)).await;
                                            }
                                        }
                                    }
                                } else if data == "req_trades" {
                                    let today = chrono::Utc::now().date_naive();
                                    if let Ok(mut trades) = self.db.get_trades_for_date(today).await {
                                        if trades.is_empty() {
                                            let _ = self.bot.send_message(&self.chat_id, "ℹ️ No trades executed today.").await;
                                        } else {
                                            trades.sort_by(|a, b| b.executed_at.cmp(&a.executed_at));
                                            let mut msg = format!("📜 <b>Recent Trades (Total Today: {})</b>\n\n", trades.len());
                                            for t in trades.into_iter().take(5) {
                                                let sign = if t.profit >= rust_decimal::Decimal::ZERO { "+" } else { "" };
                                                msg.push_str(&format!("<b>#{}</b>: {}${} | {}\n", t.trade_id, sign, t.profit.round_dp(2), truncate_question(&t.market_question, 30)));
                                            }
                                            let _ = self.bot.send_message(&self.chat_id, &msg).await;
                                        }
                                    }
                                } else if data == "poly_on" {
                                    let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Polymarket)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "✅ Polymarket routing enabled").await;
                                } else if data == "poly_off" {
                                    let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Polymarket)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "❌ Polymarket routing disabled").await;
                                } else if data == "kalshi_on" {
                                    let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Kalshi)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "✅ Kalshi routing enabled").await;
                                } else if data == "kalshi_off" {
                                    let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Kalshi)).await;
                                    let _ = self.bot.send_message(&self.chat_id, "❌ Kalshi routing disabled").await;
                                } else if let Some(id_str) = data.strip_prefix("activate_") {
                                    if let Ok(uuid) = uuid::Uuid::parse_str(id_str) {
                                        let _ = self.cmd_tx.send(SystemCommand::ActivateMarket(uuid)).await;
                                        let _ = self.bot.send_message(&self.chat_id, &format!("✅ Activation command sent for:\n<code>{id_str}</code>")).await;
                                    }
                                }
                                continue;
                            }

                            // Handle Standard Text Commands
                            if let Some(msg) = update.message {
                                if msg.chat.id.to_string() != self.chat_id { continue; } 
                                
                                if !admin_id.is_empty() {
                                    let sender_id = msg.from.map(|u| u.id.to_string()).unwrap_or_default();
                                    if sender_id != admin_id { continue; }
                                }

                                if let Some(text) = msg.text {
                                    if text.to_lowercase() == "/menu" {
                                        self.send_main_menu().await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn send_main_menu(&self) {
        let markup = serde_json::json!({
            "inline_keyboard": [
                [
                    {"text": "▶️ Start Trading", "callback_data": "cmd_start"},
                    {"text": "⏸️ Stop Trading", "callback_data": "cmd_stop"}
                ],
                [
                    {"text": "📊 Live Financial Report", "callback_data": "req_report"}
                ],
                [
                    {"text": "🔍 Suspended Markets", "callback_data": "req_suspended"},
                    {"text": "📜 Recent Trades", "callback_data": "req_trades"}
                ],
                [
                    {"text": "🟢 Poly ON", "callback_data": "poly_on"},
                    {"text": "🔴 Poly OFF", "callback_data": "poly_off"}
                ],
                [
                    {"text": "🟢 Kalshi ON", "callback_data": "kalshi_on"},
                    {"text": "🔴 Kalshi OFF", "callback_data": "kalshi_off"}
                ]
            ]
        });

        let _ = self.bot.send_message_with_markup(
            &self.chat_id,
            "🎛️ <b>MERCURY Control Panel</b>\nSelect an operation below:",
            Some(markup)
        ).await;
    }
}

fn format_daily_report(report: &DailyReport) -> String {
    let s = &report.snapshot;
    // CRITICAL FIX: ROI must be calculated against the STARTING bankroll, not the ending bankroll.
    let starting_bankroll = s.bankroll - s.net_pnl;
    let roi = if starting_bankroll > Decimal::ZERO {
        (s.net_pnl / starting_bankroll * Decimal::from(100)).round_dp(2)
    } else {
        Decimal::ZERO
    };

    let pnl_sign = if s.net_pnl >= Decimal::ZERO { "+" } else { "-" };

    let mut msg = format!(
        "📈 <b>MERCURY DAILY REPORT — {date}</b>\n\
         \n\
         \u{2550}\u{2550}\u{2550} P&amp;L Summary \u{2550}\u{2550}\u{2550}\n\
         Gross Profit:    {pnl_sign}${gross}\n\
         Total Fees:      -${fees}\n\
         <b>Net Profit:      {pnl_sign}${net}</b>\n\
         ROI Today:       {pnl_sign}{roi}%\n\
         \n\
         \u{2550}\u{2550}\u{2550} Trading Activity \u{2550}\u{2550}\u{2550}\n\
         Opportunities Executed:  {total}\n\
         Successful Trades:       {success} ({rate}%)\n\
         Failed/Loss Trades:      {fail}\n",
        date = s.date,
        gross = s.gross_pnl.round_dp(2).abs(),
        fees = s.fees_paid.round_dp(2),
        net = s.net_pnl.round_dp(2),
        total = s.trades_count,
        success = s.success_count,
        fail = s.fail_count,
        rate = (s.success_rate * Decimal::from(100)).round_dp(1),
    );

    // Platform breakdown
    let _ = write!(msg, "\n\u{2550}\u{2550}\u{2550} Platform Breakdown \u{2550}\u{2550}\u{2550}\n");
    for (platform, stats) in &report.platform_breakdown {
        let p_sign = if stats.pnl >= Decimal::ZERO { "+" } else { "" };
        let _ = writeln!(
            msg,
            "{}: ${} exposed \u{2502} {} trades \u{2502} {}${}",
            platform,
            stats.exposure.round_dp(2),
            stats.trade_count,
            p_sign,
            stats.pnl.round_dp(2),
        );
    }

    // Risk metrics
    let _ = write!(
        msg,
        "\n\u{2550}\u{2550}\u{2550} Risk Metrics \u{2550}\u{2550}\u{2550}\n\
         Bankroll:         ${bankroll}\n\
         Peak Bankroll:    ${peak}\n\
         Drawdown:         {dd}%\n\
         Kelly Utilization: {kelly}\n",
        bankroll = s.bankroll.round_dp(2),
        peak = s.peak_bankroll.round_dp(2),
        dd = s.drawdown_pct.round_dp(2),
        kelly = s.kelly_utilization.round_dp(2),
    );

    // Top trades
    if !report.top_trades.is_empty() {
        let _ = write!(msg, "\n\u{2550}\u{2550}\u{2550} Top Trades \u{2550}\u{2550}\u{2550}\n");
        for (i, t) in report.top_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            let _ = writeln!(
                msg,
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}\u{2194}{}",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                t.leg_a_platform,
                t.leg_b_platform,
            );
        }
    }

    if !report.worst_trades.is_empty() {
        let _ = write!(msg, "\n\u{2550}\u{2550}\u{2550} Worst Trades \u{2550}\u{2550}\u{2550}\n");
        for (i, t) in report.worst_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            let reason = t.failure_reason.as_deref().unwrap_or("Unknown");
            let _ = writeln!(
                msg,
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}",
                i + 1,
                sign,
                t.profit.round_dp(2),
                truncate_question(&t.market_question, 30),
                reason,
            );
        }
    }

    // System health
    let hours = report.uptime_secs / 3600;
    let mins = (report.uptime_secs % 3600) / 60;
    let db_mb = report.db_size_bytes as f64 / 1_048_576.0;
    let _ = write!(
        msg,
        "\n\u{2550}\u{2550}\u{2550} System Health \u{2550}\u{2550}\u{2550}\n\
         Uptime: {}h {}m\n\
         WS Reconnects: {}\n\
         API Errors: {}\n\
         DB Size: {:.1}MB",
        hours, mins,
        report.ws_reconnects,
        report.api_errors,
        db_mb,
    );

    msg
}

fn truncate_question(q: &str, max_len: usize) -> String {
    if max_len <= 3 || q.len() <= max_len { 
        return TelegramBot::escape_html(q); 
    }
    let mut end = max_len - 3;
    while end > 0 && !q.is_char_boundary(end) { end -= 1; }
    let truncated = format!("{}...", &q[..end]);
    TelegramBot::escape_html(&truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DailySnapshot, TradeResult, PlatformDayStats};
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    #[test]
    fn test_format_daily_report_snapshot() {
        let mut platforms = HashMap::new();
        platforms.insert(Platform::Polymarket, PlatformDayStats {
            platform: Platform::Polymarket,
            exposure: dec!(1000.0),
            trade_count: 5,
            pnl: dec!(50.0),
        });

        let report = DailyReport {
            snapshot: DailySnapshot {
                date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                bankroll: dec!(10050.0),
                peak_bankroll: dec!(10100.0),
                net_pnl: dec!(150.0),
                gross_pnl: dec!(200.0),
                fees_paid: dec!(50.0),
                trades_count: 10,
                success_count: 8,
                fail_count: 2,
                success_rate: dec!(0.8),
                drawdown_pct: dec!(0.005),
                kelly_utilization: dec!(0.25),
                report_sent: false,
            },
            platform_breakdown: platforms,
            top_trades: vec![TradeResult {
                trade_id: 1,
                market_question: "Bitcoin hit 100k?".into(),
                profit: dec!(40.0),
                leg_a_platform: Platform::Polymarket,
                leg_b_platform: Platform::Kalshi,
                ..Default::default()
            }],
            worst_trades: vec![TradeResult {
                trade_id: 2,
                market_question: "ETH hit 10k?".into(),
                profit: dec!(-10.0),
                failure_reason: Some("Hedge failed".into()),
                ..Default::default()
            }],
            uptime_secs: 3661,
            ws_reconnects: 1,
            api_errors: 0,
            db_size_bytes: 1048576,
        };

        let output = format_daily_report(&report);
        insta::assert_snapshot!(output);
    }
}

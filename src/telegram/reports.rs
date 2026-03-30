use crate::types::*;
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ReportService {
    bot: TelegramBot,
    chat_id: String,
    rx: mpsc::Receiver<DailyReport>,
    cmd_tx: mpsc::Sender<SystemCommand>,
}

impl ReportService {
    pub fn new(bot: TelegramBot, chat_id: String, rx: mpsc::Receiver<DailyReport>, cmd_tx: mpsc::Sender<SystemCommand>) -> Self {
        Self { bot, chat_id, rx, cmd_tx }
    }

    pub async fn run(mut self) {
        info!("Telegram report & command service started");
        let mut offset = 0;
        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(3)); // Aggressive but safe poll

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
                            if let Some(msg) = update.message {
                                if msg.chat.id.to_string() != self.chat_id { continue; } // Restrict to authorized chat
                                if let Some(text) = msg.text {
                                    let txt = text.to_lowercase();
                                    if txt == "/start" {
                                        let _ = self.cmd_tx.send(SystemCommand::StartTrading).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{25b6}\u{fe0f} MERCURY Trading Engine: <b>RESUMED</b>").await;
                                    } else if txt == "/stop" {
                                        let _ = self.cmd_tx.send(SystemCommand::StopTrading).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{23f8}\u{fe0f} MERCURY Trading Engine: <b>HALTED</b>").await;
                                    } else if txt == "/enable polymarket" {
                                        let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Polymarket)).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{2705} Polymarket routing enabled").await;
                                    } else if txt == "/disable polymarket" {
                                        let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Polymarket)).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{274c} Polymarket routing disabled").await;
                                    } else if txt == "/enable kalshi" {
                                        let _ = self.cmd_tx.send(SystemCommand::EnablePlatform(Platform::Kalshi)).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{2705} Kalshi routing enabled").await;
                                    } else if txt == "/disable kalshi" {
                                        let _ = self.cmd_tx.send(SystemCommand::DisablePlatform(Platform::Kalshi)).await;
                                        let _ = self.bot.send_message(&self.chat_id, "\u{274c} Kalshi routing disabled").await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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

    let pnl_sign = if s.net_pnl >= Decimal::ZERO { "+" } else { "" };

    let mut msg = format!(
        "\u{1f4c8} <b>MERCURY DAILY REPORT \u{2014} {date}</b>\n\
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
        net = s.net_pnl.round_dp(2).abs(),
        total = s.trades_count,
        success = s.success_count,
        fail = s.fail_count,
        rate = (s.success_rate * Decimal::from(100)).round_dp(1),
    );

    // Platform breakdown
    msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Platform Breakdown \u{2550}\u{2550}\u{2550}\n"));
    for (platform, stats) in &report.platform_breakdown {
        let p_sign = if stats.pnl >= Decimal::ZERO { "+" } else { "" };
        msg.push_str(&format!(
            "{}: ${} exposed \u{2502} {} trades \u{2502} {}${}\n",
            platform,
            stats.exposure.round_dp(2),
            stats.trade_count,
            p_sign,
            stats.pnl.round_dp(2),
        ));
    }

    // Risk metrics
    msg.push_str(&format!(
        "\n\u{2550}\u{2550}\u{2550} Risk Metrics \u{2550}\u{2550}\u{2550}\n\
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
        msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Top Trades \u{2550}\u{2550}\u{2550}\n"));
        for (i, t) in report.top_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            msg.push_str(&format!(
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}\u{2194}{}\n",
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
        msg.push_str(&format!("\n\u{2550}\u{2550}\u{2550} Worst Trades \u{2550}\u{2550}\u{2550}\n"));
        for (i, t) in report.worst_trades.iter().enumerate() {
            let sign = if t.profit >= Decimal::ZERO { "+" } else { "" };
            let reason = t.failure_reason.as_deref().unwrap_or("n/a");
            msg.push_str(&format!(
                "{}. {}${} \u{2502} \"{}\" \u{2502} {}\n",
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
        "\n\u{2550}\u{2550}\u{2550} System Health \u{2550}\u{2550}\u{2550}\n\
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
    let truncated = if q.len() > max_len - 3 {
        let mut end = max_len - 3;
        while end > 0 && !q.is_char_boundary(end) { end -= 1; }
        format!("{}...", &q[..end])
    } else {
        q.to_string()
    };
    TelegramBot::escape_html(&truncated)
}

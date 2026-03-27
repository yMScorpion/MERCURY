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
        TradeStatus::Success => "\u{2705}",
        TradeStatus::Fail => "\u{274c}",
        TradeStatus::Partial => "\u{26a0}\u{fe0f}",
    };

    let label = match trade.status {
        TradeStatus::Success => "PROFIT",
        TradeStatus::Fail => "LOSS",
        TradeStatus::Partial => "PARTIAL",
    };

    let profit_icon = if trade.profit >= Decimal::ZERO { "\u{1f4b0}" } else { "\u{1f4b8}" };
    let profit_sign = if trade.profit >= Decimal::ZERO { "+" } else { "" };
    let pct_sign = if trade.bankroll_change_pct >= Decimal::ZERO { "+" } else { "" };

    let question = TelegramBot::escape_html(&trade.market_question);

    let mut msg = format!(
        "{icon} <b>ARBITRAGE #{id} \u{2014} {label}</b>\n\
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
         \u{1f4ca} Bankroll: ${bankroll} ({pct_sign}{pct}%)\n\
         \u{23f1}\u{fe0f} Execution: {exec_ms}ms",
        profit = trade.profit.round_dp(2),
        bankroll = trade.bankroll_after.round_dp(2),
        pct = trade.bankroll_change_pct.round_dp(4),
        exec_ms = trade.execution_ms,
    ));

    if let Some(reason) = &trade.failure_reason {
        msg.push_str(&format!("\n\u{1f50d} Reason: {}", TelegramBot::escape_html(reason)));
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
        "\u{1f6a8} <b>CIRCUIT BREAKER TRIGGERED</b>\n\
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
        "critical" | "p1" => "\u{1f534}",
        "warning" | "p2" => "\u{1f7e1}",
        "info" | "p3" => "\u{1f535}",
        _ => "\u{26aa}",
    };

    format!(
        "{} <b>SYSTEM ALERT [{}]</b>\n\n{}",
        icon,
        TelegramBot::escape_html(severity),
        TelegramBot::escape_html(message),
    )
}

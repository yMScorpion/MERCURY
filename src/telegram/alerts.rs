use crate::types::{AlertMessage, TradeResult, TradeStatus};
use super::bot::TelegramBot;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};
use std::fmt::Write;

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

    let _ = write!(
        msg,
        "\n\n{profit_icon} <b>Profit: {profit_sign}${profit}</b>\n\
         📊 Bankroll: ${bankroll} ({pct_sign}{pct}%)\n\
         ⏱️ Execution: {exec_ms}ms",
        profit = trade.profit.round_dp(2),
        bankroll = trade.bankroll_after.round_dp(2),
        pct = trade.bankroll_change_pct.round_dp(4),
        exec_ms = trade.execution_ms,
    );

    if let Some(reason) = &trade.failure_reason {
        let _ = write!(msg, "\n🔍 Reason: {}", TelegramBot::escape_html(reason));
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
        let _ = write!(msg, "\n<b>Resume:</b> {} UTC", resume.format("%Y-%m-%d %H:%M"));
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use rust_decimal_macros::dec;
        use chrono::{TimeZone, Utc};

        #[test]    fn test_format_trade_alert_snapshot() {
    let trade = TradeResult {
        trade_id: 123,
        opp_id: uuid::Uuid::nil(),
        market_id: uuid::Uuid::nil(),
        market_question: "Will Bitcoin hit $100k?".into(),
        leg_a_platform: Platform::Polymarket,
        leg_a_side: Side::Yes,
        leg_a_price: dec!(0.60),
        leg_a_size: dec!(100),
        leg_a_fill_price: dec!(0.61),
        leg_a_fee: dec!(0.5),
        leg_b_platform: Platform::Kalshi,
        leg_b_side: Side::No,
        leg_b_price: dec!(0.35),
        leg_b_size: dec!(100),
        leg_b_fill_price: dec!(0.36),
        leg_b_fee: dec!(0.1),
        raw_spread: dec!(0.05),
        net_spread: dec!(0.03),
        profit: dec!(5.0),
        status: TradeStatus::Success,
        failure_reason: None,
        execution_ms: 120,
        executed_at: Utc::now(),
        bankroll_after: dec!(1005.0),
        bankroll_change_pct: dec!(0.005),
        approved_size: dec!(100),
    };

    let alert = format_trade_alert(&trade);
    insta::assert_snapshot!(alert);
    }

    #[test]
    fn test_format_circuit_breaker_snapshot() {
    let resume = Utc.with_ymd_and_hms(2026, 3, 27, 12, 0, 0).unwrap();
    let alert = format_circuit_breaker(
        "ExposureLimit",
        "Total exposure exceeded $5000",
        "Pausing all trading",
        Some(&resume),
    );
    insta::assert_snapshot!(alert);
    }
    }

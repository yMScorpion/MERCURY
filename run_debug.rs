use rust_decimal_macros::dec;

fn main() {
    let price = dec!(0.50);
    let max_fee = rust_decimal_macros::dec!(0.07);
    let implied_fee = price * rust_decimal_macros::dec!(0.10);
    let fee = max_fee.min(implied_fee) * dec!(1);
    println!("Fee per contract on Kalshi: {}", fee);
    
    let polymarket_fee = dec!(1) * dec!(0.02) * dec!(0.50); // polymarket fee
    println!("Fee per contract on Polymarket: {}", polymarket_fee);
    
    let raw_spread = dec!(0.02); // 2 cents
    let net = raw_spread - fee - polymarket_fee;
    println!("Net spread for 2c raw spread: {}", net);
}

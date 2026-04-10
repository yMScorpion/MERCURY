use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Polymarket,
    Kalshi,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlatformMarketInfo {
    pub platform: Platform,
    pub platform_market_id: String,
}

fn main() {
    let mut map = HashMap::new();
    map.insert(Platform::Polymarket, PlatformMarketInfo {
        platform: Platform::Polymarket,
        platform_market_id: "test".into(),
    });
    
    let json = serde_json::to_string(&map).unwrap();
    println!("JSON: {}", json);
    
    let parsed: Result<HashMap<Platform, PlatformMarketInfo>, _> = serde_json::from_str(&json);
    println!("Parsed: {:?}", parsed.is_ok());
}

fn main() {
    let raw = r#"[{"question":"Test","clobTokenIds":["123","456"]}]"#;
    let val: serde_json::Value = serde_json::from_str(raw).unwrap();
    let item = val.as_array().unwrap().get(0).unwrap();
    let raw_str = item.get("clobTokenIds").and_then(|v| v.as_str());
    println!("as_str: {:?}", raw_str);
    
    let arr = item.get("clobTokenIds").and_then(|v| v.as_array());
    println!("as_array: {:?}", arr);
}

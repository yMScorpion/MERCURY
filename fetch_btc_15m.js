async function run() {
    console.log("=== KALSHI BTC 15m ===");
    let kResp = await fetch("https://api.elections.kalshi.com/trade-api/v2/markets?status=open&series_ticker=KXBTC");
    let kData = await kResp.json();
    if (kData.markets) {
        for (let m of kData.markets) {
            let t = m.title.toLowerCase();
            if (t.includes("15") || t.includes("min")) {
                console.log(`Kalshi: ${m.ticker} | ${m.title} | Exp: ${m.close_time}`);
            }
        }
    }

    console.log("\n=== POLYMARKET BTC 15m ===");
    let cursor = "";
    let found = 0;
    for (let i = 0; i < 20; i++) {
        let url = `https://clob.polymarket.com/markets?active=true&limit=1000${cursor ? '&next_cursor=' + cursor : ''}`;
        let pResp = await fetch(url);
        let pData = await pResp.json();
        for (let m of pData.data || []) {
            let q = (m.question || "").toLowerCase();
            if ((q.includes("btc") || q.includes("bitcoin")) && (q.includes("15") || q.includes("min"))) {
                console.log(`Poly: ${m.question_id} | ${m.question} | Exp: ${m.end_date_iso}`);
                found++;
            }
        }
        cursor = pData.next_cursor;
        if (!cursor || cursor === 'null') break;
    }
    console.log(`Found ${found} Polymarket BTC 15m markets`);
}
run();

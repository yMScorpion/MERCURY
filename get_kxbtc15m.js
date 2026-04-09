async function run() {
    let url = "https://api.elections.kalshi.com/trade-api/v2/markets?status=open&series_ticker=KXBTC15M";
    let resp = await fetch(url);
    let data = await resp.json();
    for (let m of data.markets || []) {
        console.log(m.ticker, m.title);
    }
}
run();

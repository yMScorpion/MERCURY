async function run() {
    let url = "https://api.elections.kalshi.com/trade-api/v2/series";
    let resp = await fetch(url);
    let data = await resp.json();
    for (let s of data.series || []) {
        if (s.ticker.includes("BTC") || (s.title && s.title.includes("Bitcoin"))) {
            console.log(s.ticker, s.title);
        }
    }
}
run();

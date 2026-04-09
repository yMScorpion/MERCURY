async function run() {
    try {
        let resp = await fetch("https://gamma-api.polymarket.com/events?active=true&limit=100");
        let data = await resp.json();
        for (let e of data) {
            if (e.markets) {
                for (let m of e.markets) {
                    let t = (m.question || "").toLowerCase();
                    if (t.includes("btc") && (t.includes("up") || t.includes("down"))) {
                        console.log(m.id, m.question, m.endsAt);
                    }
                }
            }
        }
    } catch(err) {
        console.error(err);
    }
}
run();

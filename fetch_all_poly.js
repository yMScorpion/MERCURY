async function run() {
    let cursor = "";
    let count = 0;
    while (true) {
        let url = "https://clob.polymarket.com/markets?active=true&limit=1000" + (cursor ? "&next_cursor=" + cursor : "");
        let pResp = await fetch(url);
        let pData = await pResp.json();
        for (let m of pData.data || []) {
            count++;
            let q = (m.question || "").toLowerCase();
            if (q.includes("btc") || q.includes("bitcoin") || q.includes("15")) {
                if (q.includes("min") || q.includes("up") || q.includes("down")) {
                    console.log(m.question_id, m.question, m.end_date_iso);
                }
            }
        }
        cursor = pData.next_cursor;
        if (!cursor || cursor === "null") break;
    }
    console.log("Total Polymarket active markets:", count);
}
run();

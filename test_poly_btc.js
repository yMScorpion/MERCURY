const https = require('https');
https.get('https://gamma-api.polymarket.com/markets?active=true&closed=false&acceptingOrders=true&limit=1000', (res) => {
  let data = '';
  res.on('data', chunk => data += chunk);
  res.on('end', () => {
    let json = JSON.parse(data);
    let markets = Array.isArray(json) ? json : json.data;
    let btcMarkets = markets.filter(m => m.question && m.question.toLowerCase().includes('btc'));
    console.log(`Found ${btcMarkets.length} BTC markets`);
    btcMarkets.forEach(m => {
        console.log(`Title: ${m.question}`);
        console.log(`Exp: ${m.endDateIso}`);
    });
  });
});

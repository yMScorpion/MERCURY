const https = require('https');
https.get('https://gamma-api.polymarket.com/markets?active=true&closed=false&acceptingOrders=true&limit=5', (res) => {
  let data = '';
  res.on('data', chunk => data += chunk);
  res.on('end', () => {
    let json = JSON.parse(data);
    let markets = Array.isArray(json) ? json : json.data;
    markets.forEach(m => {
      console.log('---');
      console.log('Question:', m.question);
      console.log('clobTokenIds:', m.clobTokenIds);
      console.log('tokens:', m.tokens);
    });
  });
});

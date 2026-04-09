const https = require('https');
https.get('https://gamma-api.polymarket.com/markets?active=true&closed=false&acceptingOrders=true&limit=1000', (res) => {
  let data = '';
  res.on('data', chunk => data += chunk);
  res.on('end', () => {
    let json = JSON.parse(data);
    let markets = Array.isArray(json) ? json : json.data;
    let withTokens = 0;
    let withStringTokens = 0;
    let withArrayTokens = 0;
    markets.forEach(m => {
      if (m.tokens !== undefined) withTokens++;
      if (typeof m.clobTokenIds === 'string') withStringTokens++;
      if (Array.isArray(m.clobTokenIds)) withArrayTokens++;
    });
    console.log(`Total: ${markets.length}, with tokens: ${withTokens}, clob is string: ${withStringTokens}, clob is array: ${withArrayTokens}`);
    
    // Check a 15 min market
    let fifteen_min = markets.find(m => m.question && m.question.toLowerCase().match(/15.*min|15m|btc.*above/i));
    if (fifteen_min) {
        console.log("Found 15 min-like market:");
        console.log(fifteen_min.question);
        console.log("tokens type:", typeof fifteen_min.tokens);
        console.log("clob type:", typeof fifteen_min.clobTokenIds);
        console.log("clob array:", Array.isArray(fifteen_min.clobTokenIds));
    }
  });
});

import requests
import json
from datetime import datetime

# Kalshi
print("=== KALSHI BTC 15m ===")
url = "https://api.elections.kalshi.com/trade-api/v2/markets?status=open&series_ticker=KXBTC"
headers = {"Accept": "application/json"}
resp = requests.get(url, headers=headers).json()
if 'markets' in resp:
    for m in resp['markets']:
        if '15' in m['title'] or 'minute' in m['title'].lower() or 'min' in m['title'].lower():
            print(f"Kalshi: {m['ticker']} | {m['title']} | Exp: {m.get('close_time')}")

# Polymarket
print("\n=== POLYMARKET BTC 15m ===")
url = "https://clob.polymarket.com/markets"
cursor = ""
found = 0
for i in range(20): # up to 20k markets
    params = {"active": "true", "limit": "1000"}
    if cursor:
        params["next_cursor"] = cursor
    resp = requests.get(url, params=params).json()
    for m in resp.get('data', []):
        q = m.get('question', '')
        if ('BTC' in q or 'Bitcoin' in q) and ('15' in q or 'min' in q.lower()):
            print(f"Poly: {m.get('question_id')} | {q} | Exp: {m.get('end_date_iso')}")
            found += 1
    cursor = resp.get('next_cursor', '')
    if not cursor or cursor == 'null':
        break
print(f"Found {found} Polymarket BTC 15m markets")

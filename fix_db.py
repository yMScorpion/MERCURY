import sqlite3
import json

db = sqlite3.connect("mercury_dry_run.db")
cursor = db.cursor()

# Get all markets
cursor.execute("SELECT unified_id, platforms FROM markets")
rows = cursor.fetchall()

for unified_id, platforms_json in rows:
    platforms = json.loads(platforms_json)
    if "kalshi" in platforms and "polymarket" in platforms:
        # Check if they have the same tick_size and min_order_size
        k = platforms["kalshi"]
        p = platforms["polymarket"]
        print(f"Market {unified_id}: Kalshi={k['platform_market_id']} Poly={p['platform_market_id']}")
        
db.close()

#!/bin/bash
# Resolves platform API IPs and injects them into /etc/hosts for zero-latency DNS.
# Run once after deployment and re-run when IPs change (monitor with cron).
# WARNING: Some cloud platforms rotate IPs. Verify before pinning.
set -euo pipefail

HOSTS_FILE="/etc/hosts"
MARKER="# MERCURY DNS PIN"

# Remove any existing MERCURY pins
sudo sed -i "/$MARKER/d" "$HOSTS_FILE"

resolve_and_pin() {
    local host="$1"
    local ip
    ip=$(dig +short "$host" | head -1)
    if [ -n "$ip" ]; then
        echo "$ip $host $MARKER" | sudo tee -a "$HOSTS_FILE" > /dev/null
        echo "Pinned $host -> $ip"
    else
        echo "WARNING: Could not resolve $host" >&2
    fi
}

resolve_and_pin "trading-api.kalshi.com"
resolve_and_pin "clob.polymarket.com"
resolve_and_pin "ws-subscriptions-clob.polymarket.com"
# Kalshi WS uses the same host as REST (trading-api.kalshi.com) — already pinned above
# Additional external dependencies
resolve_and_pin "api.coingecko.com"
resolve_and_pin "api.binance.com"
resolve_and_pin "api.coinbase.com"
resolve_and_pin "gasstation.polygon.technology"
resolve_and_pin "rpc.ankr.com"

echo "DNS pins written to $HOSTS_FILE"
echo "Run 'cat /etc/hosts | grep MERCURY' to verify."
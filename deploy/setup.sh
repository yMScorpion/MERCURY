#!/bin/bash
set -euo pipefail

echo "=== MERCURY Instance Setup ==="

sudo useradd -r -s /bin/false mercury 2>/dev/null || true
sudo mkdir -p /opt/mercury/{config,data,logs,keys}
sudo chown -R mercury:mercury /opt/mercury
sudo cp target/release/mercury /opt/mercury/mercury
sudo chmod +x /opt/mercury/mercury
sudo cp config/default.yaml /opt/mercury/config/

if [ ! -f /opt/mercury/.env ]; then
    cat <<'ENVEOF' | sudo tee /opt/mercury/.env
RUST_LOG=mercury=info
TELEGRAM_BOT_TOKEN=
TELEGRAM_ALERTS_CHAT_ID=
TELEGRAM_REPORT_CHAT_ID=
POLYMARKET_API_KEY=
POLYMARKET_API_SECRET=
POLYMARKET_API_PASSPHRASE=
KALSHI_API_KEY_ID=
WALLET_PASSPHRASE=
ENVEOF
    sudo chmod 600 /opt/mercury/.env
    sudo chown mercury:mercury /opt/mercury/.env
    echo ">>> Edit /opt/mercury/.env with your credentials"
fi

sudo cp deploy/mercury.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable mercury

echo "=== Setup complete ==="
echo "1. Edit /opt/mercury/.env with your credentials"
echo "2. Start with: sudo systemctl start mercury"
echo "3. View logs: journalctl -u mercury -f"

#!/bin/bash
# ==============================================================================
# setup-vm.sh — Setup Magpie Tunnel & Auto-Sync Daemon on Linux VM
# ==============================================================================
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_USER="${SUDO_USER:-$USER}"
TARGET_HOME=$(eval echo "~$TARGET_USER")

echo "===> 1. Checking / Installing cloudflared..."
if ! command -v cloudflared &>/dev/null; then
    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64) CF_ARCH="amd64" ;;
        aarch64|arm64) CF_ARCH="arm64" ;;
        *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    echo "Downloading cloudflared for $CF_ARCH..."
    curl -sL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-${CF_ARCH}.deb" -o /tmp/cloudflared.deb
    sudo dpkg -i /tmp/cloudflared.deb
    rm /tmp/cloudflared.deb
fi
echo "cloudflared is ready: $(cloudflared --version)"

echo "===> 2. Configuring Tunnel Log File..."
sudo touch /var/log/magpie-tunnel.log
sudo chown "$TARGET_USER:$TARGET_USER" /var/log/magpie-tunnel.log

echo "===> 3. Installing sync script..."
cp "$SCRIPT_DIR/sync-tunnel-to-worker.sh" "$TARGET_HOME/sync-tunnel-to-worker.sh"
chmod +x "$TARGET_HOME/sync-tunnel-to-worker.sh"
chown "$TARGET_USER:$TARGET_USER" "$TARGET_HOME/sync-tunnel-to-worker.sh"

echo "===> 4. Installing systemd services..."
sudo cp "$SCRIPT_DIR/magpie-tunnel.service" /etc/systemd/system/magpie-tunnel.service
sudo cp "$SCRIPT_DIR/magpie-sync.service" /etc/systemd/system/magpie-sync.service

sudo sed -i "s|<YOUR_USER>|$TARGET_USER|g" /etc/systemd/system/magpie-tunnel.service
sudo sed -i "s|<YOUR_USER>|$TARGET_USER|g" /etc/systemd/system/magpie-sync.service
sudo sed -i "s|/home/<YOUR_USER>|$TARGET_HOME|g" /etc/systemd/system/magpie-sync.service

sudo systemctl daemon-reload
sudo systemctl enable --now magpie-tunnel.service
sudo systemctl enable --now magpie-sync.service

echo "===> 5. Verifying Service Status..."
sleep 2
sudo systemctl status magpie-tunnel.service --no-pager | head -n 10
sudo systemctl status magpie-sync.service --no-pager | head -n 10

echo ""
echo "Setup complete! Active quick tunnel and sync daemon are running."
echo "Check logs anytime with:"
echo "  sudo journalctl -u magpie-tunnel -f"
echo "  sudo journalctl -u magpie-sync -f"

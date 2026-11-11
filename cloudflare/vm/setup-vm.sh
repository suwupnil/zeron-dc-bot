#!/usr/bin/env bash
# ==============================================================================
# setup-vm.sh — Setup Magpie Tunnel & Auto-Sync on Linux VM
# ==============================================================================
set -eo pipefail

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
    curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-${CF_ARCH}.deb" -o /tmp/cloudflared.deb
    sudo dpkg -i /tmp/cloudflared.deb
    rm /tmp/cloudflared.deb
fi
echo "cloudflared is ready: $(cloudflared --version)"

echo "===> 2. Installing tunnel sync script..."
cp "$SCRIPT_DIR/sync-tunnel-to-worker.sh" "$TARGET_HOME/sync-tunnel-to-worker.sh"
chmod +x "$TARGET_HOME/sync-tunnel-to-worker.sh"
chown "$TARGET_USER:$TARGET_USER" "$TARGET_HOME/sync-tunnel-to-worker.sh"

echo "===> 3. Installing systemd service..."
sudo cp "$SCRIPT_DIR/magpie-tunnel.service" /etc/systemd/system/magpie-tunnel.service
sudo sed -i "s|<YOUR_USER>|$TARGET_USER|g" /etc/systemd/system/magpie-tunnel.service
sudo sed -i "s|/home/<YOUR_USER>|$TARGET_HOME|g" /etc/systemd/system/magpie-tunnel.service

sudo systemctl daemon-reload
sudo systemctl enable --now magpie-tunnel.service

echo ""
echo "Setup complete! Cloudflare tunnel & worker sync are running."
echo "View live logs with: sudo journalctl -u magpie-tunnel -f"

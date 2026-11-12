#!/usr/bin/env bash
# ==============================================================================
# setup-vm.sh — Setup Magpie & Terminal Tunnels on Linux VM
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

    if command -v dpkg &>/dev/null; then
        echo "Downloading cloudflared deb for ${CF_ARCH}..."
        curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-${CF_ARCH}.deb" -o /tmp/cloudflared.deb
        if ! sudo dpkg -i /tmp/cloudflared.deb 2>/dev/null; then
            echo "Resolving package dependencies..."
            sudo apt-get update -qq && sudo apt-get install -f -y
        fi
        rm -f /tmp/cloudflared.deb
    else
        echo "Downloading standalone cloudflared binary for ${CF_ARCH}..."
        sudo curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-${CF_ARCH}" -o /usr/local/bin/cloudflared
        sudo chmod +x /usr/local/bin/cloudflared
    fi
fi

CLOUDFLARED_VER=$(cloudflared --version 2>/dev/null || /usr/local/bin/cloudflared --version 2>/dev/null || echo "installed")
echo "cloudflared is ready: ${CLOUDFLARED_VER}"

echo "===> 2. Installing scripts & bridge daemon..."
for script in sync-tunnel-to-worker.sh sync-terminal-to-worker.sh terminal-bridge.js; do
    if [ -f "$SCRIPT_DIR/$script" ]; then
        cp "$SCRIPT_DIR/$script" "$TARGET_HOME/$script"
        chmod +x "$TARGET_HOME/$script"
        chown "$TARGET_USER:$TARGET_USER" "$TARGET_HOME/$script"
        echo "Installed: $TARGET_HOME/$script"
    fi
done

echo "===> 3. Installing systemd services..."
for unit in magpie-tunnel.service terminal-tunnel.service terminal-sync.service; do
    if [ -f "$SCRIPT_DIR/$unit" ]; then
        sudo cp "$SCRIPT_DIR/$unit" "/etc/systemd/system/$unit"
        sudo sed -i -e "s|/home/<YOUR_USER>|$TARGET_HOME|g" -e "s|<YOUR_USER>|$TARGET_USER|g" "/etc/systemd/system/$unit"
        echo "Installed service: /etc/systemd/system/$unit"
    fi
done

sudo systemctl daemon-reload

echo "===> 4. Enabling systemd services..."
sudo systemctl enable --now magpie-tunnel.service

if [ -f "$TARGET_HOME/.terminal-bridge.env" ]; then
    sudo systemctl enable --now terminal-tunnel.service
    sudo systemctl enable --now terminal-sync.service
    echo "Started terminal-tunnel.service and terminal-sync.service."
else
    sudo systemctl enable terminal-tunnel.service
    sudo systemctl enable terminal-sync.service
    echo "Notice: Create $TARGET_HOME/.terminal-bridge.env then run:"
    echo "  sudo systemctl start terminal-tunnel.service terminal-sync.service"
fi

echo ""
echo "Setup complete! Cloudflare tunnels & services are configured."
echo "Commands to inspect live logs:"
echo "  sudo journalctl -u magpie-tunnel.service -f"
echo "  sudo journalctl -u terminal-tunnel.service -f"
echo "  sudo journalctl -u terminal-sync.service -f"

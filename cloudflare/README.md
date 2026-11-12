# Cloudflare Worker & Tunnel Bridge for Discord Embedded Activities

This directory contains the production reverse-proxy infrastructure and automation scripts used to host **Magpie Web UI** and **VM Terminal** (`/terminal`) as embedded **Discord Activities** with permanent, zero-maintenance HTTPS URLs.

---

## Architecture Overview

```
Discord Client (Activity Iframe: Magpie Web UI or xterm.js Terminal)
           │
           │  HTTPS GET / POST / WebSocket (Proxied via *.discordsays.com)
           ▼
Cloudflare Worker (Permanent *.workers.dev)
  • Direct Access Guard: Rejects non-Discord internet traffic (403 Forbidden)
  • Discord Proxy Auth: Detects *.discordsays.com proxy headers
  • Optional Ed25519: Cryptographically verifies signatures when DISCORD_PUBLIC_KEY is configured
  • Path & Mode Routing:
      - /terminal/* or /ws  ──> Terminal Bridge (injects Authorization: Bearer <TERMINAL_SECRET>)
      - /*                  ──> Magpie Web UI   (injects session cookie magpie_web_4000)
  • Security Cleanup: Strips X-Frame-Options & CSP headers for seamless iframe embedding
  • Dynamic Resolution: Reads active backend origins from Cloudflare KV (MAGPIE_KV, TERMINAL_KV)
           │
           ├─── HTTPS / WebSocket ────────────────┐
           ▼                                      ▼
Cloudflare Quick Tunnel (Magpie)       Cloudflare Quick Tunnel (Terminal)
           │                                      │
           ▼                                      ▼
Host VM (Local Magpie Daemon)          Host VM (Terminal Bridge Daemon)
  • Magpie Web UI on 127.0.0.1:4000      • Node.js bridge on 127.0.0.1:7681
  • magpie-tunnel.service:               • Inlined xterm.js UI + PTY shell via SSH2
    Pushes tunnel URL & cookie to KV     • terminal-tunnel.service (Node bridge)
                                         • terminal-sync.service (cloudflared tunnel)
```

---

## Directory Structure

```
cloudflare/
├── README.md                      # Architecture and operational deployment guide
├── worker/
│   ├── package.json               # Wrangler deploy scripts and dependencies
│   ├── wrangler.example.jsonc     # Template configuration (KV bindings & fallback vars)
│   └── src/
│       └── index.js               # Reverse-proxy with Magpie & Terminal routing
└── vm/
    ├── setup-vm.sh                # Automated installer for Debian / Ubuntu VMs
    ├── sync-tunnel-to-worker.sh   # Magpie tunnel runner & Worker KV sync (with retries)
    ├── magpie-tunnel.service      # Systemd unit: Magpie cloudflared tunnel
    ├── sync-terminal-to-worker.sh # Terminal tunnel runner & Worker KV sync (with retries)
    ├── terminal-bridge.js         # Node.js WebSocket-to-SSH proxy & xterm.js host
    ├── terminal-tunnel.service    # Systemd unit: Terminal bridge daemon (Node.js)
    ├── terminal-sync.service      # Systemd unit: Terminal cloudflared tunnel
    └── vendor/                    # Offline vendored xterm.js & addon-fit assets
        ├── xterm.js
        ├── xterm.css
        └── addon-fit.js
```

---

## Step 1: Deploy the Cloudflare Worker

1. Navigate to the `worker/` directory:
   ```bash
   cd cloudflare/worker
   ```

2. Copy the template configuration:
   ```bash
   cp wrangler.example.jsonc wrangler.jsonc
   ```

3. Create the KV Namespaces for dynamic origin tracking:
   ```bash
   npx wrangler kv namespace create MAGPIE_KV
   npx wrangler kv namespace create TERMINAL_KV
   ```
   Paste the generated namespace IDs into `wrangler.jsonc` under `kv_namespaces`.

4. Configure your secrets in Cloudflare:
   ```bash
   # Required for Magpie: The session cookie (e.g., magpie_web_4000=<YOUR_KEY>)
   npx wrangler secret put MAGPIE_COOKIE

   # Required: Shared bearer token used by VM sync scripts to push dynamic tunnel URLs
   npx wrangler secret put UPDATE_SECRET

   # Required for Terminal: Shared secret between Worker and Terminal bridge
   npx wrangler secret put TERMINAL_SECRET

   # Optional: Your Discord Application Public Key for strict Ed25519 signature verification
   npx wrangler secret put DISCORD_PUBLIC_KEY
   ```

5. Deploy the Worker:
   ```bash
   npx wrangler deploy
   ```
   Note your assigned URL (e.g., `https://magpie-activity.<subdomain>.workers.dev`).

6. Verify worker health:
   ```bash
   curl -s https://magpie-activity.<subdomain>.workers.dev/__status | jq .
   ```

---

## Step 2: Setup the Host VM

### 1. Prerequisites (Node.js & Dependencies)

The terminal bridge requires Node.js (v18+) and two lightweight npm packages (`ws` and `ssh2`):

```bash
# Install Node.js (if not already installed)
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs

# Install bridge dependencies in the user's home directory
cd ~
npm install ws ssh2
```

### 2. Configure Environment Files

Create `~/.terminal-bridge.env` for Terminal Bridge and Tunnel settings:

```bash
cat << 'EOF' > ~/.terminal-bridge.env
# Shared secret matching Cloudflare Worker's TERMINAL_SECRET
TERMINAL_SECRET=your-random-shared-secret-here

# Shared token matching Cloudflare Worker's UPDATE_SECRET
UPDATE_SECRET=your-update-secret-token

# Cloudflare Worker endpoint for Terminal origin updates
TERMINAL_WORKER_ENDPOINT=https://magpie-activity.<subdomain>.workers.dev/__update_terminal_origin

# Path to VM private SSH key (stays local on VM, never sent over network)
TERMINAL_SSH_KEY=/home/<YOUR_USER>/.ssh/id_ed25519

# SSH username to log in as (defaults to current user if unset)
TERMINAL_SSH_USER=<YOUR_USER>

# Local port to bind bridge to (127.0.0.1 only)
TERMINAL_PORT=7681
EOF
chmod 600 ~/.terminal-bridge.env
```

Ensure the specified `TERMINAL_SSH_KEY` exists and has access to log into `127.0.0.1`:
```bash
# Generate key if you don't already have one
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519 -N ""
cat ~/.ssh/id_ed25519.pub >> ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys
```

Create `~/.magpie-tunnel.env` for Magpie Tunnel settings:
```bash
cat << 'EOF' > ~/.magpie-tunnel.env
WORKER_ENDPOINT=https://magpie-activity.<subdomain>.workers.dev/__update_origin
UPDATE_SECRET=your-update-secret-token
LOCAL_PORT=4000
EOF
chmod 600 ~/.magpie-tunnel.env
```

### 3. Copy VM Files & Run Installer

1. Copy the `cloudflare/vm/` directory to your target Linux VM:
   ```bash
   scp -r cloudflare/vm user@your-vm-ip:~/vm-setup
   ```

2. SSH into your VM and run `setup-vm.sh`:
   ```bash
   ssh user@your-vm-ip
   cd ~/vm-setup
   chmod +x setup-vm.sh sync-tunnel-to-worker.sh sync-terminal-to-worker.sh
   sudo ./setup-vm.sh
   ```
   `setup-vm.sh` installs `cloudflared`, copies scripts and `terminal-bridge.js` to `~`, configures the 3 systemd services, and starts them:
   - `magpie-tunnel.service`: Runs Magpie quick tunnel and syncs origin + cookie.
   - `terminal-tunnel.service`: Runs the Node.js WebSocket-to-SSH bridge on port 7681.
   - `terminal-sync.service`: Runs Terminal quick tunnel and syncs origin to worker KV.

3. Inspect live logs for each service:
   ```bash
   # Terminal bridge logs (WebSocket & SSH events)
   sudo journalctl -u terminal-tunnel.service -f

   # Terminal tunnel sync logs (cloudflared & worker POST responses)
   sudo journalctl -u terminal-sync.service -f

   # Magpie tunnel sync logs
   sudo journalctl -u magpie-tunnel.service -f
   ```

---

## Step 3: Configure Discord Developer Portal

1. Open [Discord Developer Portal](https://discord.com/developers/applications) $\rightarrow$ Your Application $\rightarrow$ **Activities**.
2. Click **Get Started** under Activities (if not already enabled).
3. Under **URL Mappings**, add mappings pointing to your Worker *(without `https://`)*:
   - **Prefix:** `/terminal`
     - **Target:** `<your-worker-subdomain>.workers.dev`
   - **Prefix:** `/`
     - **Target:** `<your-worker-subdomain>.workers.dev`

> **Note on Prefix Stripping:**
> When Discord proxies requests mapped with a prefix like `/terminal`, the iframe request `wss://<app>.discordsays.com/.proxy/terminal/ws` or `/ws` may have the prefix stripped by Discord's edge proxy. Both the Cloudflare Worker and the Terminal Bridge accept both `/terminal/ws` and `/ws` so WebSocket connections establish seamlessly regardless of prefix routing behavior.

4. Run commands in your Discord server:
   - `/magpie` — launches the embedded Magpie Web UI Activity.
   - `/terminal` — launches the embedded VM Terminal Activity.

---

## Troubleshooting & Diagnostics

- **Health Check Status:**
  Query `https://<your-worker>.workers.dev/__status` to check whether `active_backend_origin` and `terminal_backend_origin` are actively populated in KV.
- **Worker Logs:**
  Run `npx wrangler tail` in `cloudflare/worker` to observe live traffic, WebSocket handshakes, and response status codes.
- **Bridge Diagnostics:**
  The terminal client displays visible connection diagnostics (`[Attempt N: wss://... failed (code: X, reason: Y)]`) if an endpoint fails, facilitating fast identification of proxy or network blocks.
- **Service Restarts:**
  To restart all tunnel components on the VM:
  ```bash
  sudo systemctl restart terminal-tunnel.service terminal-sync.service magpie-tunnel.service
  ```

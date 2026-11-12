# Cloudflare Worker & Tunnel Bridge for Discord Embedded Activities

This directory contains the production infrastructure and automation scripts used to host **Magpie Web UI** and **VM Terminal** (`/terminal`) as embedded **Discord Activities** with permanent, zero-maintenance URLs.

---

## Architecture Overview

```
Discord Client (Activity Iframe: Magpie or xterm.js)
           │
           │  HTTPS GET / POST / WebSocket (Proxied via *.discordsays.com)
           ▼
Cloudflare Worker (Permanent *.workers.dev)
  • Direct Access Guard: Unconditionally rejects public internet traffic (403 Forbidden)
  • Discord Proxy Auth: Detects *.discordsays.com proxy routing
  • Optional Ed25519: Cryptographically verifies signatures when DISCORD_PUBLIC_KEY is set
  • Path-Based Routing:
      - /terminal/*  ──> Terminal Bridge (injects Authorization: Bearer <TERMINAL_SECRET>)
      - /*           ──> Magpie Web UI   (injects session cookie magpie_web_4000)
  • Security Cleanup: Strips X-Frame-Options & CSP headers for seamless iframe rendering
  • Dynamic Resolution: Reads active backend origins from Cloudflare KV (MAGPIE_KV, TERMINAL_KV)
           │
           ├─── HTTPS / WebSocket ────────────────┐
           ▼                                      ▼
Cloudflare Quick Tunnel (Magpie)       Cloudflare Quick Tunnel (Terminal)
           │                                      │
           ▼                                      ▼
Host VM (Local Magpie Daemon)          Host VM (Terminal Bridge Daemon)
  • Magpie Web UI on 127.0.0.1:4000      • Node.js bridge on 127.0.0.1:7681
  • magpie-tunnel.service:               • Serves inline xterm.js UI at /terminal/
    Pushes tunnel URL to Worker KV        • Proxies WebSocket to 127.0.0.1:22 via SSH2
                                         • terminal-tunnel.service & sync script
```

---

## Directory Structure

```
cloudflare/
├── README.md                      # Setup and architecture guide
├── worker/
│   ├── package.json               # Wrangler deploy scripts
│   ├── wrangler.example.jsonc     # Template configuration (KV bindings & vars)
│   └── src/
│       └── index.js               # Reverse-proxy with Magpie & Terminal routing
└── vm/
    ├── setup-vm.sh                # Automated installer for Debian / Ubuntu VMs
    ├── sync-tunnel-to-worker.sh   # Magpie tunnel runner & Worker KV sync
    ├── magpie-tunnel.service      # Systemd unit for Magpie tunnel
    ├── sync-terminal-to-worker.sh # Terminal bridge tunnel runner & Worker KV sync
    ├── terminal-bridge.js         # Node.js WebSocket-to-SSH proxy & xterm.js host
    └── terminal-tunnel.service    # Systemd unit for Terminal bridge
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

   # Required: Shared bearer token used by VM sync scripts to push tunnel URLs
   npx wrangler secret put UPDATE_SECRET

   # Required for Terminal: Shared secret between Worker and Terminal bridge
   npx wrangler secret put TERMINAL_SECRET

   # Optional: Your Discord Application Public Key for strict Ed25519 cryptographic verification
   npx wrangler secret put DISCORD_PUBLIC_KEY
   ```

5. Deploy the Worker:
   ```bash
   npx wrangler deploy
   ```
   Note your assigned URL (e.g., `https://magpie-activity.<subdomain>.workers.dev`).

---

## Step 2: Setup the Host VM

### 1. Prerequisites (Node.js & Dependencies)

The terminal bridge requires Node.js (v18+) and two lightweight npm packages (`ws` and `ssh2`):

```bash
# Install Node.js (if not already installed)
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs

# Install bridge dependencies in the home directory
cd ~
npm install ws ssh2
```

### 2. Configure `.terminal-bridge.env`

Create `~/.terminal-bridge.env` to configure the terminal bridge daemon:

```bash
cat << 'EOF' > ~/.terminal-bridge.env
# Shared secret matching Cloudflare Worker's TERMINAL_SECRET
TERMINAL_SECRET=your-random-shared-secret-here

# Path to VM private SSH key (key stays local on VM, never sent over network)
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

### 3. Copy VM Files & Configure Sync Scripts

1. Copy the `cloudflare/vm/` directory to your target Linux VM:
   ```bash
   scp -r cloudflare/vm user@your-vm-ip:~/vm-setup
   ```

2. SSH into your VM:
   ```bash
   ssh user@your-vm-ip
   cd ~/vm-setup
   chmod +x setup-vm.sh sync-tunnel-to-worker.sh sync-terminal-to-worker.sh
   ```

3. Set your Worker endpoints and update secret in both sync scripts:
   - In `sync-tunnel-to-worker.sh`:
     ```bash
     WORKER_ENDPOINT="https://<YOUR_WORKER_SUBDOMAIN>.workers.dev/__update_origin"
     UPDATE_SECRET="<YOUR_UPDATE_SECRET>"
     ```
   - In `sync-terminal-to-worker.sh`:
     ```bash
     TERMINAL_WORKER_ENDPOINT="https://<YOUR_WORKER_SUBDOMAIN>.workers.dev/__update_terminal_origin"
     UPDATE_SECRET="<YOUR_UPDATE_SECRET>"
     ```

4. Run the installer:
   ```bash
   sudo ./setup-vm.sh
   ```
   `setup-vm.sh` installs `cloudflared`, copies the scripts and `terminal-bridge.js` to `~`, configures `magpie-tunnel.service` and `terminal-tunnel.service`, and starts them with systemd.

5. View live service logs anytime:
   ```bash
   # Terminal bridge logs
   sudo journalctl -u terminal-tunnel -f

   # Magpie tunnel logs
   sudo journalctl -u magpie-tunnel -f
   ```

---

## Step 3: Configure Discord Developer Portal

1. Open [Discord Developer Portal](https://discord.com/developers/applications) $\rightarrow$ Your Application $\rightarrow$ **Activities**.
2. Click **Get Started** under Activities (if not already enabled).
3. Under **URL Mappings**, add mappings pointing to your Worker *(without `https://`)*:
   - For Terminal (add specific prefix first):
     - **Prefix:** `/terminal`
     - **Target:** `<your-worker-subdomain>.workers.dev`
   - For Magpie:
     - **Prefix:** `/`
     - **Target:** `<your-worker-subdomain>.workers.dev`
4. Run commands in your Discord server:
   - `/magpie` — launches the embedded Magpie Web UI Activity.
   - `/terminal` — launches the embedded browser terminal connected to the VM shell.

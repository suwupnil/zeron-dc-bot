# Cloudflare Worker & Tunnel Bridge for Discord Embedded Activities

This directory contains the production infrastructure and automation scripts used to host **Magpie Web UI** as an embedded **Discord Activity** with a permanent, zero-maintenance URL.

---

## Architecture Overview

```
Discord Client (Activity Iframe)
           │
           │  HTTPS GET / POST (Proxied via *.discordsays.com)
           ▼
Cloudflare Worker (Permanent *.workers.dev)
  • Direct Access Guard: Unconditionally rejects public internet traffic (403 Forbidden)
  • Discord Proxy Auth: Detects *.discordsays.com proxy routing
  • Optional Ed25519: Cryptographically verifies signatures when DISCORD_PUBLIC_KEY is set
  • Auto-Authentication: Injects Magpie session cookie (magpie_web_4000)
  • Security Cleanup: Strips X-Frame-Options & CSP headers for seamless iframe rendering
  • Dynamic Resolution: Reads active backend origin from Cloudflare KV
           │
           │  HTTPS
           ▼
Cloudflare Quick Tunnel (trycloudflare.com)
           │
           ▼
Host VM (Local Magpie Daemon)
  • Magpie Web UI on 127.0.0.1:4000
  • magpie-tunnel.service: Streams cloudflared output and pushes URLs directly to Worker KV
```

---

## Directory Structure

```
cloudflare/
├── README.md                      # Setup and architecture guide
├── worker/
│   ├── package.json               # Wrangler deploy scripts
│   ├── wrangler.example.jsonc     # Template configuration (KV binding & vars)
│   └── src/
│       └── index.js               # Clean, high-performance Worker reverse-proxy
└── vm/
    ├── setup-vm.sh                # Automated installer for Debian / Ubuntu VMs
    ├── sync-tunnel-to-worker.sh   # Live streaming tunnel runner & Worker KV sync
    └── magpie-tunnel.service      # Single systemd unit managing tunnel & sync
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

3. Create the KV Namespace for dynamic origin tracking:
   ```bash
   npx wrangler kv namespace create MAGPIE_KV
   ```
   Paste the generated namespace ID into `wrangler.jsonc` under `kv_namespaces[0].id`.

4. Configure your secrets in Cloudflare:
   ```bash
   # Required: The Magpie web key cookie (e.g., magpie_web_4000=<YOUR_KEY>)
   npx wrangler secret put MAGPIE_COOKIE

   # Required: A secret bearer token used by the VM to push tunnel URLs
   npx wrangler secret put UPDATE_SECRET

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

1. Copy the `cloudflare/vm/` directory to your target Linux VM:
   ```bash
   scp -r cloudflare/vm user@your-vm-ip:~/vm-setup
   ```

2. SSH into your VM:
   ```bash
   ssh user@your-vm-ip
   cd ~/vm-setup
   chmod +x setup-vm.sh sync-tunnel-to-worker.sh
   ```

3. Set your Worker endpoint and update secret in `sync-tunnel-to-worker.sh`:
   ```bash
   nano sync-tunnel-to-worker.sh
   # Set WORKER_ENDPOINT and UPDATE_SECRET matching what you deployed in Step 1
   ```

4. Run the installer:
   ```bash
   sudo ./setup-vm.sh
   ```

5. View live tunnel logs anytime:
   ```bash
   sudo journalctl -u magpie-tunnel -f
   ```

---

## Step 3: Configure Discord Developer Portal

1. Open [Discord Developer Portal](https://discord.com/developers/applications) $\rightarrow$ Your Application $\rightarrow$ **Activities**.
2. Click **Get Started** under Activities (if not already enabled).
3. Under **URL Mappings**:
   - **Prefix:** `/`
   - **Target:** `<your-worker-subdomain>.workers.dev` *(do NOT include `https://`)*
4. Run `/magpie` in your Discord server to launch the Web UI.

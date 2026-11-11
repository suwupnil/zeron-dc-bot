# Cloudflare Worker & Tunnel Bridge for Discord Embedded Activities

This directory contains the production infrastructure and automation scripts used to host **Magpie Web UI** as an embedded **Discord Activity** with a permanent, zero-maintenance URL.

---

## Architecture Overview

```
Discord Client (Activity Iframe)
           │
           │  HTTPS GET / POST (Signed with Ed25519)
           ▼
Cloudflare Worker (Permanent *.workers.dev)
  • Cryptographic Verification: Validates X-Signature-Ed25519 & X-Signature-Timestamp
  • Dynamic Resolution: Reads active tunnel endpoint from Cloudflare KV
  • Auto-Authentication: Injects Magpie session cookie (magpie_web_4000)
  • Security Cleanup: Strips X-Frame-Options & CSP headers for iframe embedding
           │
           │  HTTPS
           ▼
Cloudflare Quick Tunnel (trycloudflare.com)
           │
           ▼
Host VM (Local Magpie Daemon)
  • Magpie Web UI on 127.0.0.1:4000
  • Auto-Sync Daemon: Pushes new tunnel endpoints to Worker KV on restart
```

---

## Directory Structure

```
cloudflare/
├── README.md                      # This setup and architecture guide
├── worker/
│   ├── package.json               # Wrangler deploy scripts
│   ├── wrangler.jsonc             # Worker configuration, KV binding, and vars
│   └── src/
│       └── index.js               # Production Worker reverse-proxy & Ed25519 verifier
└── vm/
    ├── setup-vm.sh                # Automated installer for Debian / Ubuntu VMs
    ├── sync-tunnel-to-worker.sh   # Background daemon syncing tunnel URL to Worker KV
    ├── magpie-tunnel.service      # Systemd unit for cloudflared quick tunnel
    ├── magpie-sync.service        # Systemd unit for the sync daemon
    └── apache-proxy.conf          # Optional local Apache proxy with cookie injection
```

---

## Step 1: Deploy the Cloudflare Worker

1. Navigate to the `worker/` directory:
   ```bash
   cd cloudflare/worker
   ```

2. Log in to Cloudflare with Wrangler:
   ```bash
   npx wrangler login
   ```

3. Create the KV Namespace for dynamic origin tracking:
   ```bash
   npx wrangler kv namespace create MAGPIE_KV
   ```
   Copy the generated namespace ID and paste it into `wrangler.jsonc` under `kv_namespaces[0].id`.

4. Deploy the Worker:
   ```bash
   npx wrangler deploy
   ```
   Note the assigned permanent domain (e.g., `https://magpie-activity.<your-subdomain>.workers.dev`).

---

## Step 2: Setup the Host VM

1. Copy the `cloudflare/vm/` directory to your target Linux VM:
   ```bash
   scp -r cloudflare/vm user@your-vm-ip:~/vm-setup
   ```

2. SSH into your VM and run the automated installer:
   ```bash
   ssh user@your-vm-ip
   cd ~/vm-setup
   chmod +x setup-vm.sh sync-tunnel-to-worker.sh
   sudo ./setup-vm.sh
   ```

3. Check service health:
   ```bash
   sudo systemctl status magpie-tunnel.service
   sudo systemctl status magpie-sync.service
   ```

The sync daemon will automatically read `/var/log/magpie-tunnel.log` and POST active endpoints to `https://<your-worker>/__update_origin`.

---

## Step 3: Configure Discord Developer Portal

1. Open the [Discord Developer Portal](https://discord.com/developers/applications).
2. Select your Application $\rightarrow$ **Activities**.
3. Under **URL Mappings**:
   - **Prefix / Path:** `/`
   - **Target:** `magpie-activity.<your-subdomain>.workers.dev` *(your Worker's hostname)*
4. Click **Save Changes**.

---

## Step 4: Strict Cryptographic Ed25519 Verification (Optional)

To mathematically ensure that **only** Discord's edge proxy can communicate with your Worker:

1. Copy your application's **Public Key** from the [Developer Portal General Information](https://discord.com/developers/applications) tab (a 64-character hex string).
2. Set it as a secret on your Worker:
   ```bash
   cd cloudflare/worker
   npx wrangler secret put DISCORD_PUBLIC_KEY
   ```
3. Once set, the Worker enforces `X-Signature-Ed25519` on all requests. Direct browser requests from outside Discord will be rejected with `401 Unauthorized`.
4. Verify anytime at `https://<your-worker>/__status` — `"ed25519_verification_enforced": true`.

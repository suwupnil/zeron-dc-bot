#!/bin/bash
# ==============================================================================
# sync-tunnel-to-worker.sh
# 
# Monitors the Cloudflare quick tunnel log for active/new trycloudflare.com URLs
# and automatically updates the Cloudflare Worker's KV storage via the protected
# POST /__update_origin endpoint.
# ==============================================================================

LOG_FILE="${TUNNEL_LOG_FILE:-/var/log/magpie-tunnel.log}"
WORKER_ENDPOINT="${WORKER_ENDPOINT:-https://<YOUR_WORKER_SUBDOMAIN>.workers.dev/__update_origin}"
UPDATE_SECRET="${UPDATE_SECRET:-<YOUR_UPDATE_SECRET>}"
CHECK_INTERVAL="${CHECK_INTERVAL:-10}"

if [[ "$WORKER_ENDPOINT" == *"YOUR_WORKER_SUBDOMAIN"* ]] || [[ "$UPDATE_SECRET" == *"<YOUR_UPDATE_SECRET>"* ]]; then
    echo "Error: WORKER_ENDPOINT and UPDATE_SECRET must be configured before running."
    echo "Example: export WORKER_ENDPOINT='https://my-worker.my-subdomain.workers.dev/__update_origin'"
    echo "         export UPDATE_SECRET='my-secret-token'"
    exit 1
fi

LAST_URL=""

echo "[$(date)] Starting Magpie Tunnel -> Cloudflare Worker sync daemon..."
echo "[$(date)] Watching: $LOG_FILE"
echo "[$(date)] Target Worker Endpoint: $WORKER_ENDPOINT"

while true; do
    if [ -f "$LOG_FILE" ]; then
        CURRENT_URL=$(grep -o 'https://[a-zA-Z0-9.-]*\.trycloudflare\.com' "$LOG_FILE" 2>/dev/null | tail -n 1)
        if [ -n "$CURRENT_URL" ] && [ "$CURRENT_URL" != "$LAST_URL" ]; then
            echo "[$(date)] New tunnel URL detected: $CURRENT_URL"
            RES=$(curl -s -X POST "$WORKER_ENDPOINT" \
                -H "Authorization: Bearer $UPDATE_SECRET" \
                -H "Content-Type: application/json" \
                -d "{\"origin\":\"$CURRENT_URL\"}")
            echo "[$(date)] Worker sync response: $RES"
            LAST_URL="$CURRENT_URL"
        fi
    fi
    sleep "$CHECK_INTERVAL"
done

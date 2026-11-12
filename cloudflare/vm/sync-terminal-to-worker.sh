#!/usr/bin/env bash
# ==============================================================================
# sync-terminal-to-worker.sh
#
# Runs cloudflared quick tunnel to local Terminal Bridge (port 7681), streams its
# output in real time, and immediately POSTs the assigned trycloudflare.com URL
# to the Cloudflare Worker at /__update_terminal_origin.
# ==============================================================================
set -eo pipefail

if [ -f "${HOME}/.terminal-bridge.env" ]; then
    # shellcheck disable=SC1091
    source "${HOME}/.terminal-bridge.env"
fi

WORKER_ENDPOINT="${TERMINAL_WORKER_ENDPOINT:-https://<YOUR_WORKER_SUBDOMAIN>.workers.dev/__update_terminal_origin}"
UPDATE_SECRET="${UPDATE_SECRET:-<YOUR_UPDATE_SECRET>}"
LOCAL_PORT="${TERMINAL_PORT:-7681}"

if [[ "$WORKER_ENDPOINT" == *"YOUR_WORKER_SUBDOMAIN"* ]] || [[ "$UPDATE_SECRET" == *"<YOUR_UPDATE_SECRET>"* ]]; then
    echo "Error: TERMINAL_WORKER_ENDPOINT and UPDATE_SECRET must be configured before running."
    echo "Example: export TERMINAL_WORKER_ENDPOINT='https://my-worker.workers.dev/__update_terminal_origin'"
    echo "         export UPDATE_SECRET='my-secret-token'"
    exit 1
fi

CLOUDFLARED_BIN="$(command -v cloudflared 2>/dev/null || echo '/usr/bin/cloudflared')"

echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Starting cloudflared tunnel to http://127.0.0.1:${LOCAL_PORT} using ${CLOUDFLARED_BIN}..."

# Execute cloudflared and stream output line-by-line in real time
"${CLOUDFLARED_BIN}" tunnel --url "http://127.0.0.1:${LOCAL_PORT}" 2>&1 | while IFS= read -r line; do
    echo "$line"
    if [[ "$line" =~ (https://[a-zA-Z0-9.-]+\.trycloudflare\.com) ]]; then
        TUNNEL_URL="${BASH_REMATCH[1]}"
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Detected terminal tunnel URL: ${TUNNEL_URL}"

        PAYLOAD="{\"origin\":\"${TUNNEL_URL}\"}"

        RESPONSE=$(curl -fsS -X POST "${WORKER_ENDPOINT}" \
            -H "Authorization: Bearer ${UPDATE_SECRET}" \
            -H "Content-Type: application/json" \
            -d "${PAYLOAD}" 2>&1 || echo "Sync failed: $?")
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Worker sync response: ${RESPONSE}"
    fi
done

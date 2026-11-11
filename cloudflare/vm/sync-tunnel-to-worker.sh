#!/usr/bin/env bash
# ==============================================================================
# sync-tunnel-to-worker.sh
#
# Runs cloudflared quick tunnel to local Magpie (port 4000), streams its output
# in real time, and immediately POSTs the assigned trycloudflare.com URL and
# the active Magpie authentication cookie to the Cloudflare Worker.
# ==============================================================================
set -eo pipefail

WORKER_ENDPOINT="${WORKER_ENDPOINT:-https://<YOUR_WORKER_SUBDOMAIN>.workers.dev/__update_origin}"
UPDATE_SECRET="${UPDATE_SECRET:-<YOUR_UPDATE_SECRET>}"
LOCAL_PORT="${LOCAL_PORT:-4000}"

if [[ "$WORKER_ENDPOINT" == *"YOUR_WORKER_SUBDOMAIN"* ]] || [[ "$UPDATE_SECRET" == *"<YOUR_UPDATE_SECRET>"* ]]; then
    echo "Error: WORKER_ENDPOINT and UPDATE_SECRET must be configured before running."
    echo "Example: export WORKER_ENDPOINT='https://my-worker.workers.dev/__update_origin'"
    echo "         export UPDATE_SECRET='my-secret-token'"
    exit 1
fi

# Detect active MAGPIE_WEB_KEY from environment or systemd user service
get_magpie_cookie() {
    local key="$MAGPIE_WEB_KEY"
    if [ -z "$key" ] && [ -f "${HOME}/.config/systemd/user/magpie.service" ]; then
        key=$(sed -n 's/.*MAGPIE_WEB_KEY=\([^" ]*\).*/\1/p' "${HOME}/.config/systemd/user/magpie.service" 2>/dev/null || true)
    fi
    if [ -n "$key" ]; then
        echo "magpie_web_${LOCAL_PORT}=${key}"
    fi
}

echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Starting cloudflared tunnel to http://127.0.0.1:${LOCAL_PORT}..."

# Execute cloudflared and stream output line-by-line in real time
/usr/bin/cloudflared tunnel --url "http://127.0.0.1:${LOCAL_PORT}" 2>&1 | while IFS= read -r line; do
    echo "$line"
    if [[ "$line" =~ (https://[a-zA-Z0-9.-]+\.trycloudflare\.com) ]]; then
        TUNNEL_URL="${BASH_REMATCH[1]}"
        COOKIE=$(get_magpie_cookie)
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Detected tunnel URL: ${TUNNEL_URL} (auto-sync cookie: $([ -n "$COOKIE" ] && echo "yes" || echo "no"))"

        if [ -n "$COOKIE" ]; then
            PAYLOAD="{\"origin\":\"${TUNNEL_URL}\",\"cookie\":\"${COOKIE}\"}"
        else
            PAYLOAD="{\"origin\":\"${TUNNEL_URL}\"}"
        fi

        RESPONSE=$(curl -fsS -X POST "${WORKER_ENDPOINT}" \
            -H "Authorization: Bearer ${UPDATE_SECRET}" \
            -H "Content-Type: application/json" \
            -d "${PAYLOAD}" 2>&1 || echo "Sync failed: $?")
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Worker sync response: ${RESPONSE}"
    fi
done

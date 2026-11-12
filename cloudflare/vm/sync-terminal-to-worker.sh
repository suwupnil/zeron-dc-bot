#!/usr/bin/env bash
# ==============================================================================
# sync-terminal-to-worker.sh
#
# Runs cloudflared quick tunnel to local Terminal Bridge (port 7681), streams its
# output in real time, and immediately POSTs the assigned trycloudflare.com URL
# to the Cloudflare Worker at /__update_terminal_origin with retry resilience.
# ==============================================================================
set -eo pipefail

# Source configuration from user environment if present
if [ -f "${HOME}/.terminal-bridge.env" ]; then
    # shellcheck disable=SC1091
    source "${HOME}/.terminal-bridge.env"
elif [ -f "/etc/terminal-bridge.env" ]; then
    # shellcheck disable=SC1091
    source "/etc/terminal-bridge.env"
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

# Locate cloudflared binary across standard installation paths
CLOUDFLARED_BIN="$(command -v cloudflared 2>/dev/null || true)"
if [ -z "$CLOUDFLARED_BIN" ] || [ ! -x "$CLOUDFLARED_BIN" ]; then
    if [ -x "/usr/local/bin/cloudflared" ]; then
        CLOUDFLARED_BIN="/usr/local/bin/cloudflared"
    elif [ -x "/usr/bin/cloudflared" ]; then
        CLOUDFLARED_BIN="/usr/bin/cloudflared"
    else
        echo "Error: cloudflared binary not found in PATH, /usr/local/bin, or /usr/bin."
        exit 1
    fi
fi

# Robust worker synchronization with exponential backoff retries
sync_to_worker() {
    local payload="$1"
    local max_retries=5
    local delay=2
    local attempt=1

    while [ "$attempt" -le "$max_retries" ]; do
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Posting terminal origin to worker (attempt ${attempt}/${max_retries})..."
        local response
        if response=$(curl -fsS -m 10 -X POST "${WORKER_ENDPOINT}" \
            -H "Authorization: Bearer ${UPDATE_SECRET}" \
            -H "Content-Type: application/json" \
            -d "${payload}" 2>&1); then
            echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Worker sync successful: ${response}"
            return 0
        else
            echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Sync attempt ${attempt} failed: ${response}"
            if [ "$attempt" -lt "$max_retries" ]; then
                sleep "$delay"
                delay=$((delay * 2))
            fi
            attempt=$((attempt + 1))
        fi
    done

    echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] WARNING: Failed to sync terminal origin after ${max_retries} attempts."
    return 1
}

echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Starting cloudflared tunnel to http://127.0.0.1:${LOCAL_PORT} using ${CLOUDFLARED_BIN}..."

# Execute cloudflared and stream output line-by-line in real time
"${CLOUDFLARED_BIN}" tunnel --url "http://127.0.0.1:${LOCAL_PORT}" 2>&1 | while IFS= read -r line; do
    echo "$line"
    if [[ "$line" =~ (https://[a-zA-Z0-9.-]+\.trycloudflare\.com) ]]; then
        TUNNEL_URL="${BASH_REMATCH[1]}"
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] Detected terminal tunnel URL: ${TUNNEL_URL}"

        PAYLOAD="{\"origin\":\"${TUNNEL_URL}\"}"
        sync_to_worker "${PAYLOAD}" || true
    fi
done

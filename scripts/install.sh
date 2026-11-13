#!/usr/bin/env bash
# ==============================================================================
# zeron-dc-bot — Automated Linux/macOS VM Installer
# ==============================================================================
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/suwupnil/zeron-dc-bot/main/scripts/install.sh | bash
# Or run locally:
#   ./scripts/install.sh
# ==============================================================================

set -eo pipefail

# Text formatting
BOLD="\033[1m"
GREEN="\033[0;32m"
CYAN="\033[0;36m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
MAGENTA="\033[0;35m"
RESET="\033[0m"

REPO_OWNER="suwupnil"
REPO_NAME="zeron-dc-bot"

# Read user input handling both direct terminal and piped curl execution
prompt_read() {
    local prompt_msg="$1"
    local var_name="$2"
    local default_val="$3"
    local input=""

    if [ "${NON_INTERACTIVE:-0}" = "1" ]; then
        # Automated non-interactive mode: read directly from stdin
        printf "%b" "$prompt_msg"
        read -r input || input=""
    elif [ -t 0 ]; then
        # stdin is directly attached to a terminal
        printf "%b" "$prompt_msg"
        read -r input || input=""
    elif [ -c /dev/tty ]; then
        # stdin is piped (e.g. curl ... | bash), read from /dev/tty
        printf "%b" "$prompt_msg" > /dev/tty
        read -r input < /dev/tty || input=""
    else
        # Fallback
        printf "%b" "$prompt_msg"
        read -r input || input=""
    fi

    # Strip leading/trailing whitespace and carriage returns
    input="$(echo "$input" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' | tr -d '\r')"

    if [ -z "$input" ] && [ -n "$default_val" ]; then
        input="$default_val"
    fi

    eval "$var_name=\"\$input\""
}

prompt_secret() {
    local prompt_msg="$1"
    local var_name="$2"
    local input=""

    if [ "${NON_INTERACTIVE:-0}" = "1" ]; then
        printf "%b" "$prompt_msg"
        read -r input || input=""
    elif [ -t 0 ]; then
        printf "%b" "$prompt_msg"
        read -r -s input || input=""
        echo ""
    elif [ -c /dev/tty ]; then
        printf "%b" "$prompt_msg" > /dev/tty
        read -r -s input < /dev/tty || input=""
        echo "" > /dev/tty
    else
        printf "%b" "$prompt_msg"
        read -r input || input=""
    fi

    # Strip leading/trailing whitespace and carriage returns
    input="$(echo "$input" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' | tr -d '\r')"

    eval "$var_name=\"\$input\""
}

# Elevation helper
run_as_root() {
    if [ "$EUID" -eq 0 ]; then
        "$@"
    else
        if command -v sudo &>/dev/null; then
            sudo "$@"
        else
            printf "%bError: 'sudo' is required to execute: %s%b\n" "$RED" "$*" "$RESET" >&2
            exit 1
        fi
    fi
}

echo ""
printf "%b" "${CYAN}${BOLD}"
cat << 'EOF'
  _____                         _        _           _   
 |__  /___ _ __ ___  _ __      | |      | |         | |  
   / // _ \ '__/ _ \| '_ \  ___| |___ __| |___  ___ | |_ 
  / /|  __/ | | (_) | | | |/ __| / __/ _` | __|/ _ \| __|
 /____\___|_|  \___/|_| |_|\___|_\___\__,_\__| \___/ \__|
EOF
printf "%b" "${RESET}"
printf "%b\n" "${BOLD}   Discord Bridge Bot for Zeron Multi-Agent Engine${RESET}"
printf "%b\n\n" "${CYAN}================================================================${RESET}"

# 1. System & Architecture Detection
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64|amd64) TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
            *)
                printf "%bError: Unsupported Linux architecture: %s%b\n" "$RED" "$ARCH" "$RESET" >&2
                exit 1
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64) TARGET="x86_64-apple-darwin" ;;
            arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
            *)
                printf "%bError: Unsupported macOS architecture: %s%b\n" "$RED" "$ARCH" "$RESET" >&2
                exit 1
                ;;
        esac
        ;;
    *)
        printf "%bError: Unsupported operating system: %s%b\n" "$RED" "$OS" "$RESET" >&2
        exit 1
        ;;
esac

printf "%b✔ Detected Platform:%b %s (%s)\n" "$GREEN" "$RESET" "$OS" "$TARGET"

# Check required utilities
for cmd in curl tar; do
    if ! command -v "$cmd" &>/dev/null; then
        printf "%bMissing required tool: %s. Attempting to install...%b\n" "$YELLOW" "$cmd" "$RESET"
        if command -v apt-get &>/dev/null; then
            run_as_root apt-get update -y && run_as_root apt-get install -y "$cmd"
        elif command -v dnf &>/dev/null; then
            run_as_root dnf install -y "$cmd"
        elif command -v yum &>/dev/null; then
            run_as_root yum install -y "$cmd"
        elif command -v pacman &>/dev/null; then
            run_as_root pacman -Sy --noconfirm "$cmd"
        else
            printf "%bError: Please install '%s' and rerun the installer.%b\n" "$RED" "$cmd" "$RESET" >&2
            exit 1
        fi
    fi
done

# 2. Determine Installation Targets
TARGET_USER="${SUDO_USER:-$USER}"
TARGET_HOME=$(eval echo "~$TARGET_USER")
INSTALL_DIR="${INSTALL_DIR:-$TARGET_HOME/zeron-dc-bot}"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"

printf "%b✔ Install Directory:%b %s\n" "$GREEN" "$RESET" "$INSTALL_DIR"
printf "%b✔ Binary Location:%b   %s/zeron-dc-bot\n\n" "$GREEN" "$RESET" "$BIN_DIR"

# 3. Download and Install Prebuilt Binary from GitHub Actions
printf "%b===> Step 1: Downloading prebuilt binary from GitHub...%b\n" "$CYAN" "$RESET"

if [ "${SKIP_DOWNLOAD:-0}" = "1" ] || [ "${1:-}" = "--skip-download" ]; then
    printf "%bSkipping binary download (SKIP_DOWNLOAD=1 / --skip-download).%b\n\n" "$YELLOW" "$RESET"
else
    TMP_DIR="$(mktemp -d /tmp/zeron-dc-bot-install.XXXXXX)"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    VERSION="${VERSION:-latest}"
    if [ "$VERSION" = "latest" ]; then
        DOWNLOAD_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/latest/download/zeron-dc-bot-${TARGET}.tar.gz"
        API_URL="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    else
        DOWNLOAD_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${VERSION}/zeron-dc-bot-${TARGET}.tar.gz"
        API_URL="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/${VERSION}"
    fi
    printf "Fetching: %b%s%b\n" "$CYAN" "$DOWNLOAD_URL" "$RESET"

    DOWNLOAD_SUCCESS=false
    if curl -fSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_DIR/zeron-dc-bot.tar.gz"; then
        DOWNLOAD_SUCCESS=true
    else
        # Fallback 1: query GitHub Releases API for specified version/tag
        printf "%bDirect download link not ready yet, checking release assets via GitHub API...%b\n" "$YELLOW" "$RESET"
        LATEST_JSON=$(curl -sSL "$API_URL" 2>/dev/null || true)
        ASSET_URL=$(echo "$LATEST_JSON" | grep "browser_download_url" | grep "${TARGET}.tar.gz" | head -n 1 | cut -d '"' -f 4 || true)

        if [ -n "$ASSET_URL" ]; then
            printf "Found asset: %s\n" "$ASSET_URL"
            if curl -fSL --progress-bar "$ASSET_URL" -o "$TMP_DIR/zeron-dc-bot.tar.gz"; then
                DOWNLOAD_SUCCESS=true
            fi
        fi

        # Fallback 2: search full releases list if tag pointer is not yet assigned
        if [ "$DOWNLOAD_SUCCESS" = false ]; then
            RELEASES_LIST=$(curl -sSL "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases" 2>/dev/null || true)
            ASSET_URL=$(echo "$RELEASES_LIST" | grep "browser_download_url" | grep "${TARGET}.tar.gz" | head -n 1 | cut -d '"' -f 4 || true)
            if [ -n "$ASSET_URL" ]; then
                printf "Found asset from releases list: %s\n" "$ASSET_URL"
                if curl -fSL --progress-bar "$ASSET_URL" -o "$TMP_DIR/zeron-dc-bot.tar.gz"; then
                    DOWNLOAD_SUCCESS=true
                fi
            fi
        fi
    fi

    if [ "$DOWNLOAD_SUCCESS" = false ]; then
        printf "\n%bError: Could not download prebuilt release binary for %s.%b\n" "$RED" "$TARGET" "$RESET" >&2
        printf "%bPlease verify that GitHub Actions has built and published releases for %s/%s.%b\n" "$YELLOW" "$REPO_OWNER" "$REPO_NAME" "$RESET" >&2
        exit 1
    fi

    tar -xzf "$TMP_DIR/zeron-dc-bot.tar.gz" -C "$TMP_DIR"

    if [ ! -f "$TMP_DIR/zeron-dc-bot" ]; then
        printf "%bError: Binary 'zeron-dc-bot' not found in downloaded archive.%b\n" "$RED" "$RESET" >&2
        exit 1
    fi

    chmod +x "$TMP_DIR/zeron-dc-bot"

    if [ -w "$BIN_DIR" ] || (! [ -e "$BIN_DIR" ] && [ -w "$(dirname "$BIN_DIR")" ]); then
        mkdir -p "$BIN_DIR"
        cp "$TMP_DIR/zeron-dc-bot" "$BIN_DIR/zeron-dc-bot"
        chmod +x "$BIN_DIR/zeron-dc-bot"
    else
        run_as_root mkdir -p "$BIN_DIR"
        run_as_root cp "$TMP_DIR/zeron-dc-bot" "$BIN_DIR/zeron-dc-bot"
        run_as_root chmod +x "$BIN_DIR/zeron-dc-bot"
    fi

    printf "%b✔ Binary successfully installed to %s/zeron-dc-bot%b\n\n" "$GREEN" "$BIN_DIR" "$RESET"
fi

# 4. Interactive Configuration Wizard (.env)
printf "%b===> Step 2: Environment Configuration (.env)%b\n" "$CYAN" "$RESET"

mkdir -p "$INSTALL_DIR"
chown "$TARGET_USER:$TARGET_USER" "$INSTALL_DIR" 2>/dev/null || true

ENV_FILE="$INSTALL_DIR/.env"
RECONFIGURE=true

if [ -f "$ENV_FILE" ]; then
    printf "%bExisting configuration found at %s%b\n" "$YELLOW" "$ENV_FILE" "$RESET"
    prompt_read "Do you want to reconfigure your .env settings? [y/N]: " KEEP_EXISTING "n"
    if [[ ! "$KEEP_EXISTING" =~ ^[Yy]$ ]]; then
        RECONFIGURE=false
        printf "%b✔ Keeping existing configuration.%b\n\n" "$GREEN" "$RESET"
    fi
fi

if [ "$RECONFIGURE" = true ]; then
    printf "\n%b--- Mandatory Discord Credentials ---%b\n" "$BOLD" "$RESET"

    # DISCORD_TOKEN
    DISCORD_TOKEN=""
    while [ -z "$DISCORD_TOKEN" ]; do
        prompt_read "1. Discord Bot Token (from Discord Developer Portal -> Bot -> Token): " DISCORD_TOKEN ""
        if [ -z "$DISCORD_TOKEN" ]; then
            printf "%bDiscord Bot Token is required.%b\n" "$RED" "$RESET"
        fi
    done

    # APPLICATION_ID
    APPLICATION_ID=""
    while [ -z "$APPLICATION_ID" ]; do
        prompt_read "2. Discord Application ID (from Developer Portal -> General Information): " APPLICATION_ID ""
        if [ -z "$APPLICATION_ID" ]; then
            printf "%bDiscord Application ID is required.%b\n" "$RED" "$RESET"
        elif [[ ! "$APPLICATION_ID" =~ ^[0-9]+$ ]]; then
            printf "%bDiscord Application ID must be a numeric snowflake (e.g. 123456789012345678).%b\n" "$RED" "$RESET"
            APPLICATION_ID=""
        fi
    done

    # GUILD_ID (Optional)
    while true; do
        prompt_read "3. Guild ID for fast testing (Optional, press Enter to register commands globally): " GUILD_ID ""
        if [ -z "$GUILD_ID" ] || [[ "$GUILD_ID" =~ ^[0-9]+$ ]]; then
            break
        else
            printf "%bGuild ID must be a numeric snowflake if provided.%b\n" "$RED" "$RESET"
        fi
    done

    printf "\n%b--- Zeron Engine Configuration ---%b\n" "$BOLD" "$RESET"
    prompt_read "4. Path to Zeron binary [zeron]: " ZERON_PATH "zeron"
    if ! command -v "$ZERON_PATH" &>/dev/null && [ ! -f "$ZERON_PATH" ]; then
        printf "%bNotice: '%s' not found yet in PATH or filesystem. Ensure Zeron is installed before launching.%b\n" "$YELLOW" "$ZERON_PATH" "$RESET"
    fi

    prompt_read "5. Default Harness (e.g., codex, claude-code, devin, antigravity) [codex]: " DEFAULT_HARNESS "codex"
    prompt_read "6. Default Model (e.g., gpt-6.1-sol, claude-sonnet-5, or press Enter for default): " DEFAULT_MODEL ""
    DEFAULT_PROJECT_DIR_FALLBACK="${TARGET_HOME}/projects"
    prompt_read "7. Default Project / Workspace Directory [${DEFAULT_PROJECT_DIR_FALLBACK}]: " DEFAULT_PROJECT_DIR "${DEFAULT_PROJECT_DIR_FALLBACK}"
    prompt_read "8. Command Prefix [!z]: " COMMAND_PREFIX "!z"

    # Ensure default project directory exists
    mkdir -p "$DEFAULT_PROJECT_DIR" 2>/dev/null || true
    chown "$TARGET_USER:$TARGET_USER" "$DEFAULT_PROJECT_DIR" 2>/dev/null || true

    printf "\n%b--- Cloudflare Clef (Intelligent Chatter Filter) ---%b\n" "$BOLD" "$RESET"
    printf "Cloudflare Clef uses Workers AI (@cf/cloudflare/clef-flash) to evaluate if multi-user\n"
    printf "conversations in text channels are directing requests to the bot.\n"
    printf "If not configured, the bot automatically uses an intelligent heuristic fallback.\n\n"
    prompt_read "Do you want to configure Cloudflare Clef? [y/N]: " SETUP_CLEF "n"

    CF_ACCOUNT_ID=""
    CF_API_TOKEN=""
    CF_MODEL="@cf/cloudflare/clef-flash"
    CF_ENDPOINT=""

    if [[ "$SETUP_CLEF" =~ ^[Yy]$ ]]; then
        while [ -z "$CF_ACCOUNT_ID" ]; do
            prompt_read "  • Cloudflare Account ID: " CF_ACCOUNT_ID ""
            if [ -z "$CF_ACCOUNT_ID" ]; then
                printf "  %bCloudflare Account ID is required when Clef is enabled.%b\n" "$RED" "$RESET"
            fi
        done

        while [ -z "$CF_API_TOKEN" ]; do
            prompt_read "  • Cloudflare API Token (Workers AI Read/Edit): " CF_API_TOKEN ""
            if [ -z "$CF_API_TOKEN" ]; then
                printf "  %bCloudflare API Token is required when Clef is enabled.%b\n" "$RED" "$RESET"
            fi
        done

        prompt_read "  • Cloudflare Model [@cf/cloudflare/clef-flash]: " CF_MODEL "@cf/cloudflare/clef-flash"
        prompt_read "  • Custom AI Endpoint URL (press Enter to skip): " CF_ENDPOINT ""
        printf "%b✔ Cloudflare Clef configuration collected.%b\n" "$GREEN" "$RESET"
    else
        printf "%bClef skipped. The bot will automatically use the built-in conversational heuristic fallback.%b\n" "$CYAN" "$RESET"
    fi

    printf "\n%b--- Discord Embedded Activities (/magpie & /terminal) ---%b\n" "$BOLD" "$RESET"
    printf "To enable embedded Magpie Web UI and VM Terminal activities in Discord,\n"
    printf "the bot requires your Cloudflare Worker URL and the shared UPDATE_SECRET.\n"
    printf "Both are required to safely coordinate activity switching for /magpie and /terminal.\n\n"
    prompt_read "Do you want to configure Discord Activities? [y/N]: " SETUP_ACTIVITIES "n"

    MAGPIE_URL=""
    UPDATE_SECRET=""

    if [[ "$SETUP_ACTIVITIES" =~ ^[Yy]$ ]]; then
        while [ -z "$MAGPIE_URL" ]; do
            prompt_read "  • Cloudflare Worker URL (e.g., https://magpie-activity.<subdomain>.workers.dev): " MAGPIE_URL ""
            if [ -z "$MAGPIE_URL" ]; then
                printf "  %bWorker URL is required for Discord Activities.%b\n" "$RED" "$RESET"
            fi
        done

        while [ -z "$UPDATE_SECRET" ]; do
            prompt_secret "  • UPDATE_SECRET (shared Worker auth secret): " UPDATE_SECRET
            if [ -z "$UPDATE_SECRET" ]; then
                printf "  %bUPDATE_SECRET is required to coordinate launch mode with Worker.%b\n" "$RED" "$RESET"
            fi
        done
        printf "%b✔ Discord Activities configuration collected.%b\n" "$GREEN" "$RESET"
    else
        printf "%bActivities skipped. Note: /magpie and /terminal will require MAGPIE_URL and UPDATE_SECRET in .env to function.%b\n" "$CYAN" "$RESET"
    fi

    # Generate .env file
    cat << EOF > "$ENV_FILE"
# ==============================================================================
# zeron-dc-bot Configuration (.env)
# Generated automatically by zeron-dc-bot installer
# ==============================================================================

# Discord Configuration
DISCORD_TOKEN=${DISCORD_TOKEN}
APPLICATION_ID=${APPLICATION_ID}
EOF

    if [ -n "$GUILD_ID" ]; then
        echo "GUILD_ID=${GUILD_ID}" >> "$ENV_FILE"
    else
        echo "# GUILD_ID=" >> "$ENV_FILE"
    fi

    cat << EOF >> "$ENV_FILE"

# Zeron Engine Configuration
ZERON_PATH=${ZERON_PATH}
DEFAULT_HARNESS=${DEFAULT_HARNESS}
DEFAULT_PROJECT_DIR=${DEFAULT_PROJECT_DIR}
EOF

    if [ -n "$DEFAULT_MODEL" ]; then
        echo "DEFAULT_MODEL=${DEFAULT_MODEL}" >> "$ENV_FILE"
    else
        echo "# DEFAULT_MODEL=" >> "$ENV_FILE"
    fi

    cat << EOF >> "$ENV_FILE"

# Bot Behavior
AUTO_CREATE_SESSIONS=true
COMMAND_PREFIX=${COMMAND_PREFIX}
SESSION_STORAGE_PATH=${INSTALL_DIR}/sessions.json

# Logging
RUST_LOG=info
EOF

    if [ -n "$CF_ACCOUNT_ID" ] && [ -n "$CF_API_TOKEN" ]; then
        cat << EOF >> "$ENV_FILE"

# Cloudflare Decision Model Configuration (Clef Flash)
CLOUDFLARE_ACCOUNT_ID=${CF_ACCOUNT_ID}
CLOUDFLARE_API_TOKEN=${CF_API_TOKEN}
CLOUDFLARE_MODEL=${CF_MODEL}
EOF
        if [ -n "$CF_ENDPOINT" ]; then
            echo "CLOUDFLARE_AI_ENDPOINT=${CF_ENDPOINT}" >> "$ENV_FILE"
        fi
    else
        cat << 'EOF' >> "$ENV_FILE"

# Cloudflare Decision Model Configuration (Clef Flash)
# Unconfigured: The bot automatically operates using an intelligent conversational heuristic fallback.
# CLOUDFLARE_ACCOUNT_ID=
# CLOUDFLARE_API_TOKEN=
# CLOUDFLARE_MODEL=@cf/cloudflare/clef-flash
# CLOUDFLARE_AI_ENDPOINT=
EOF
    fi

    if [ -n "$MAGPIE_URL" ] && [ -n "$UPDATE_SECRET" ]; then
        cat << EOF >> "$ENV_FILE"

# Magpie & Terminal Discord Embedded Activities (Required for /magpie and /terminal)
MAGPIE_URL=${MAGPIE_URL}
UPDATE_SECRET=${UPDATE_SECRET}
EOF
    else
        cat << 'EOF' >> "$ENV_FILE"

# Magpie & Terminal Discord Embedded Activities (Required for /magpie and /terminal)
# MAGPIE_URL=https://magpie-activity.your-worker.workers.dev
# UPDATE_SECRET=your_update_secret_token_here
EOF
    fi

    chmod 600 "$ENV_FILE"
    chown "$TARGET_USER:$TARGET_USER" "$ENV_FILE" 2>/dev/null || true
    printf "\n%b✔ Saved configuration to %s (permissions: 0600)%b\n\n" "$GREEN" "$ENV_FILE" "$RESET"
fi

# 5. Setup Systemd Service (Linux only)
if [ "$OS" = "Linux" ] && command -v systemctl &>/dev/null; then
    printf "%b===> Step 3: Systemd Daemon Service%b\n" "$CYAN" "$RESET"
    prompt_read "Would you like to install and enable zeron-dc-bot as a systemd service? [Y/n]: " SETUP_SYSTEMD "y"

    if [[ "$SETUP_SYSTEMD" =~ ^[Yy]$ ]]; then
        SERVICE_FILE="/etc/systemd/system/zeron-dc-bot.service"

        cat << EOF | run_as_root tee "$SERVICE_FILE" > /dev/null
[Unit]
Description=Zeron Discord Bridge Bot
After=network.target

[Service]
Type=simple
User=${TARGET_USER}
WorkingDirectory=${INSTALL_DIR}
EnvironmentFile=-${INSTALL_DIR}/.env
Environment="PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${TARGET_HOME}/.cargo/bin:${TARGET_HOME}/.local/bin"
ExecStart=${BIN_DIR}/zeron-dc-bot
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

        printf "%b✔ Created %s%b\n" "$GREEN" "$SERVICE_FILE" "$RESET"

        run_as_root systemctl daemon-reload
        run_as_root systemctl enable --now zeron-dc-bot.service

        sleep 1
        if systemctl is-active --quiet zeron-dc-bot.service; then
            printf "%b✔ zeron-dc-bot systemd service is active and running!%b\n\n" "$GREEN" "$RESET"
        else
            printf "%bNotice: Service installed. Status: %s%b\n\n" "$YELLOW" "$(systemctl is-active zeron-dc-bot.service || true)" "$RESET"
        fi
    fi
fi

# 6. Installation Summary
printf "%b================================================================%b\n" "$CYAN" "$RESET"
printf "%b🎉 Installation Complete!%b\n\n" "${GREEN}${BOLD}" "$RESET"
printf "  • Binary:       %b%s/zeron-dc-bot%b\n" "$BOLD" "$BIN_DIR" "$RESET"
printf "  • Config:       %b%s/.env%b\n" "$BOLD" "$INSTALL_DIR" "$RESET"
printf "  • State File:   %b%s/sessions.json%b\n" "$BOLD" "$INSTALL_DIR" "$RESET"

if [ "$OS" = "Linux" ] && command -v systemctl &>/dev/null; then
    printf "\n%bUseful Management Commands:%b\n" "$BOLD" "$RESET"
    printf "  • View live logs:       %bsudo journalctl -u zeron-dc-bot -f%b\n" "$CYAN" "$RESET"
    printf "  • Check service status: %bsudo systemctl status zeron-dc-bot%b\n" "$CYAN" "$RESET"
    printf "  • Restart bot:          %bsudo systemctl restart zeron-dc-bot%b\n" "$CYAN" "$RESET"
    printf "  • Stop bot:             %bsudo systemctl stop zeron-dc-bot%b\n" "$CYAN" "$RESET"
else
    printf "\n%bTo launch the bot manually:%b\n" "$BOLD" "$RESET"
    printf "  cd %s && %s/zeron-dc-bot\n" "$INSTALL_DIR" "$BIN_DIR"
fi
printf "%b================================================================%b\n\n" "$CYAN" "$RESET"

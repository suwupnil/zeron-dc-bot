# 🚀 zeron-dc-bot

[![Rust](https://img.shields.io/badge/rust-1.99.1%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Discord: serenity-rs](https://img.shields.io/badge/serenity--rs-0.12-5865F2.svg)](https://github.com/serenity-rs/serenity)
[![MCP: JSON--RPC](https://img.shields.io/badge/protocol-MCP%20JSON--RPC-blueviolet.svg)](https://modelcontextprotocol.io)

**zeron-dc-bot** is a Discord bridge bot for [Zeron](https://zeron.sh), built in Rust using [`serenity-rs`](https://github.com/serenity-rs/serenity). It links Discord text channels and threads directly with local Zeron AI coding agent sessions running on your host machine or target virtual machine.

---

## 🏗️ Architecture

```
                                    Host / VM Machine
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│   Discord Gateway                     zeron-dc-bot                     │
│         ▲                                                              │
│         │ WebSocket                                                    │
│         ▼                                                              │
│  [ Serenity Client ] ──► [ Event Handler ]                             │
│                                │                                       │
│                                ▼                                       │
│                        [ App State ]                                   │
│                        (Session Map,                                   │
│                         Threads/Sides,                                 │
│                         CWD, Models)                                   │
│                                │                                       │
│                                ▼                                       │
│                     [ ZeronClient (MCP) ]                              │
│                                │                                       │
│                                │ stdio JSON-RPC 2.0                    │
│                                ▼                                       │
│                   ┌─────────────────────────┐                          │
│                   │      zeron mcp          │                          │
│                   │  (Model Context Server) │                          │
│                   └────────────┬────────────┘                          │
│                                │ Local IPC                             │
│                                ▼                                       │
│                   ┌─────────────────────────┐                          │
│                   │      Zeron Engine       │                          │
│                   │   (Codex / Claude Code  │                          │
│                   │    Devin / Antigravity) │                          │
│                   └─────────────────────────┘                          │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 1. Channel-to-Session Mapping
When the bot is invited or active in a Discord text channel, it initializes a dedicated Zeron chat session (`kind: "chat"`) titled after the channel (e.g. `#general (Discord)`).
- Working directory, active AI model, and agent harness (`codex`, `claude-code`, etc.) are tracked per channel.
- Sessions are persisted to disk (`sessions.json`), ensuring uninterrupted state across bot reboots.

### 2. Threads as Side Chats (Context Forking)
When a Discord user creates a **Thread** inside a text channel:
- The bot creates a **Zeron Side Chat** (`create_chat` with `kind: "side"` and `parent` set to the channel's main chat session ID).
- **Context Inheritance:** The thread inherits the parent channel's conversational context, project path, and model.
- **Isolation:** Developers can ask questions, run benchmarks, or delegate speculative refactoring tasks in threads without disrupting or polluting the main channel's session history.

### 3. Local VM Execution Only
All activities execute strictly on the target host/VM running the Zeron Engine. Files are edited locally, tools are invoked locally, and no external code execution occurs outside your configured environment.

### 4. Multimodal Local Attachment Pipeline
When Discord users send images, stickers, voice notes, or documents:
- **Zero Cloud Leakage for Local Agent Execution:** Files are downloaded into a local disk cache (`./media_cache/<channel_id>/<message_id>/...`).
- **Voice Transcription:** Discord voice notes (Opus audio) are converted to 16kHz mono WAV via `ffmpeg` and transcribed using Cloudflare Workers AI Whisper or OpenAI Whisper.
- **Image OCR:** Images and stickers are processed via Cloudflare Workers AI vision models or local Tesseract OCR.
- **Native Local File References:** Zeron harnesses receive absolute paths to local files on disk alongside OCR/transcripts, allowing agents to directly open, inspect, or edit images and documents.

---

## ⚡ Slash Commands

| Command | Description | Autocomplete / Features |
|---|---|---|
| `/spawn <directory> [harness] [model] [effort]` | Initialize a project-bound Zeron session bound to a directory | Path validation, autocompletes harnesses, models, and reasoning effort levels. |
| `/model [harness] [model] [effort]` | Switch AI harness, model, or reasoning effort | Live autocomplete from Zeron daemon; preserves prior conversation memory. |
| `/effort <level>` | Adjust reasoning effort for active harness/model | Autocompletes supported levels (`low`, `medium`, `high`, etc.). |
| `/close` | Unlink monitoring from the channel and archive session | Cleanly tidies up previous chat state. |
| `/resume [session_id]` | Re-activate a closed session or switch to an archived session | Validates no conflicting channel bindings. |
| `/skills` | List detected skills across local installations | Ephemeral output of detected skills. |
| `/mcp` | List configured MCP servers and tool registries | Ephemeral output of active and configured MCP servers. |
| `/magpie` | Launch the Magpie Web UI Activity iframe | Discord native Activity integration (requires `MAGPIE_URL` & `UPDATE_SECRET`). |
| `/terminal` | Launch embedded VM terminal Activity | Discord native Activity integration (requires `MAGPIE_URL` & `UPDATE_SECRET`). |
| `/status` | View current chat session, active model, and directory | Displays live session info and decision model state. |
| `/interrupt` | Stop a currently running agent turn immediately | Calls Zeron MCP `interrupt_chat`. |
| `/help` | Display interactive guide and command manual | Ephemeral formatted markdown manual. |

---

## 🎙️ Multimodal Attachments (Images, Voice Notes, STT & OCR)

`zeron-dc-bot` includes full end-to-end multimodal attachment handling for images, Discord voice notes, stickers, and files.

### How It Works

1. **Local Disk Caching & Path Injection:**
   - Attachments sent in Discord channels are downloaded and cached on the local host machine under `MEDIA_CACHE_DIR` (default: `./media_cache`).
   - Because Zeron agent harnesses (Devin, Codex, Claude Code, etc.) execute directly on the local VM filesystem, the bot provides **absolute local file paths** in the prompt (`Attached images (local files — open them to view): ...`). Agents can inspect, open, and edit files using their native toolsets.

2. **Voice Notes & Audio Transcription (STT):**
   - Discord voice notes (`audio/ogg`, Opus) are automatically transcoded to 16kHz mono PCM WAV via `ffmpeg`.
   - Transcribed through your configured speech-to-text provider (`STT_PROVIDER`):
     - **Cloudflare Workers AI:** Fast, serverless Whisper transcription (`@cf/openai/whisper`) using your Cloudflare account credentials.
     - **OpenAI Whisper:** Direct transcription via OpenAI's audio transcription API (`whisper-1`).
     - **None:** Disables STT and flags voice notes with a fallback annotation.

3. **Images, Stickers & OCR:**
   - Discord stickers are automatically downloaded and processed as images.
   - Text within images is extracted via Optical Character Recognition (`OCR_PROVIDER`):
     - **Cloudflare Vision:** Cloudflare Workers AI vision model (`@cf/llava-hf/llava-1.5-7b-hf`).
     - **Tesseract CLI:** Fast offline local OCR using the `tesseract` binary on the host machine.
     - **None:** Disables OCR text extraction.

4. **Media-Aware Multi-User Chatter Decision:**
   - When users send images or voice notes without an explicit `@mention`, the Cloudflare Clef decision model and conversational heuristic take media presence into account, ensuring the bot attends to user-shared visual and audio context.

5. **Video Handling:**
   - Video files are currently out of scope and skipped safely with an informational log.

### System Requirements
- **ffmpeg:** Required for converting Discord voice notes to WAV for STT. Install via your package manager:
  ```bash
  # Debian / Ubuntu
  sudo apt-get install -y ffmpeg

  # macOS
  brew install ffmpeg
  ```
- **tesseract (Optional):** Required only if using `OCR_PROVIDER=tesseract`:
  ```bash
  # Debian / Ubuntu
  sudo apt-get install -y tesseract-ocr

  # macOS
  brew install tesseract
  ```

### Multimodal Configuration Reference

```env
# Directory to cache downloaded media files
MEDIA_CACHE_DIR=./media_cache

# Maximum file size boundaries (in bytes)
MEDIA_MAX_IMAGE_BYTES=25000000   # 25 MB
MEDIA_MAX_AUDIO_BYTES=25000000   # 25 MB
MEDIA_MAX_FILE_BYTES=50000000    # 50 MB

# Path to ffmpeg executable
FFMPEG_PATH=ffmpeg

# Speech-to-Text (STT) Settings
STT_PROVIDER=groq                # groq | cloudflare | openai | none
STT_MODEL=whisper-large-v3-turbo # whisper-large-v3-turbo, @cf/openai/whisper, or whisper-1
GROQ_API_KEY=                    # required if STT_PROVIDER=groq
OPENAI_API_KEY=                  # required if STT_PROVIDER=openai

# Optical Character Recognition (OCR) Settings
OCR_PROVIDER=cloudflare          # cloudflare | tesseract | none
OCR_MODEL=@cf/llava-hf/llava-1.5-7b-hf
TESSERACT_PATH=tesseract         # path to tesseract binary
```

---

## ⚡ Quick VM Installation (Recommended)

To install `zeron-dc-bot` on a Linux VM (Ubuntu, Debian, etc.) or macOS without compiling Rust from source:

```bash
curl -fsSL https://raw.githubusercontent.com/suwupnil/zeron-dc-bot/main/scripts/install.sh | bash
```

### What the installer does:
1. **GitHub Actions Prebuilt Binary:** Automatically detects your architecture (`x86_64` or `aarch64` / `arm64`) and downloads the precompiled binary built by GitHub Actions into `/usr/local/bin/zeron-dc-bot`. No Rust toolchain or build dependencies required on the VM.
2. **Terminal Configuration Wizard:** Guides you step-by-step through setting up `.env`:
   - **Mandatory Discord Keys:** Discord Bot Token (`DISCORD_TOKEN`) and Application ID (`APPLICATION_ID`).
   - **Zeron Settings:** Binary path (`ZERON_PATH`), default agent harness (`DEFAULT_HARNESS`), optional default model (`DEFAULT_MODEL`), and default project workspace directory (`DEFAULT_PROJECT_DIR`).
   - **Cloudflare Clef Filter:** Prompts if you would like to enable Cloudflare Clef (`@cf/cloudflare/clef-flash`) for multi-user channels. If yes, it collects your Account ID and API token. If no, the bot automatically uses the built-in conversational heuristic fallback.
   - **Multimodal Media (STT & OCR):** Prompts for optional media handling configurations (Speech-to-Text with Cloudflare Whisper or OpenAI Whisper, and OCR with Cloudflare vision or Tesseract), checks for `ffmpeg`, and configures media caching.
   - **Embedded Activities (/magpie & /terminal):** Prompts for `MAGPIE_URL` and `UPDATE_SECRET` (both required to coordinate launch mode with the Cloudflare Worker).
3. **Automated systemd Service (Linux):** Configures and starts `/etc/systemd/system/zeron-dc-bot.service` automatically so your bot runs persistently across reboots.

---

## 🛠️ Manual & Developer Setup

If you prefer building from source or contributing to the codebase:

### Prerequisites

1. **Rust:** Rust 1.85+ (tested and confirmed on Rust 1.99.1).
2. **Zeron Engine:** Zeron installed and running locally on macOS or Linux.
   - Default binary path: `/Applications/Zeron.app/Contents/MacOS/zeron` or `zeron` in your `$PATH`.
3. **Discord Bot Token:** A Discord application bot from the [Discord Developer Portal](https://discord.com/developers/applications).
4. **ffmpeg (Recommended):** Required for transcoding Discord voice notes to WAV for Speech-to-Text (`sudo apt-get install -y ffmpeg` or `brew install ffmpeg`).
5. **tesseract (Optional):** Required only if using local Tesseract for OCR (`sudo apt-get install -y tesseract-ocr` or `brew install tesseract`).

---

### Step 1: Discord Bot Setup

1. Go to the [Discord Developer Portal](https://discord.com/developers/applications) and create a **New Application**.
2. Navigate to the **Bot** tab:
   - Click **Reset Token** and copy your token.
   - Under **Privileged Gateway Intents**, enable:
     - `Message Content Intent` (required for receiving user prompts)
     - `Server Members Intent` (optional)
3. Navigate to **OAuth2 -> URL Generator**:
   - Scopes: `bot`, `applications.commands`
   - Bot Permissions:
     - `Send Messages`
     - `Send Messages in Threads`
     - `Create Public Threads`
     - `Create Private Threads`
     - `Embed Links`
     - `Attach Files`
     - `Read Message History`
     - `Use Slash Commands`
4. Copy the generated URL and authorize the bot to your Discord server.

---

### Step 2: Configuration

Clone the repository and copy the example environment configuration:

```bash
git clone https://github.com/suwupnil/zeron-dc-bot.git
cd zeron-dc-bot
cp .env.example .env
```

Edit `.env` with your values:

```env
# Discord Credentials
DISCORD_TOKEN=your_bot_token_here
APPLICATION_ID=your_application_id_here

# Optional: Specific Guild ID for instantaneous slash command registration during development
GUILD_ID=your_guild_id_here

# Local Zeron Daemon (Linux VM or macOS)
ZERON_PATH=zeron

# Default Agent Settings
DEFAULT_HARNESS=codex
DEFAULT_MODEL=gpt-6.1-sol
DEFAULT_PROJECT_DIR=/home/ubuntu/projects

# Behavior
AUTO_CREATE_SESSIONS=true
COMMAND_PREFIX=!z
SESSION_STORAGE_PATH=./sessions.json
RUST_LOG=info

# Multimodal & Media Processing
MEDIA_CACHE_DIR=./media_cache
MEDIA_MAX_IMAGE_BYTES=25000000
MEDIA_MAX_AUDIO_BYTES=25000000
MEDIA_MAX_FILE_BYTES=50000000
FFMPEG_PATH=ffmpeg
STT_PROVIDER=cloudflare
STT_MODEL=@cf/openai/whisper
OCR_PROVIDER=cloudflare
OCR_MODEL=@cf/llava-hf/llava-1.5-7b-hf

# Embedded Activities (Required for /magpie & /terminal)
MAGPIE_URL=https://magpie-activity.your-worker.workers.dev
UPDATE_SECRET=your_worker_update_secret
```

---

### Step 3: Build & Run

Run the bot:

```bash
# Check compilation
cargo check

# Run all unit and integration tests
cargo test

# Launch the bot in release mode
cargo run --release
```

---

### 🖥️ Running on a Linux VM (systemd Service)

To run the bot as a persistent daemon on your Ubuntu / Debian VM:

1. Build the release binary:
```bash
cargo build --release
sudo cp target/release/zeron-dc-bot /usr/local/bin/
```

2. Create `/etc/systemd/system/zeron-dc-bot.service`:
```ini
[Unit]
Description=Zeron Discord Bridge Bot
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/zeron-dc-bot
EnvironmentFile=/home/ubuntu/zeron-dc-bot/.env
ExecStart=/usr/local/bin/zeron-dc-bot
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

3. Enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now zeron-dc-bot
sudo systemctl status zeron-dc-bot
```

---

## 🧪 Testing

The repository includes a comprehensive test suite covering formatting, chunking, session persistence, and live MCP integration:

```bash
# Run all unit and integration tests
cargo test

# Run tests with stdout logging
cargo test -- --nocapture
```

Test suite overview:
- `tests/formatter_test.rs`: Ensures Discord 2000-character boundary splitting preserves open markdown code blocks (` ```lang `) across message chunks.
- `tests/state_test.rs`: Validates session serialization, persistence across disk reloads, and directory/model mutation.
- `tests/zeron_models_test.rs`: Verifies JSON-RPC 2.0 protocol serialization and deserialization against MCP schemas.
- `tests/zeron_live_test.rs`: Exercises live MCP initialization, harness discovery, model listing, session creation, side chat branching, and clean archival against the local Zeron engine.

---

## 🛡️ Security & Privacy

- **Machine-Local Execution:** Agent execution occurs strictly on the local machine hosting Zeron.
- **No Remote Code Execution:** The bot forwards Discord user prompts to the local Zeron daemon via stdio JSON-RPC.
- **Isolated Side Chats:** Threads operate in their own branching side chat, preventing experimental code or prompt injections from contaminating your main text channel session.

---

## 📄 License

Licensed under the [MIT License](LICENSE).

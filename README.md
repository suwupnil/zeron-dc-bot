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

---

## ⚡ Slash Commands

| Command | Description | Autocomplete / Features |
|---|---|---|
| `/dir <path>` | Set the host VM project directory for this channel / thread | Real-time path autocomplete & local VM directory validation. |
| `/model <name>` | Change the AI model for this channel / thread | Dynamic autocomplete querying available models from Zeron `list_models`. |
| `/mcp <server>` | Attach/select an MCP server for this conversation | Autocompletes available MCP servers, tools, and local plugins. |
| `/skills <skill>` | Activate a specialized workflow / skill | Autocompletes local skills (Cloudflare, React, HyperFrames, etc.). |
| `/status` | View current chat session, active model, directory, and thread hierarchy | Displays live status (`idle`, `working`, `awaitingInput`) from Zeron engine. |
| `/reset` | Archive the current Zeron session and start a fresh session | Cleanly tidies up previous chat state. |
| `/interrupt` | Stop a currently running agent turn immediately | Calls Zeron `interrupt_chat`. |
| `/help` | Display interactive guide and command overview | Formatted markdown manual. |

---

## 🚀 Getting Started

### Prerequisites

1. **Rust:** Rust 1.85+ (tested and confirmed on Rust 1.99.1).
2. **Zeron Engine:** Zeron installed and running locally on macOS or Linux.
   - Default binary path: `/Applications/Zeron.app/Contents/MacOS/zeron` or `zeron` in your `$PATH`.
3. **Discord Bot Token:** A Discord application bot from the [Discord Developer Portal](https://discord.com/developers/applications).

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
git clone https://github.com/suwu/zeron-dc-bot.git
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

# Local Zeron Daemon
ZERON_PATH=/Applications/Zeron.app/Contents/MacOS/zeron

# Default Agent Settings
DEFAULT_HARNESS=codex
DEFAULT_MODEL=gpt-6.1-sol
DEFAULT_PROJECT_DIR=/Users/swap/Developer

# Behavior
AUTO_CREATE_SESSIONS=true
COMMAND_PREFIX=!z
SESSION_STORAGE_PATH=./sessions.json
RUST_LOG=info
```

---

### Step 3: Build & Run

Run the bot locally:

```bash
# Check code
cargo check

# Run tests (includes live Zeron MCP integration tests)
cargo test

# Launch the bot
cargo run --release
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

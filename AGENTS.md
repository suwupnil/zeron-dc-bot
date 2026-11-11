# AGENTS.md — Zeron Discord Bridge Bot (`zeron-dc-bot`)

Comprehensive technical architecture, operating model, and guidelines for AI agents, developers, and autonomous operators working on or maintaining this codebase.

---

## 1. System Mission & Overview

`zeron-dc-bot` is a high-performance, asynchronous Rust bridge between **Discord** and **Zeron** (a local multi-agent execution engine). It transforms Discord channels and threads into persistent, interactive workspaces powered by local or cloud AI agent harnesses (Devin, Codex, Claude Code, OpenCode, etc.).

### Key Highlights
- **Sub-process JSON-RPC 2.0 MCP Client:** Direct stdio communication with the local `zeron mcp` daemon.
- **Intelligent Cloudflare Decision Gateway:** Filters multi-user chatter using Cloudflare Workers AI (`@cf/cloudflare/clef-flash`) and a conversational heuristic fallback to prevent bot spam while staying active when needed.
- **Dynamic Branching Side Chats:** Discord threads automatically fork into Zeron side chats, inheriting parent project context without polluting the primary workspace branch.
- **Real-Time Introspection:** Autocompletes harnesses, models, and reasoning effort levels directly from live Zeron daemon status.
- **Zero-Friction Channel Topics:** Synchronizes channel topics to `<harness>/<model>.<effort>`.

---

## 2. Architecture & Component Graph

```
                              ┌──────────────────────────────────────────────┐
                              │                Discord Client                │
                              └──────────────────────┬───────────────────────┘
                                                     │ Gateway Events / Interactions
                                                     ▼
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     serenity::EventHandler                                     │
│                                  (src/discord/handler.rs)                                      │
├───────────────────────────────┬──────────────────────────────────┬─────────────────────────────┤
│         Slash Commands        │          Channel Messages        │        Thread Lifecycle     │
│   (/spawn, /model, /effort,   │  • Extract text (strip media)    │  • thread_create:           │
│    /close, /resume, /skills,  │  • Decision Model / Heuristic    │    forks Zeron side chat    │
│    /mcp, /magpie, /status,    │  • Route to active Zeron chat    │    from parent workspace    │
│    /interrupt)                │                                  │                             │
└───────────────┬───────────────┴──────────────────┬───────────────┴──────────────┬──────────────┘
                │                                  │                              │
                ▼                                  ▼                              ▼
┌───────────────────────────────┐ ┌────────────────────────────────┐ ┌───────────────────────────┐
│          AppState             │ │         DecisionClient         │ │        ZeronClient        │
│       (src/state.rs)          │ │       (src/decision.rs)        │ │  (src/zeron/client.rs)    │
├───────────────────────────────┤ ├────────────────────────────────┤ ├───────────────────────────┤
│ • Channel-to-session map      │ │ • Cloudflare Clef-Flash AI     │ │ • stdio JSON-RPC 2.0      │
│ • Disk persistence (JSON)     │ │ • Native "noul" probability    │ │ • MCP tool invocation     │
│ • Origin tracking (Spawn/Tag) │ │ • Conversational Heuristic     │ │ • Streams turns & tools   │
└───────────────┬───────────────┘ └────────────────────────────────┘ └─────────────┬─────────────┘
                │                                                                 │
                ▼                                                                 ▼
      `./sessions.json`                                            `/your/path/.local/bin/zeron`
```

---

## 3. Directory & Module Map

| Path | Responsibility |
| :--- | :--- |
| [`src/main.rs`](/zeron-dc-bot/src/main.rs) | Entry point: parses environment, connects to Zeron MCP, sets gateway intents, initializes Discord client. |
| [`src/lib.rs`](/zeron-dc-bot/src/lib.rs) | Crate root exposing `config`, `decision`, `discord`, `error`, `state`, and `zeron`. |
| [`src/config.rs`](/zeron-dc-bot/src/config.rs) | Environment variable reader (`.env`) for Discord tokens, Cloudflare tokens, paths, and defaults. |
| [`src/decision.rs`](/zeron-dc-bot/src/decision.rs) | Binary classification engine determining if the bot should intervene in multi-user chatter. |
| [`src/state.rs`](/zeron-dc-bot/src/state.rs) | Thread-safe in-memory session registry with atomic disk serialization to `./sessions.json`. |
| [`src/error.rs`](/zeron-dc-bot/src/error.rs) | Structured error definitions (`BotError`, `BotResult`) with `thiserror`. |
| [`src/zeron/client.rs`](/zeron-dc-bot/src/zeron/client.rs) | Asynchronous MCP client managing child stdio processes and line-delimited JSON-RPC calls. |
| [`src/zeron/models.rs`](/zeron-dc-bot/src/zeron/models.rs) | Strongly typed serde structs matching Zeron MCP schemas (`create_chat`, `list_harnesses`, etc.). |
| [`src/discord/handler.rs`](/zeron-dc-bot/src/discord/handler.rs) | Serenity `EventHandler` implementing slash command routing, message triage, and thread forking. |
| [`src/discord/formatter.rs`](/zeron-dc-bot/src/discord/formatter.rs) | 2000-character boundary splitter preserving markdown code fences (` ``` `). |
| [`src/discord/topic.rs`](/zeron-dc-bot/src/discord/topic.rs) | Channel topic synchronization (`<harness>/<model>.<effort>`) and recursive skill/MCP scanner. |
| [`src/discord/commands/`](/zeron-dc-bot/src/discord/commands/) | Slash command implementations: `spawn`, `close`, `resume`, `model`, `effort`, `skills`, `mcp`, `magpie`, `status`, `interrupt`, `help`. |
| [`scripts/`](/zeron-dc-bot/scripts) | Automated installation scripts for Linux/macOS VMs (`scripts/install.sh` for 1-step curl installs and interactive `.env` configuration). |
| [`cloudflare/`](/zeron-dc-bot/cloudflare) | Cloudflare Worker reverse-proxy and VM automation scripts for Discord Embedded Activities (auto-auth, self-healing origin sync, Ed25519 verification). |

---

## 4. Operational Workflows & Rules

### 4.1 Session Creation & Triage
1. **Never auto-create sessions on channel creation.**
2. **`/spawn <directory>` (Project Session):**
   - Mandatory argument `directory`. If path does not exist, it is created automatically.
   - Spawns a full project-bound Zeron session with origin `SessionOrigin::Spawn`.
   - Side chats (Discord threads) are enabled for this channel.
3. **`@mention` (Casual Tag Session):**
   - Triggered only when the bot is tagged in a channel with **no active session**.
   - If a session is already active, `@mention` does **not** recreate the session; it routes the message into the active conversation.
   - Casual sessions are marked `SessionOrigin::Tag` and archived in Zeron without a project directory.
4. **Enforcement:**
   - Any operational slash command other than `/spawn`, `/resume`, `/help`, or `/magpie` is blocked if no active session is linked to the channel.

### 4.2 Decision Model & Chatter Suppression
- In channels with an active session, if the bot is **not** explicitly @mentioned or replied to:
  1. The bot pulls the last 10 text messages from the channel.
  2. Media URLs, emojis, and attachments are stripped via `extract_text_only`.
  3. The payload is sent to Cloudflare Workers AI (`@cf/cloudflare/clef-flash`) using typed `noul` instructions.
  4. If probability $\ge 0.5$, the bot responds; otherwise, it remains silent.
  5. If Cloudflare is unconfigured or unreachable, it falls back to the smart conversational heuristic in [`src/decision.rs`](/zeron-dc-bot/src/decision.rs).
  6. The bot's name is dynamically fetched from the Discord gateway cache (`ctx.cache.current_user().name`).

### 4.3 Harness & Model Autocomplete Rules
- **`/model` arguments:**
  1. `harness` *(optional)*
  2. `model` *(optional)*
  3. `effort` *(optional)*
  - Running `/model` without arguments automatically shifts to the previously chosen model configuration (or default).
- **Harness Autocomplete:**
  - Mock internal fixtures (`id == "mock"`) are strictly excluded.
  - Harnesses with `installed == false` are hidden.
  - Harnesses with `signed_in == false` are marked `<Name> (not signed in)`.
  - Fully ready harnesses appear cleanly as `<Name>`.
  - The `available` flag is ignored.
- **Model Autocomplete:**
  - Dynamically queries `list_models` for the selected harness.
- **Effort Autocomplete:**
  - Queries `reasoningLevels` from `list_models` and `list_harnesses`.
- **Channel Topic Sync:**
  - Format: `<harness>/<model>.<effort>`.
  - Skills and MCP lists are never shown in the channel topic.

### 4.4 `/effort` Command
- Allows quick switching of thinking/reasoning levels for the currently active harness and model without needing to re-enter them.
- Updates the channel topic upon change.

### 4.5 Ephemeral Tools (`/skills` and `/mcp`)
- **`/skills`:** Lists all detected skills found recursively in `~/.gemini/antigravity-cli/builtin/skills`, `~/.hermes/hermes-agent/skills`, etc.
- **`/mcp`:** Lists all configured MCP servers found in `~/.gemini/config/mcp_config.json`, Codex plugins, and live Zeron plugins.
- Both commands respond with **ephemeral** messages (visible only to the caller and dismissible).

### 4.6 `/close` and `/resume` Lifecycle
- **`/close`:** Unlinks monitoring from the channel and archives the chat session in Zeron.
- **`/resume [session_id]`:**
  - Without argument: Re-activates the previously closed session of this channel.
  - With argument: Switches/resumes the specific Zeron session ID, ensuring that no other active Discord channel is bound to that session ID.

### 4.7 `/magpie` Discord Embedded Activity
- Triggers Discord's native `LAUNCH_ACTIVITY` response (Type 12) to open the embedded Magpie UI iframe inside Discord.
- If the application does not yet have Activities enabled in the Discord Developer Portal, it returns a graceful ephemeral guide with setup instructions and a direct link.
- Available globally across channels without requiring an active Zeron chat session.

---

## 5. Development & Deployment

### 5.1 Local Testing & Build
```bash
# Run unit and integration tests
cargo test

# Build optimized release binary
cargo build --release
```

### 5.2 VM Deployment
The production bot runs on Linux VMs (Debian, Ubuntu, etc.):

#### 1-Step Automated VM Installation
On the target VM, run:
```bash
curl -fsSL https://raw.githubusercontent.com/suwupnil/zeron-dc-bot/main/scripts/install.sh | bash
```
This automatically downloads prebuilt GitHub Actions binaries, provides an interactive terminal wizard to configure `.env` (Discord credentials, Zeron engine defaults, optional Cloudflare Clef flash filter), and installs the systemd unit.

#### Manual Build & Service Management
```bash
# Connect to host
ssh -i /path/to/ssh-key user@host

# Build release on VM (if building manually)
export PATH="/your/path/.cargo/bin:$PATH"
cd /your/path/zeron-dc-bot
cargo build --release

# Restart systemd service
sudo systemctl restart zeron-dc-bot

# View live service logs
journalctl -u zeron-dc-bot -f
```

---

## 6. Guidelines for Autonomous Agents

When modifying this repository:
1. **Preserve JSON-RPC Schema Integrity:** Do not change field naming in [`src/zeron/models.rs`](/zeron-dc-bot/src/zeron/models.rs) without verifying against `zeron mcp` tools.
2. **Maintain Discord 2000-Char Safety:** Always use `MessageChunker::chunk` when sending assistant messages to avoid Discord API errors.
3. **Keep Tests Green:** All unit tests in `src/` and integration tests in `tests/` must pass before pushing or deploying.
4. **Never Expose Secrets:** Ensure tokens in `.env` are never logged, echoed, or checked into version control.

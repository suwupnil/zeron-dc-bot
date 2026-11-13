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
│   (/spawn, /model, /effort,   │  • Multimodal Attachment Triage  │  • thread_create:           │
│    /close, /resume, /skills,  │  • Audio/STT & Image/OCR router  │    forks Zeron side chat    │
│    /mcp, /magpie, /status,    │  • Media-aware Clef & Heuristic  │    from parent workspace    │
│    /interrupt)                │  • Prompt assembly & local paths │                             │
└───────────────┬───────────────┴──────────────────┬───────────────┴──────────────┬──────────────┘
                │                                  │                              │
                ▼                                  ▼                              ▼
┌───────────────────────────────┐ ┌────────────────────────────────┐ ┌───────────────────────────┐
│          AppState             │ │         DecisionClient         │ │        ZeronClient        │
│       (src/state.rs)          │ │       (src/decision.rs)        │ │  (src/zeron/client.rs)    │
├───────────────────────────────┤ ├────────────────────────────────┤ ├───────────────────────────┤
│ • Channel-to-session map      │ │ • Cloudflare Clef-Flash AI     │ │ • stdio JSON-RPC 2.0      │
│ • Disk persistence (JSON)     │ │ • Media-aware prompt context   │ │ • MCP tool invocation     │
│ • Origin tracking (Spawn/Tag) │ │ • Conversational Heuristic     │ │ • Streams turns & tools   │
└───────────────┬───────────────┘ └────────────────┬───────────────┘ └─────────────┬─────────────┘
                │                                  │                               │
                │                                  ▼                               │
                │                 ┌────────────────────────────────┐               │
                │                 │         Media Subsystem        │               │
                │                 │       (src/media/*.rs)         │               │
                │                 ├────────────────────────────────┤               │
                │                 │ • Cache & size validation      │               │
                │                 │ • ffmpeg 16kHz WAV conversion  │               │
                │                 │ • Cloudflare/OpenAI STT        │               │
                │                 │ • Cloudflare/Tesseract OCR     │               │
                │                 │ • Local file path injection    │               │
                │                 └────────────────────────────────┘               │
                ▼                                                                  ▼
      `./sessions.json`                                            `/your/path/.local/bin/zeron`
```

---

## 3. Directory & Module Map

| Path | Responsibility |
| :--- | :--- |
| [`src/main.rs`](/zeron-dc-bot/src/main.rs) | Entry point: parses environment, connects to Zeron MCP, sets gateway intents, initializes Discord client. |
| [`src/lib.rs`](/zeron-dc-bot/src/lib.rs) | Crate root exposing `config`, `decision`, `discord`, `error`, `media`, `state`, and `zeron`. |
| [`src/config.rs`](/zeron-dc-bot/src/config.rs) | Environment variable reader (`.env`) for Discord tokens, Cloudflare tokens, media limits, STT/OCR, paths, and defaults. |
| [`src/decision.rs`](/zeron-dc-bot/src/decision.rs) | Binary classification engine determining if the bot should intervene in multi-user chatter, enriched with media summary awareness. |
| [`src/media/mod.rs`](/zeron-dc-bot/src/media/mod.rs) | Multimodal attachment pipeline: extraction, disk caching, size enforcement, and sticker/video classification. |
| [`src/media/transcribe.rs`](/zeron-dc-bot/src/media/transcribe.rs) | Speech-to-text pipeline transcoding voice notes/audio via ffmpeg and querying Cloudflare Whisper or OpenAI Whisper. |
| [`src/media/ocr.rs`](/zeron-dc-bot/src/media/ocr.rs) | Vision & OCR pipeline executing Cloudflare Workers AI vision models or local Tesseract OCR CLI. |
| [`src/media/prompt.rs`](/zeron-dc-bot/src/media/prompt.rs) | Formats multimodal prompts injecting author prefixes, local file paths, and transcript/OCR blocks for Zeron. |
| [`src/state.rs`](/zeron-dc-bot/src/state.rs) | Thread-safe in-memory session registry with atomic disk serialization to `./sessions.json`. |
| [`src/error.rs`](/zeron-dc-bot/src/error.rs) | Structured error definitions (`BotError`, `BotResult`) with `thiserror`. |
| [`src/zeron/client.rs`](/zeron-dc-bot/src/zeron/client.rs) | Asynchronous MCP client managing child stdio processes and line-delimited JSON-RPC calls. |
| [`src/zeron/models.rs`](/zeron-dc-bot/src/zeron/models.rs) | Strongly typed serde structs matching Zeron MCP schemas (`create_chat`, `list_harnesses`, etc.). |
| [`src/discord/handler.rs`](/zeron-dc-bot/src/discord/handler.rs) | Serenity `EventHandler` implementing slash command routing, multimodal message triage, and thread forking. |
| [`src/discord/formatter.rs`](/zeron-dc-bot/src/discord/formatter.rs) | 2000-character boundary splitter preserving markdown code fences (` ``` `). |
| [`src/discord/topic.rs`](/zeron-dc-bot/src/discord/topic.rs) | Channel topic synchronization (`<harness>/<model>.<effort>`) and recursive skill/MCP scanner. |
| [`src/discord/commands/`](/zeron-dc-bot/src/discord/commands/) | Slash command implementations: `spawn`, `close`, `resume`, `model`, `effort`, `skills`, `mcp`, `magpie`, `terminal`, `status`, `interrupt`, `help`. |
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
   - Any operational slash command other than `/spawn`, `/resume`, `/help`, `/magpie`, or `/terminal` is blocked if no active session is linked to the channel.

### 4.2 Multimodal Decision Model & Chatter Suppression
- In channels with an active session, if the bot is **not** explicitly @mentioned or replied to:
  1. The bot inspects incoming attachments and generates a structured media summary (e.g., `[user sent 2 images, 1 voice note]`).
  2. The bot pulls the last 10 text messages from the channel.
  3. Media URLs, emojis, and inline artifacts are cleaned while preserving message intent.
  4. The payload—including recent message history and the media summary—is evaluated by Cloudflare Workers AI (`@cf/cloudflare/clef-flash`) using typed `noul` instructions.
  5. If probability $\ge 0.5$, the bot responds; otherwise, it remains silent.
  6. **Fallback Heuristic:** If Cloudflare is unconfigured or unreachable, it falls back to the smart conversational heuristic in [`src/decision.rs`](/zeron-dc-bot/src/decision.rs). If `media_summary` is non-empty and user text is empty or brief, the heuristic biases to `true` (attending to user-shared images or voice notes).
  7. The bot's name is dynamically fetched from the Discord gateway cache (`ctx.cache.current_user().name`).

### 4.3 Multimodal Attachment Pipeline (Images, Voice Notes, STT & OCR)
1. **Attachment Ingestion & Caching:**
   - Discord attachments and stickers are saved under `MEDIA_CACHE_DIR/<channel_id>/<message_id>/<filename>`.
   - File size boundaries are strictly enforced: `MEDIA_MAX_IMAGE_BYTES` (default 25MB), `MEDIA_MAX_AUDIO_BYTES` (default 25MB), and `MEDIA_MAX_FILE_BYTES` (default 50MB).
   - Filenames are sanitized to prevent directory traversal attacks.
2. **Video Handling:**
   - Video files (`video/*`) are explicitly out of scope; the bot logs an informational note and skips video processing without interrupting the turn.
3. **Voice Note / Audio Processing & Speech-to-Text (STT):**
   - Discord voice notes (`audio/ogg`, Opus) are converted into 16kHz mono PCM WAV via `ffmpeg` (`ffmpeg -i input -ar 16000 -ac 1 -c:a pcm_s16le output.wav`).
   - Transcription routing (`STT_PROVIDER`):
     - `groq`: Ultra-fast Groq Cloud Whisper API (`whisper-large-v3-turbo`) using `GROQ_API_KEY`.
     - `cloudflare`: Cloudflare Workers AI Whisper (`@cf/openai/whisper`) sending base64-encoded audio.
     - `openai`: OpenAI Whisper API (`whisper-1`) using `OPENAI_API_KEY`.
     - `none`: Skips automatic transcription.
     - If transcription fails or STT is unavailable, returns `[Voice message — transcription unavailable]`.
4. **Image & Sticker Processing & Optical Character Recognition (OCR):**
   - Discord stickers (`msg.sticker_items`) are retrieved and handled as image attachments.
   - Visual understanding heuristic determines whether text extraction is appropriate.
   - OCR routing (`OCR_PROVIDER`):
     - `cloudflare`: Cloudflare Workers AI vision model (`@cf/llava-hf/llava-1.5-7b-hf`).
     - `tesseract`: Local CLI `tesseract` execution on the host VM.
     - `none`: Disables OCR text extraction.
5. **Zeron Native Local File Prompt Assembly:**
   - Because Zeron agent harnesses execute directly on the local host machine, attachments are passed as real filesystem paths rather than remote HTTP URLs.
   - `build_zeron_prompt` formats prompt text and appends:
     ```
     See the attached image(s).
     Attached images (local files — open them to view):
     - /absolute/path/to/media_cache/.../image.png
       OCR: <extracted text>

     [Voice message from <author>]
     Transcript: <transcribed text>

     See the attached file(s).
     Attached files (local files — open them to view):
     - /absolute/path/to/media_cache/.../doc.pdf
     ```

### 4.4 Harness & Model Autocomplete Rules
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

### 4.5 `/effort` Command
- Allows quick switching of thinking/reasoning levels for the currently active harness and model without needing to re-enter them.
- Updates the channel topic upon change.

### 4.6 Ephemeral Tools (`/skills` and `/mcp`)
- **`/skills`:** Lists all detected skills found recursively in `~/.gemini/antigravity-cli/builtin/skills`, `~/.hermes/hermes-agent/skills`, etc.
- **`/mcp`:** Lists all configured MCP servers found in `~/.gemini/config/mcp_config.json`, Codex plugins, and live Zeron plugins.
- Both commands respond with **ephemeral** messages (visible only to the caller and dismissible).

### 4.7 `/close` and `/resume` Lifecycle
- **`/close`:** Unlinks monitoring from the channel and archives the chat session in Zeron.
- **`/resume [session_id]`:**
  - Without argument: Re-activates the previously closed session of this channel.
  - With argument: Switches/resumes the specific Zeron session ID, ensuring that no other active Discord channel is bound to that session ID.

### 4.8 `/magpie` Discord Embedded Activity
- Triggers Discord's native `LAUNCH_ACTIVITY` response (Type 12) to open the embedded Magpie UI iframe inside Discord.
- If the application does not yet have Activities enabled in the Discord Developer Portal, it returns a graceful ephemeral guide with setup instructions and a direct link.
- Available globally across channels without requiring an active Zeron chat session.

### 4.9 `/terminal` Discord Embedded Activity
- Triggers Discord's native `LAUNCH_ACTIVITY` response (Type 12) to open an embedded xterm.js terminal connected directly to the host VM shell via SSH.
- Proxied through the Cloudflare Worker with path routing (`/terminal/*`), injecting `Authorization: Bearer <TERMINAL_SECRET>` into the WebSocket stream to the local bridge daemon (`terminal-bridge.js` on `127.0.0.1:7681`).
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

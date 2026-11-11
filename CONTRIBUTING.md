# Contributing to zeron-dc-bot

First off, thank you for considering contributing to **zeron-dc-bot**! 🎉

This document outlines the principles, guidelines, and workflows for contributing code, reporting bugs, and proposing new features.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Architecture & Design Principles](#architecture--design-principles)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Development Workflow](#development-workflow)
  - [Branching Strategy](#branching-strategy)
  - [Coding Standards](#coding-standards)
  - [Running Tests](#running-tests)
- [Submitting Changes](#submitting-changes)
  - [Pull Request Process](#pull-request-process)
- [Security Disclosures](#security-disclosures)

---

## Code of Conduct

All contributors and maintainers are expected to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it to ensure a welcoming and productive environment.

---

## Architecture & Design Principles

Before making changes, please familiarize yourself with the system architecture in [`README.md`](README.md) and [`AGENTS.md`](AGENTS.md). Keep the following core principles in mind:

1. **Subprocess JSON-RPC 2.0 MCP Client:** The bot communicates with the local Zeron engine via stdio using the Model Context Protocol (MCP). Preserve schema definitions in `src/zeron/models.rs`.
2. **Machine-Local Execution:** Agent execution occurs strictly on the host VM running the Zeron daemon. Do not introduce remote command execution pathways.
3. **Discord 2000-Character Message Safety:** Discord enforces a strict 2000-character limit per message. Always format outbound messages through `MessageChunker::chunk` in `src/discord/formatter.rs` to maintain code fence integrity (` ``` `).
4. **Context Isolation:** Threads map to Zeron side chats (`kind: "side"`). Keep side chat state and mutations isolated from parent workspace sessions.
5. **No Secrets in Source:** Ensure bot tokens, API keys, or private identifiers are never committed or logged in test outputs.

---

## Development Setup

### Prerequisites

- **Rust toolchain:** Rust 1.85+ (tested on stable, e.g. 1.85 - 1.99+). Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Zeron Engine:** Zeron installed and available in `$PATH` or configured via `ZERON_PATH` in `.env`.
- **Discord Bot Application:** Create an application on the [Discord Developer Portal](https://discord.com/developers/applications) with:
  - Bot permissions: `Send Messages`, `Send Messages in Threads`, `Create Public Threads`, `Create Private Threads`, `Embed Links`, `Read Message History`, `Use Slash Commands`.
  - Gateway Intents: `Message Content Intent`.

### Local Setup

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/<your-username>/zeron-dc-bot.git
   cd zeron-dc-bot
   ```

2. Configure environment variables:
   ```bash
   cp .env.example .env
   # Edit .env with your DISCORD_TOKEN, APPLICATION_ID, and optional GUILD_ID for rapid testing
   ```

3. Build the project:
   ```bash
   cargo check
   cargo build
   ```

---

## Project Structure

```
.
├── .github/                  # GitHub workflows, issue templates, PR template
├── cloudflare/               # Cloudflare Worker & Activity proxy integration
├── scripts/                  # Automated VM installer (scripts/install.sh)
├── src/
│   ├── config.rs             # Configuration parser and environment reader
│   ├── decision.rs           # Multi-user conversation triage and decision model
│   ├── error.rs              # Structured error types using thiserror
│   ├── lib.rs                # Crate root
│   ├── main.rs               # Application entry point & serenity setup
│   ├── state.rs              # In-memory thread-safe state & session persistence
│   ├── discord/
│   │   ├── commands/         # Slash command handlers (/spawn, /model, /status, etc.)
│   │   ├── formatter.rs      # Discord 2000-char message chunker
│   │   ├── handler.rs        # Serenity event handler (messages, threads, slash commands)
│   │   └── topic.rs          # Channel topic updater and tool synchronizer
│   └── zeron/
│       ├── client.rs         # Async JSON-RPC stdio MCP client
│       └── models.rs         # Serde models for Zeron MCP protocol
└── tests/                    # Integration and unit tests
```

---

## Development Workflow

### Branching Strategy

- Create feature or bugfix branches from `main`.
- Branch naming convention:
  - `feat/description-of-feature`
  - `fix/description-of-bug`
  - `docs/description-of-docs-update`
  - `refactor/description-of-refactor`

### Coding Standards

We enforce idiomatic Rust conventions:

1. **Formatting:** Format your code using `rustfmt`:
   ```bash
   cargo fmt --check
   cargo fmt
   ```
2. **Clippy Lints:** Run the Clippy linter to catch common idioms and errors:
   ```bash
   cargo clippy --all-targets -- -D warnings
   ```
3. **Error Handling:** Use structured errors via `thiserror` (see `src/error.rs`) rather than `unwrap()` or `expect()` in library code.

### Running Tests

Ensure all unit tests and integration tests pass cleanly:

```bash
# Run unit and integration tests
cargo test

# Run tests with stdout output enabled
cargo test -- --nocapture
```

If adding new features, include unit tests covering expected behavior and edge cases in `tests/` or inline test modules.

---

## Submitting Changes

### Pull Request Process

1. **Check CI locally:** Ensure `cargo fmt --check`, `cargo clippy`, and `cargo test` pass before committing.
2. **Commit messages:** Write concise, descriptive commit messages following the Conventional Commits style:
   - `feat: add support for dynamic harness selection`
   - `fix: resolve boundary split when message ends in open code block`
   - `docs: update deployment instructions for systemd`
3. **Push & Open PR:** Push your branch and open a pull request against `main`. Fill out the [Pull Request Template](.github/pull_request_template.md).
4. **Code Review:** Maintainers and automated checks will review your pull request. Address feedback promptly.

---

## Security Disclosures

Please do **not** report security vulnerabilities via public GitHub issues. Refer to [SECURITY.md](SECURITY.md) for instructions on confidential disclosure.

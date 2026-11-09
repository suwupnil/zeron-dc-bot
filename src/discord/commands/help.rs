use crate::error::BotResult;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("help").description("Learn how to use the Zeron Discord Bridge Bot")
}

pub async fn run(ctx: &Context, command: &CommandInteraction) -> BotResult<()> {
    let help_text = "\
### 🚀 Zeron Discord Bridge Bot

Connect your Discord channels and threads to your local **Zeron** coding agents.

---
#### 🛠️ Slash Commands
- `/spawn <directory>`: Spawn a new project session linked to a host directory (creates directory if missing). Enables side chats, thought process tracking, and keeps channel topic populated with available Skills and MCP tools.
- `/close`: End the current session and stop monitoring the channel.
- `/resume [session_id]`: Resume monitoring a previously closed session in this channel, or attach to a specific session ID.
- `/model <harness> <model> [effort]`: Switch the active AI harness, model, and thinking effort for this session.
- `/effort <level>`: Switch the reasoning/thinking effort level for the active model.
- `/skills`: Ephemeral list of detected Skills across local agent harnesses.
- `/mcp`: Ephemeral list of configured MCP servers and tools.
- `/magpie`: Launch the Magpie Web UI Activity (or configure embedded activity link).
- `/status`: Inspect active chat session, origin, model, harness, directory, and monitoring state.
- `/interrupt`: Stop a running agent turn immediately.
- `/help`: Show this guide.

---
#### 🤖 Session Types
1. **Project Sessions (`/spawn <directory>`)**:
   - Designed for software development and engineering workflows.
   - Shows model thought process formatted as small text (`-#`).
   - Dynamically updates channel topic with the active model, skills, and MCP tools.
   - Supports context-forked side chats in Discord Threads.
2. **Casual Sessions (Tagging `@bot`)**:
   - Initialized the first time the bot is tagged in a channel without an active session.
   - Operates conversationally and human-like without code project attachment.

---
#### 🧠 Multi-User Decision Model
- In multi-user channels, a decision model analyzes the last 10 messages to determine if a message asks for assistant help or if users are talking among themselves.
- Explicitly tagging `@bot` or replying to the bot always guarantees an immediate response.
";

    let resp = CreateInteractionResponseMessage::new()
        .content(help_text)
        .ephemeral(true);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

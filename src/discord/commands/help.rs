use crate::error::BotResult;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("help")
        .description("Learn how to use the Zeron Discord Bridge Bot")
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
) -> BotResult<()> {
    let help_text = "\
### 🚀 Zeron Discord Bridge Bot

This bot connects your Discord channels and threads to your local **Zeron** coding agents. All operations execute strictly on your target host VM/machine.

---
#### 🛠️ Slash Commands
- `/dir <path>`: Set the host VM project directory for this channel or thread (includes path validation & autocomplete).
- `/model <name>`: Switch the AI model (shows dynamic autocompletions of all available models from `list_models`).
- `/mcp <server>`: Attach an MCP server to this conversation.
- `/skills <skill>`: Activate a specialized skill/workflow (Cloudflare, React best practices, HyperFrames, etc.).
- `/status`: Inspect active chat session, model, harness, directory, and thread hierarchy.
- `/reset`: Archive the current Zeron session and spawn a fresh one.
- `/interrupt`: Stop a running agent turn immediately.
- `/help`: Show this guide.

---
#### 🧵 Threads as Side Chats (Context Forking)
- When you create a **Discord Thread** inside any text channel, the bot creates a **Zeron Side Chat** (`kind: side`, parent set to the channel's main chat).
- The side chat **inherits full context** from the channel's main chat.
- You can ask questions, run exploratory tasks, or refactor code in threads without polluting or interrupting the main session!

---
#### 💬 Chatting
- Simply type a message in the channel or thread (or mention the bot).
- The bot will display a typing indicator while Zeron is working, and post the assistant's replies formatted with syntax-highlighted code blocks.
";

    let resp = CreateInteractionResponseMessage::new().content(help_text);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

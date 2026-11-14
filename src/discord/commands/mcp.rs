use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{CommandInteraction, Context, CreateCommand, EditInteractionResponse};

pub fn register() -> CreateCommand {
    CreateCommand::new("mcp").description("List available and active MCP servers")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer ephemeral response
    command.defer_ephemeral(&ctx.http).await?;

    let (_, mcps) = topic::get_available_skills_and_mcp(state).await;

    let content = if mcps.is_empty() {
        "🔌 No MCP servers currently discovered.".to_string()
    } else {
        format!(
            "🔌 **Available MCP Servers ({})**\n\n{}",
            mcps.len(),
            mcps.iter()
                .map(|m| format!("• `{}`", m))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let edit = EditInteractionResponse::new().content(content);
    command.edit_response(&ctx.http, edit).await?;

    Ok(())
}

use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("mcp")
        .description("List available and active MCP servers")
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let (_, mcps) = topic::get_available_skills_and_mcp(state).await;

    let content = if mcps.is_empty() {
        "🔌 No MCP servers currently discovered.".to_string()
    } else {
        format!(
            "🔌 **Available MCP Servers ({})**\n\n{}",
            mcps.len(),
            mcps
                .iter()
                .map(|m| format!("• `{}`", m))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let resp = CreateInteractionResponseMessage::new()
        .content(content)
        .ephemeral(true);

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

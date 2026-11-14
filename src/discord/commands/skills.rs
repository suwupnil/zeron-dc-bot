use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{CommandInteraction, Context, CreateCommand, EditInteractionResponse};

pub fn register() -> CreateCommand {
    CreateCommand::new("skills").description("List available and installed skills")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer ephemeral response
    command.defer_ephemeral(&ctx.http).await?;

    let (skills, _) = topic::get_available_skills_and_mcp(state).await;

    let content = if skills.is_empty() {
        "No skills available".to_string()
    } else {
        format!(
            "**Listed {} skill(s)**\n\n{}",
            skills.len(),
            skills
                .iter()
                .map(|s| format!("• `{}`", s))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let edit = EditInteractionResponse::new().content(content);
    command.edit_response(&ctx.http, edit).await?;

    Ok(())
}

use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("skills").description("List available and installed skills")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
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

    let resp = CreateInteractionResponseMessage::new()
        .content(content)
        .ephemeral(true);

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

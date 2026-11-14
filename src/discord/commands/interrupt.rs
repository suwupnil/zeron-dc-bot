use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{CommandInteraction, Context, CreateCommand, EditInteractionResponse};

pub fn register() -> CreateCommand {
    CreateCommand::new("interrupt").description("Stop agent")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer ephemeral response
    command.defer_ephemeral(&ctx.http).await?;

    let channel_id = command.channel_id.get();
    let session = state.get_session(channel_id).await;

    let content = match session {
        Some(s) => match state.zeron.interrupt_chat(&s.chat_id).await {
            Ok(_) => format!("🛑 **Turn Interrupted** for session `{}`.", s.chat_id),
            Err(e) => format!("⚠️ Could not interrupt turn: {}", e),
        },
        None => "❌ No active session found in this channel.".to_string(),
    };

    let edit = EditInteractionResponse::new().content(content);
    command.edit_response(&ctx.http, edit).await?;

    Ok(())
}

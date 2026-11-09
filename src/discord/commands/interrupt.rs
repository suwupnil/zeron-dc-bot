use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("interrupt")
        .description("Stop any currently running agent turn in this channel / thread")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    let channel_id = command.channel_id.get();
    let session = state.get_session(channel_id).await;

    let content = match session {
        Some(s) => match state.zeron.interrupt_chat(&s.chat_id).await {
            Ok(_) => format!("🛑 **Turn Interrupted** for session `{}`.", s.chat_id),
            Err(e) => format!("⚠️ Could not interrupt turn: {}", e),
        },
        None => "❌ No active session found in this channel.".to_string(),
    };

    let resp = CreateInteractionResponseMessage::new().content(content);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

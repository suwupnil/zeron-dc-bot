use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("close")
        .description("End the current Zeron session and stop monitoring this channel")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    let channel_id = command.channel_id.get();

    match state.close_session(channel_id).await {
        Ok(session) => {
            // Empty channel topic when no active session is linked
            let _ = topic::clear_channel_topic(ctx, channel_id).await;

            let resp = CreateInteractionResponseMessage::new().content(format!(
                "🔒 **Zeron Session Closed**\n\
                 - **Session ID:** `{}`\n\
                 - **Directory:** `{}`\n\
                 This channel is no longer monitored.\n\
                 You can resume monitoring this session anytime using `/resume`, or spawn a new session with `/spawn` or by @mentioning the bot.",
                session.chat_id, session.project_dir
            ));
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
        }
        Err(e) => {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("⚠️ Could not close session: {}", e));
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
        }
    }
    Ok(())
}

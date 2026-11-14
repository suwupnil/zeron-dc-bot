use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{CommandInteraction, Context, CreateCommand, EditInteractionResponse};

pub fn register() -> CreateCommand {
    CreateCommand::new("close")
        .description("End the current Zeron session and stop monitoring this channel")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer public response
    command.defer(&ctx.http).await?;

    let channel_id = command.channel_id.get();

    match state.close_session(channel_id).await {
        Ok(session) => {
            // Empty channel topic when no active session is linked
            let _ = topic::clear_channel_topic(ctx, channel_id).await;

            let edit = EditInteractionResponse::new().content(format!(
                "🔒 **Zeron Session Closed**\n\
                 - **Session ID:** `{}`\n\
                 - **Directory:** `{}`\n\
                 This channel is no longer monitored.\n\
                 You can resume monitoring this session anytime using `/resume`, or spawn a new session with `/spawn` or by @mentioning the bot.",
                session.chat_id, session.project_dir
            ));
            command.edit_response(&ctx.http, edit).await?;
        }
        Err(e) => {
            let edit = EditInteractionResponse::new()
                .content(format!("⚠️ Could not close session: {}", e));
            command.edit_response(&ctx.http, edit).await?;
        }
    }
    Ok(())
}

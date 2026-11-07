use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("resume")
        .description("Resume monitoring a previously closed Zeron session in this channel")
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let channel_id = command.channel_id.get();

    match state.resume_session(channel_id).await {
        Ok(session) => {
            let _ = topic::update_channel_topic(ctx, channel_id, state).await;

            let resp = CreateInteractionResponseMessage::new().content(format!(
                "▶️ **Zeron Session Resumed**\n\
                 - **Session ID:** `{}`\n\
                 - **Kind:** `{}`\n\
                 - **Directory:** `{}`\n\
                 - **Harness:** `{}`\n\
                 - **Active Model:** `{}`\n\
                 Monitoring has resumed for this channel.",
                session.chat_id,
                session.kind,
                session.project_dir,
                session.harness,
                session.model.as_deref().unwrap_or("harness default")
            ));
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
        }
        Err(e) => {
            let resp = CreateInteractionResponseMessage::new().content(format!(
                "⚠️ Could not resume session: {}\n\
                 Use `/spawn <directory>` or @mention the bot to create a new session.",
                e
            ));
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
        }
    }

    Ok(())
}

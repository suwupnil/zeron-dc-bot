use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("status")
        .description("View current Zeron session state, model, directory, and thread hierarchy")
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let channel_id = command.channel_id.get();
    let session = state.get_session(channel_id).await;

    let content = match session {
        Some(s) => {
            let live_chat = state.zeron.get_chat(&s.chat_id).await.ok();
            let live_status = live_chat
                .as_ref()
                .and_then(|c| c.status.as_deref())
                .unwrap_or("idle");

            let parent_info = if let Some(ref parent_id) = s.parent_chat_id {
                format!("\n- **Parent Chat:** `{}` (Branch / Side Chat)", parent_id)
            } else {
                "\n- **Hierarchy:** Main Channel Chat".to_string()
            };

            format!(
                "📊 **Zeron Session Status**\n\
                 - **Chat ID:** `{}`\n\
                 - **Kind:** `{}`{}\n\
                 - **Live Status:** `{}`\n\
                 - **Harness:** `{}`\n\
                 - **Active Model:** `{}`\n\
                 - **Project Directory:** `{}`\n\
                 - **Execution Target:** Local Host VM",
                s.chat_id,
                s.kind,
                parent_info,
                live_status,
                s.harness,
                s.model.as_deref().unwrap_or("harness default"),
                s.project_dir
            )
        }
        None => {
            "ℹ️ No active Zeron session for this channel yet. Send a message or use `/dir` to initialize one.".to_string()
        }
    };

    let resp = CreateInteractionResponseMessage::new().content(content);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

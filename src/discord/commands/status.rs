use crate::error::BotResult;
use crate::state::{AppState, SessionOrigin};
use serenity::all::{CommandInteraction, Context, CreateCommand, EditInteractionResponse};

pub fn register() -> CreateCommand {
    CreateCommand::new("status").description("Get info")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer ephemeral response
    command.defer_ephemeral(&ctx.http).await?;

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

            let origin_desc = match s.origin {
                SessionOrigin::Spawn => "Project (/spawn) - side chats & thought process enabled",
                SessionOrigin::Tag => "Casual (@tag) - conversational & human-like",
            };

            let monitor_status = if s.is_closed {
                "🔴Unmonitored (use `/resume` or `/spawn` to enable)"
            } else {
                "🟢 Active"
            };

            let decision_model_desc = if state.decision.is_configured() {
                format!("Cloudflare Workers AI (`{}`)", state.decision.model)
            } else {
                "Fallback heuristic (Cloudflare unconfigured)".to_string()
            };

            format!(
                "📊 **Zeron Session Status**\n\
                 - **Chat ID:** `{}`\n\
                 - **Origin:** {}\n\
                 - **Monitoring:** {}\n\
                 - **Kind:** `{}`{}\n\
                 - **Live Status:** `{}`\n\
                 - **Harness:** `{}`\n\
                 - **Active Model:** `{}`\n\
                 - **Project Directory:** `{}`\n\
                 - **Decision Model:** {}",
                s.chat_id,
                origin_desc,
                monitor_status,
                s.kind,
                parent_info,
                live_status,
                s.harness,
                s.model.as_deref().unwrap_or("harness default"),
                if s.project_dir.is_empty() { "No directory" } else { &s.project_dir },
                decision_model_desc,
            )
        }
        None => {
            "ℹ️ No Zeron session linked to this channel yet.\n\
             Use `/spawn <directory>` to start a project session, or @mention the bot to start a casual conversation.".to_string()
        }
    };

    let edit = EditInteractionResponse::new().content(content);
    command.edit_response(&ctx.http, edit).await?;

    Ok(())
}

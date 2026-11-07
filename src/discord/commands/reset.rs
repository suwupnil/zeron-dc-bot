use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    Channel, CommandInteraction, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("reset")
        .description("Reset and archive the current Zeron session and start a fresh session")
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let channel_id = command.channel_id.get();

    let channel_info = command.channel_id.to_channel(&ctx.http).await.ok();
    let (name, is_thread, parent_channel_id) = match channel_info {
        Some(Channel::Guild(gc)) => {
            let is_th = matches!(
                gc.kind,
                serenity::model::channel::ChannelType::PublicThread
                    | serenity::model::channel::ChannelType::PrivateThread
                    | serenity::model::channel::ChannelType::NewsThread
            );
            let parent_id = gc.parent_id.map(|p| p.get());
            (gc.name, is_th, parent_id)
        }
        _ => ("chat".to_string(), false, None),
    };

    let session = state
        .reset_session(
            channel_id,
            &name,
            command.guild_id.map(|g| g.get()),
            is_thread,
            parent_channel_id,
        )
        .await?;

    let resp_text = format!(
        "🔄 **Session Reset Complete**\n- **New Session ID:** `{}`\n- **Kind:** `{}`\n- **Working Dir:** `{}`",
        session.chat_id, session.kind, session.project_dir
    );

    let resp = CreateInteractionResponseMessage::new().content(resp_text);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

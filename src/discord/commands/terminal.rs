use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use tracing::{info, warn};

pub fn register() -> CreateCommand {
    CreateCommand::new("terminal").description("Open a VM terminal session in Discord")
}

pub async fn notify_worker_launch_mode(state: &AppState, channel_id: u64, mode: &str) {
    let worker_url = match &state.config.magpie_url {
        Some(url) => url.clone(),
        None => return,
    };

    let endpoint = format!("{}/__set_launch_mode", worker_url.trim_end_matches('/'));
    let secret = state
        .config
        .update_secret
        .as_deref()
        .unwrap_or("suwu-magpie-token-2026");

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "channel_id": channel_id.to_string(),
        "mode": mode,
    });

    if let Err(e) = client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", secret))
        .json(&body)
        .timeout(std::time::Duration::from_millis(1500))
        .send()
        .await
    {
        warn!("Failed to coordinate activity launch mode with worker: {}", e);
    }
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    info!(
        "User {} requested /terminal activity in channel {}",
        command.user.name, command.channel_id
    );

    // Notify worker of launch mode for this channel so the iframe routes to terminal
    notify_worker_launch_mode(state, command.channel_id.get(), "terminal").await;

    match command
        .create_response(&ctx.http, CreateInteractionResponse::LaunchActivity)
        .await
    {
        Ok(_) => {
            info!("Successfully launched terminal Activity");
            Ok(())
        }
        Err(e) => {
            warn!("LaunchActivity rejected for /terminal: {}", e);

            let app_id = state
                .config
                .application_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| command.application_id.to_string());

            let embed = CreateEmbed::new()
                .title("🖥️  VM Terminal — Activity Setup Required")
                .description(format!(
                    "To enable the terminal as an embedded Discord Activity:\n\n\
                    1. Open the [Discord Developer Portal](https://discord.com/developers/applications/{}/activities).\n\
                    2. Enable **Activities** and add a URL mapping:\n\
                       - **Prefix:** `/terminal`\n\
                       - **Target:** your Cloudflare Worker URL *(without `https://`)*\n\
                    3. Run `/terminal` again once configured.",
                    app_id
                ))
                .color(0x2b2d31);

            let resp = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            let _ = command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_command_registration() {
        let cmd = register();
        let val = serde_json::to_value(&cmd).unwrap();
        assert_eq!(val["name"], "terminal");
        assert_eq!(val["description"], "Open a VM terminal session in Discord");
    }
}

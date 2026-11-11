use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use tracing::{info, warn};

pub fn register() -> CreateCommand {
    CreateCommand::new("magpie").description("Launch the Magpie Web UI Activity")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    info!(
        "User {} requested /magpie activity in channel {}",
        command.user.name, command.channel_id
    );

    // Attempt to respond with Discord's native LAUNCH_ACTIVITY interaction response (type 12)
    match command
        .create_response(&ctx.http, CreateInteractionResponse::LaunchActivity)
        .await
    {
        Ok(_) => {
            info!("Successfully launched Discord Activity for /magpie");
            Ok(())
        }
        Err(e) => {
            warn!(
                "Native LaunchActivity response rejected (embedded activity not configured in Developer Portal yet): {}",
                e
            );

            let app_id = state
                .config
                .application_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "YOUR_APP_ID".to_string());

            let mut desc = String::new();
            if let Some(magpie_url) = &state.config.magpie_url {
                desc.push_str(&format!("🌐 **Direct Web UI:** [{0}]({0})\n\n", magpie_url));
            }

            desc.push_str(&format!(
                "To launch Magpie directly as an embedded Discord Activity:\n\n\
                1. Open the [Discord Developer Portal](https://discord.com/developers/applications/{}/activities).\n\
                2. Click **Get Started** under Activities.\n\
                3. Under **URL Mappings**, set `/` to your public HTTPS domain for Magpie.\n\
                4. Once saved, `/magpie` will launch the embedded Magpie UI seamlessly right inside Discord!\n\n\
                *Reference: [usemagpie.ai](https://usemagpie.ai)*",
                app_id
            ));

            let embed = CreateEmbed::new()
                .title("🐦 Magpie Web UI — Activity Setup")
                .description(desc)
                .color(0x5865F2);

            let resp = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            if let Err(msg_err) = command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await
            {
                warn!("Failed to send fallback response for /magpie: {}", msg_err);
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magpie_command_registration() {
        let cmd = register();
        let val = serde_json::to_value(&cmd).unwrap();
        assert_eq!(val["name"], "magpie");
        assert_eq!(val["description"], "Launch the Magpie Web UI Activity");
    }
}

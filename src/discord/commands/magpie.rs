use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage,
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

    // Notify worker of launch mode for this channel so the iframe routes to magpie
    if let Err(e) = crate::discord::commands::terminal::notify_worker_launch_mode(
        state,
        command.channel_id.get(),
        "magpie",
    )
    .await
    {
        warn!(
            "Aborting /magpie launch due to worker coordination failure: {}",
            e
        );

        let embed = CreateEmbed::new()
            .title("❌ Magpie Launch Failed")
            .description(format!(
                "Could not set activity launch mode to **magpie**.\n\n\
                **Error:** {}\n\n\
                *Please verify that `MAGPIE_URL` and `UPDATE_SECRET` are properly configured in `.env` and match your Cloudflare Worker configuration.*",
                e
            ))
            .color(0xED4245);

        let resp = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true);

        if let Err(msg_err) = command
            .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
            .await
        {
            warn!("Failed to send error response for /magpie: {}", msg_err);
        }

        return Ok(());
    }

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
                .embed(embed.clone())
                .ephemeral(true);

            if let Err(msg_err) = command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await
            {
                warn!("Failed to send initial fallback response for /magpie: {}. Attempting followup...", msg_err);
                let followup = CreateInteractionResponseFollowup::new()
                    .embed(embed)
                    .ephemeral(true);
                let _ = command.create_followup(&ctx.http, followup).await;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discord::commands::terminal::{notify_worker_launch_mode_client, LaunchModeError};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_magpie_command_registration() {
        let cmd = register();
        let val = serde_json::to_value(&cmd).unwrap();
        assert_eq!(val["name"], "magpie");
        assert_eq!(val["description"], "Launch the Magpie Web UI Activity");
    }

    #[tokio::test]
    async fn test_magpie_notify_worker_missing_magpie_url() {
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, None, Some("secret"), 98765, "magpie").await;
        assert!(matches!(res, Err(LaunchModeError::MissingMagpieUrl)));

        let res_empty = notify_worker_launch_mode_client(&client, Some("   "), Some("secret"), 98765, "magpie").await;
        assert!(matches!(res_empty, Err(LaunchModeError::MissingMagpieUrl)));
    }

    #[tokio::test]
    async fn test_magpie_notify_worker_missing_update_secret() {
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some("http://127.0.0.1"), None, 98765, "magpie").await;
        assert!(matches!(res, Err(LaunchModeError::MissingUpdateSecret)));

        let res_empty = notify_worker_launch_mode_client(&client, Some("http://127.0.0.1"), Some("   "), 98765, "magpie").await;
        assert!(matches!(res_empty, Err(LaunchModeError::MissingUpdateSecret)));
    }

    async fn spawn_mock_server(
        status_code: u16,
        status_text: &'static str,
        response_body: &'static str,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = socket.read(&mut buf).await;
                let http_response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status_code,
                    status_text,
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(http_response.as_bytes()).await;
                let _ = socket.flush().await;
            }
        });
        (format!("http://{}", addr), handle)
    }

    #[tokio::test]
    async fn test_magpie_notify_worker_success() {
        let (base_url, handle) = spawn_mock_server(200, "OK", r#"{"success":true,"channel_id":"98765","mode":"magpie"}"#).await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret-token"), 98765, "magpie").await;
        let _ = handle.await;
        assert!(res.is_ok(), "Expected Ok(()), got {:?}", res);
    }

    #[tokio::test]
    async fn test_magpie_notify_worker_rejected_unauthorized() {
        let (base_url, handle) = spawn_mock_server(401, "Unauthorized", "Unauthorized").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("bad-secret"), 98765, "magpie").await;
        let _ = handle.await;
        match res {
            Err(LaunchModeError::Rejected { status, body }) => {
                assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
                assert_eq!(body, "Unauthorized");
            }
            other => panic!("Expected Rejected with 401, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_magpie_notify_worker_rejected_service_unavailable() {
        let (base_url, handle) = spawn_mock_server(503, "Service Unavailable", "Mode store unavailable").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret"), 98765, "magpie").await;
        let _ = handle.await;
        match res {
            Err(LaunchModeError::Rejected { status, body }) => {
                assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body, "Mode store unavailable");
            }
            other => panic!("Expected Rejected with 503, got {:?}", other),
        }
    }
}

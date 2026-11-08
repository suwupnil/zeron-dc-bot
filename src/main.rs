use serenity::all::GatewayIntents;
use serenity::Client;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use zeron_dc_bot::config::Config;
use zeron_dc_bot::discord::handler::Handler;
use zeron_dc_bot::state::AppState;
use zeron_dc_bot::zeron::ZeronClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Zeron Discord Bridge Bot...");

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            eprintln!("\n⚠️  Please make sure your .env file is set up with DISCORD_TOKEN.\n");
            return Err(e.into());
        }
    };

    info!(
        "Connecting to local Zeron daemon at: {:?}",
        config.zeron_path
    );
    let zeron_client = ZeronClient::new(&config.zeron_path);

    // Initialize state and load persisted sessions
    let state = AppState::new(config.clone(), zeron_client.clone());
    if let Err(e) = state.load_sessions().await {
        tracing::warn!("Could not load previous sessions: {}", e);
    }

    // Attempt initial test connection to Zeron
    match zeron_client.connect().await {
        Ok(_) => {
            info!("Connected to Zeron MCP successfully!");
            if let Ok(harnesses) = zeron_client.list_harnesses().await {
                let connected_names: Vec<String> = harnesses
                    .iter()
                    .filter(|h| h.connected)
                    .map(|h| h.name.clone())
                    .collect();
                info!("Available connected harnesses: {:?}", connected_names);
            }
        }
        Err(e) => {
            tracing::warn!(
                "Initial connection to Zeron MCP daemon deferred: {}. Will reconnect upon first activity.",
                e
            );
        }
    }

    // Configure Discord Gateway Intents
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::DIRECT_MESSAGES;

    let handler = Handler::new(state.clone());

    let mut client = Client::builder(&config.discord_token, intents)
        .event_handler(handler)
        .await?;

    info!("Starting Discord gateway listener...");

    tokio::select! {
        res = client.start() => {
            if let Err(e) = res {
                error!("Discord client encountered an error: {:?}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal (Ctrl+C). Saving sessions and exiting gracefully...");
            let _ = state.save_sessions().await;
        }
    }

    info!("Zeron Discord Bridge Bot shutdown complete.");
    Ok(())
}

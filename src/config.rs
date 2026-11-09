use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub discord_token: String,
    pub application_id: Option<u64>,
    pub guild_id: Option<u64>,
    pub zeron_path: PathBuf,
    pub default_harness: String,
    pub default_model: Option<String>,
    pub default_project_dir: PathBuf,
    pub auto_create_sessions: bool,
    pub command_prefix: String,
    pub session_storage_path: PathBuf,
    pub cloudflare_account_id: Option<String>,
    pub cloudflare_api_token: Option<String>,
    pub cloudflare_model: String,
    pub cloudflare_ai_endpoint: Option<String>,
    pub magpie_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Load .env file if available
        let _ = dotenvy::dotenv();

        let discord_token = std::env::var("DISCORD_TOKEN")
            .context("DISCORD_TOKEN environment variable is required")?;

        let application_id = std::env::var("APPLICATION_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        let guild_id = std::env::var("GUILD_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        let zeron_path = std::env::var("ZERON_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let default_mac = PathBuf::from("/Applications/Zeron.app/Contents/MacOS/zeron");
                if default_mac.exists() {
                    default_mac
                } else {
                    PathBuf::from("zeron")
                }
            });

        let default_harness =
            std::env::var("DEFAULT_HARNESS").unwrap_or_else(|_| "codex".to_string());

        let default_model = std::env::var("DEFAULT_MODEL").ok();

        let default_project_dir = std::env::var("DEFAULT_PROJECT_DIR")
            .map(|s| {
                let expanded = shellexpand::tilde(&s);
                PathBuf::from(expanded.as_ref())
            })
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let auto_create_sessions = std::env::var("AUTO_CREATE_SESSIONS")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true);

        let command_prefix = std::env::var("COMMAND_PREFIX").unwrap_or_else(|_| "!z".to_string());

        let session_storage_path = std::env::var("SESSION_STORAGE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./sessions.json"));

        let cloudflare_account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID").ok();
        let cloudflare_api_token = std::env::var("CLOUDFLARE_API_TOKEN").ok();
        let cloudflare_model = std::env::var("CLOUDFLARE_MODEL")
            .unwrap_or_else(|_| "@cf/cloudflare/clef-flash".to_string());
        let cloudflare_ai_endpoint = std::env::var("CLOUDFLARE_AI_ENDPOINT").ok();
        let magpie_url = std::env::var("MAGPIE_URL").ok();

        Ok(Self {
            discord_token,
            application_id,
            guild_id,
            zeron_path,
            default_harness,
            default_model,
            default_project_dir,
            auto_create_sessions,
            command_prefix,
            session_storage_path,
            cloudflare_account_id,
            cloudflare_api_token,
            cloudflare_model,
            cloudflare_ai_endpoint,
            magpie_url,
        })
    }
}

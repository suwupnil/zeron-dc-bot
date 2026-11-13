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
    pub cloudflare_refresh_token: Option<String>,
    pub cloudflare_model: String,
    pub cloudflare_ai_endpoint: Option<String>,
    pub magpie_url: Option<String>,
    pub update_secret: Option<String>,
    pub media_cache_dir: PathBuf,
    pub media_max_image_bytes: u64,
    pub media_max_audio_bytes: u64,
    pub media_max_file_bytes: u64,
    pub ffmpeg_path: String,
    pub stt_provider: String,
    pub stt_model: String,
    pub groq_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub ocr_provider: String,
    pub ocr_model: String,
    pub tesseract_path: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_env_internal(true)
    }

    pub fn from_env_no_dotenv() -> Result<Self> {
        Self::from_env_internal(false)
    }

    fn from_env_internal(load_dotenv: bool) -> Result<Self> {
        if load_dotenv {
            let _ = dotenvy::dotenv();
        }

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
        let cloudflare_refresh_token = std::env::var("CLOUDFLARE_REFRESH_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let cloudflare_model = std::env::var("CLOUDFLARE_MODEL")
            .unwrap_or_else(|_| "@cf/cloudflare/clef-flash".to_string());
        let cloudflare_ai_endpoint = std::env::var("CLOUDFLARE_AI_ENDPOINT").ok();
        let magpie_url = std::env::var("MAGPIE_URL").ok();
        let update_secret = std::env::var("UPDATE_SECRET").ok();

        let media_cache_dir = std::env::var("MEDIA_CACHE_DIR")
            .map(|s| {
                let expanded = shellexpand::tilde(&s);
                PathBuf::from(expanded.as_ref())
            })
            .unwrap_or_else(|_| PathBuf::from("./media_cache"));

        let media_max_image_bytes = std::env::var("MEDIA_MAX_IMAGE_BYTES")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(25_000_000);

        let media_max_audio_bytes = std::env::var("MEDIA_MAX_AUDIO_BYTES")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(25_000_000);

        let media_max_file_bytes = std::env::var("MEDIA_MAX_FILE_BYTES")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(50_000_000);

        let ffmpeg_path = std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".to_string());

        let stt_provider =
            std::env::var("STT_PROVIDER").unwrap_or_else(|_| "cloudflare".to_string());

        let stt_model = std::env::var("STT_MODEL").unwrap_or_else(|_| {
            match stt_provider.to_lowercase().as_str() {
                "groq" => "whisper-large-v3-turbo".to_string(),
                "openai" => "whisper-1".to_string(),
                _ => "@cf/openai/whisper".to_string(),
            }
        });

        let groq_api_key = std::env::var("GROQ_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let ocr_provider =
            std::env::var("OCR_PROVIDER").unwrap_or_else(|_| "cloudflare".to_string());

        let ocr_model = std::env::var("OCR_MODEL")
            .unwrap_or_else(|_| "@cf/llava-hf/llava-1.5-7b-hf".to_string());

        let tesseract_path =
            std::env::var("TESSERACT_PATH").unwrap_or_else(|_| "tesseract".to_string());

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
            cloudflare_refresh_token,
            cloudflare_model,
            cloudflare_ai_endpoint,
            magpie_url,
            update_secret,
            media_cache_dir,
            media_max_image_bytes,
            media_max_audio_bytes,
            media_max_file_bytes,
            ffmpeg_path,
            stt_provider,
            stt_model,
            groq_api_key,
            openai_api_key,
            ocr_provider,
            ocr_model,
            tesseract_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parsing() {
        std::env::set_var("DISCORD_TOKEN", "test_token");
        std::env::set_var("APPLICATION_ID", "123456789012345678");
        std::env::set_var("DEFAULT_PROJECT_DIR", "/tmp/projects");
        std::env::set_var("MAGPIE_URL", "https://activity.example.com");
        std::env::set_var("UPDATE_SECRET", "super-secret-token");
        std::env::set_var("STT_PROVIDER", "cloudflare");
        std::env::set_var("STT_MODEL", "@cf/openai/whisper");
        std::env::remove_var("GROQ_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("CLOUDFLARE_REFRESH_TOKEN");

        let config = Config::from_env_no_dotenv().expect("Config should parse cleanly");
        assert_eq!(config.discord_token, "test_token");
        assert_eq!(config.application_id, Some(123456789012345678));
        assert_eq!(config.default_project_dir, PathBuf::from("/tmp/projects"));
        assert_eq!(
            config.magpie_url,
            Some("https://activity.example.com".to_string())
        );
        assert_eq!(
            config.update_secret,
            Some("super-secret-token".to_string())
        );
        assert_eq!(config.cloudflare_model, "@cf/cloudflare/clef-flash");
        assert_eq!(config.media_cache_dir, PathBuf::from("./media_cache"));
        assert_eq!(config.media_max_image_bytes, 25_000_000);
        assert_eq!(config.media_max_audio_bytes, 25_000_000);
        assert_eq!(config.media_max_file_bytes, 50_000_000);
        assert_eq!(config.ffmpeg_path, "ffmpeg");
        assert_eq!(config.stt_provider, "cloudflare");
        assert_eq!(config.stt_model, "@cf/openai/whisper");
        assert_eq!(config.groq_api_key, None);
        assert_eq!(config.openai_api_key, None);
        assert_eq!(config.cloudflare_refresh_token, None);
        assert_eq!(config.ocr_provider, "cloudflare");
        assert_eq!(config.ocr_model, "@cf/llava-hf/llava-1.5-7b-hf");
        assert_eq!(config.tesseract_path, "tesseract");

        // Test Groq STT parsing
        std::env::set_var("STT_PROVIDER", "groq");
        std::env::remove_var("STT_MODEL");
        std::env::set_var("GROQ_API_KEY", "gsk_test");

        let config_groq = Config::from_env_no_dotenv().expect("Config should parse cleanly");
        assert_eq!(config_groq.stt_provider, "groq");
        assert_eq!(config_groq.stt_model, "whisper-large-v3-turbo");
        assert_eq!(config_groq.groq_api_key, Some("gsk_test".to_string()));
    }
}

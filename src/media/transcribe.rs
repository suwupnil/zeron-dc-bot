use crate::config::Config;
use crate::error::BotResult;
use base64::Engine;
use std::path::Path;
use tracing::{debug, warn};

pub const TRANSCRIPTION_FALLBACK: &str = "[Voice message — transcription unavailable]";

async fn transcribe_with_groq(
    api_key: &str,
    model: &str,
    audio_bytes: &[u8],
    client: &reqwest::Client,
) -> Option<String> {
    debug!("Attempting Groq Whisper transcription with model {}...", model);
    let file_part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(Vec::new()));

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_string());

    let resp = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(text) = body.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            None
        }
        Ok(r) => {
            warn!("Groq Whisper STT returned HTTP status: {}", r.status());
            None
        }
        Err(e) => {
            warn!("Network error calling Groq Whisper: {}", e);
            None
        }
    }
}

async fn transcribe_with_cloudflare(
    account_id: &str,
    api_token: &str,
    model: &str,
    endpoint_override: Option<&str>,
    audio_bytes: &[u8],
    client: &reqwest::Client,
) -> Option<String> {
    let endpoint = endpoint_override.map(String::from).unwrap_or_else(|| {
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            account_id, model
        )
    });

    debug!(
        "Sending STT request to Cloudflare Whisper model {} at {}",
        model, endpoint
    );

    let b64_audio = base64::prelude::BASE64_STANDARD.encode(audio_bytes);
    let payload = serde_json::json!({
        "audio": b64_audio
    });

    let resp = client
        .post(&endpoint)
        .bearer_auth(api_token)
        .json(&payload)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            let text = body
                .get("result")
                .and_then(|res| res.get("text"))
                .or_else(|| body.get("text"))
                .and_then(|v| v.as_str());

            if let Some(t) = text {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            None
        }
        Ok(r) => {
            warn!(
                "Cloudflare Whisper STT returned HTTP status: {}",
                r.status()
            );
            None
        }
        Err(e) => {
            warn!("Network error calling Cloudflare Whisper: {}", e);
            None
        }
    }
}

async fn transcribe_with_openai(
    api_key: &str,
    model: &str,
    audio_bytes: &[u8],
    client: &reqwest::Client,
) -> Option<String> {
    debug!("Attempting OpenAI Whisper transcription...");
    let file_part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(Vec::new()));

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_string());

    let resp = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(text) = body.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            None
        }
        Ok(r) => {
            warn!("OpenAI Whisper STT returned HTTP status: {}", r.status());
            None
        }
        Err(e) => {
            warn!("Network error calling OpenAI Whisper: {}", e);
            None
        }
    }
}

/// Transcribes audio using Groq, Cloudflare Workers AI Whisper, or OpenAI Whisper.
/// Returns a fallback string if transcription fails or services are unavailable.
pub async fn transcribe_audio(
    audio_path: &Path,
    config: &Config,
    client: &reqwest::Client,
) -> BotResult<String> {
    let provider = config.stt_provider.to_lowercase();

    if provider == "none" {
        debug!("STT skipped: STT_PROVIDER is set to 'none'");
        return Ok(TRANSCRIPTION_FALLBACK.to_string());
    }

    let audio_bytes = match tokio::fs::read(audio_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Failed to read audio file at {:?} for STT: {}", audio_path, e);
            return Ok(TRANSCRIPTION_FALLBACK.to_string());
        }
    };

    if audio_bytes.is_empty() {
        warn!("Audio file at {:?} is empty, skipping STT", audio_path);
        return Ok(TRANSCRIPTION_FALLBACK.to_string());
    }

    // 1. Try primary provider according to STT_PROVIDER
    match provider.as_str() {
        "groq" => {
            if let Some(ref api_key) = config.groq_api_key {
                let model = if config.stt_model.starts_with("@cf/") || config.stt_model == "whisper-1" {
                    "whisper-large-v3-turbo"
                } else {
                    &config.stt_model
                };
                if let Some(text) = transcribe_with_groq(api_key, model, &audio_bytes, client).await {
                    return Ok(text);
                }
            } else {
                warn!("GROQ_API_KEY is missing while STT_PROVIDER is set to 'groq'");
            }
        }
        "cloudflare" => {
            if let (Some(account_id), Some(api_token)) = (
                config.cloudflare_account_id.as_deref(),
                config.cloudflare_api_token.as_deref(),
            ) {
                if let Some(text) = transcribe_with_cloudflare(
                    account_id,
                    api_token,
                    &config.stt_model,
                    config.cloudflare_ai_endpoint.as_deref(),
                    &audio_bytes,
                    client,
                )
                .await
                {
                    return Ok(text);
                }
            } else {
                warn!("Cloudflare STT credentials missing while STT_PROVIDER is set to 'cloudflare'");
            }
        }
        "openai" => {
            if let Some(ref api_key) = config.openai_api_key {
                let model = if config.stt_model.starts_with("@cf/") {
                    "whisper-1"
                } else {
                    &config.stt_model
                };
                if let Some(text) = transcribe_with_openai(api_key, model, &audio_bytes, client).await {
                    return Ok(text);
                }
            } else {
                warn!("OPENAI_API_KEY is missing while STT_PROVIDER is set to 'openai'");
            }
        }
        unknown => {
            warn!("Unknown STT_PROVIDER '{}', checking available providers...", unknown);
        }
    }

    // 2. Fallbacks: Try any other configured provider
    if provider != "groq" {
        if let Some(ref api_key) = config.groq_api_key {
            debug!("Attempting Groq STT fallback...");
            if let Some(text) = transcribe_with_groq(api_key, "whisper-large-v3-turbo", &audio_bytes, client).await {
                return Ok(text);
            }
        }
    }

    if provider != "cloudflare" {
        if let (Some(account_id), Some(api_token)) = (
            config.cloudflare_account_id.as_deref(),
            config.cloudflare_api_token.as_deref(),
        ) {
            debug!("Attempting Cloudflare STT fallback...");
            let model = if config.stt_model.starts_with("@cf/") {
                &config.stt_model
            } else {
                "@cf/openai/whisper"
            };
            if let Some(text) = transcribe_with_cloudflare(
                account_id,
                api_token,
                model,
                config.cloudflare_ai_endpoint.as_deref(),
                &audio_bytes,
                client,
            )
            .await
            {
                return Ok(text);
            }
        }
    }

    if provider != "openai" {
        if let Some(ref api_key) = config.openai_api_key {
            debug!("Attempting OpenAI STT fallback...");
            if let Some(text) = transcribe_with_openai(api_key, "whisper-1", &audio_bytes, client).await {
                return Ok(text);
            }
        }
    }

    Ok(TRANSCRIPTION_FALLBACK.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_stt_provider_none() {
        let mut config = Config::from_env().unwrap_or_else(|_| panic!("Failed to load config"));
        config.stt_provider = "none".to_string();
        let client = reqwest::Client::new();
        let path = PathBuf::from("audio.wav");

        let res = transcribe_audio(&path, &config, &client).await.unwrap();
        assert_eq!(res, TRANSCRIPTION_FALLBACK);
    }

    #[tokio::test]
    async fn test_stt_missing_file_fallback() {
        let config = Config::from_env().unwrap_or_else(|_| panic!("Failed to load config"));
        let client = reqwest::Client::new();
        let path = PathBuf::from("nonexistent_audio_file.wav");

        let res = transcribe_audio(&path, &config, &client).await.unwrap();
        assert_eq!(res, TRANSCRIPTION_FALLBACK);
    }

    #[tokio::test]
    async fn test_stt_groq_missing_file_fallback() {
        let mut config = Config::from_env().unwrap_or_else(|_| panic!("Failed to load config"));
        config.stt_provider = "groq".to_string();
        config.groq_api_key = Some("dummy_key".to_string());
        let client = reqwest::Client::new();
        let path = PathBuf::from("nonexistent_audio_file.wav");

        let res = transcribe_audio(&path, &config, &client).await.unwrap();
        assert_eq!(res, TRANSCRIPTION_FALLBACK);
    }
}

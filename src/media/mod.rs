pub mod ocr;
pub mod prompt;
pub mod transcribe;

pub use prompt::build_zeron_prompt;

use crate::config::Config;
use crate::error::{BotError, BotResult};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Represents a media or file attachment extracted from a Discord message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaAttachment {
    Image {
        path: PathBuf,
        name: String,
        ocr_text: Option<String>,
    },
    Audio {
        path: PathBuf,
        name: String,
        transcript: String,
    },
    File {
        path: PathBuf,
        name: String,
        mime: String,
    },
}

impl MediaAttachment {
    pub fn path(&self) -> &Path {
        match self {
            Self::Image { path, .. } => path,
            Self::Audio { path, .. } => path,
            Self::File { path, .. } => path,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Image { name, .. } => name,
            Self::Audio { name, .. } => name,
            Self::File { name, .. } => name,
        }
    }
}

/// Checks whether an attachment is a video based on MIME or extension.
pub fn is_video_mime_or_ext(filename: &str, content_type: Option<&str>) -> bool {
    if let Some(ct) = content_type {
        if ct.to_lowercase().starts_with("video/") {
            return true;
        }
    }
    let lower = filename.to_lowercase();
    let video_extensions = [
        ".mp4", ".mov", ".webm", ".mkv", ".avi", ".wmv", ".flv", ".m4v", ".3gp", ".ogv",
    ];
    video_extensions.iter().any(|ext| lower.ends_with(ext))
}

/// Checks if a Serenity Discord message contains any video attachments.
pub fn has_video_attachments(msg: &serenity::all::Message) -> bool {
    msg.attachments
        .iter()
        .any(|att| is_video_mime_or_ext(&att.filename, att.content_type.as_deref()))
}

/// Checks whether an attachment is an audio file based on MIME or extension.
pub fn is_audio_mime_or_ext(filename: &str, content_type: Option<&str>) -> bool {
    if let Some(ct) = content_type {
        if ct.to_lowercase().starts_with("audio/") {
            return true;
        }
    }
    let lower = filename.to_lowercase();
    let audio_extensions = [
        ".ogg", ".oga", ".opus", ".wav", ".mp3", ".m4a", ".aac", ".flac", ".weba", ".wma",
    ];
    audio_extensions.iter().any(|ext| lower.ends_with(ext))
}

/// Checks whether an attachment is an image based on MIME or extension.
pub fn is_image_mime_or_ext(filename: &str, content_type: Option<&str>) -> bool {
    if let Some(ct) = content_type {
        if ct.to_lowercase().starts_with("image/") {
            return true;
        }
    }
    let lower = filename.to_lowercase();
    let image_extensions = [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff", ".svg", ".ico",
    ];
    image_extensions.iter().any(|ext| lower.ends_with(ext))
}

/// Sanitizes a filename to avoid path traversal, null bytes, and unsafe characters.
pub fn sanitize_filename(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    let base = Path::new(&normalized)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);

    let mut sanitized = String::new();
    for ch in base.chars() {
        if ch.is_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    let result = sanitized
        .trim_matches(|c| c == '.' || c == '_' || c == '-')
        .to_string();
    if result.is_empty() {
        "attachment.bin".to_string()
    } else {
        result
    }
}

/// Converts audio (such as OGG/Opus) to 16kHz mono WAV using ffmpeg CLI.
pub async fn convert_audio_to_wav_16k(
    ffmpeg_path: &str,
    input_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output_path)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn ffmpeg at '{}': {}", ffmpeg_path, e))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg conversion failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Downloads and processes all message attachments and stickers under
/// `cache_dir/<channel_id>/<message_id>/<filename>`, enforcing size limits,
/// sanitizing names, running STT / OCR, and rejecting videos.
pub async fn extract_attachments(
    msg: &serenity::all::Message,
    config: &Config,
    client: &reqwest::Client,
) -> BotResult<Vec<MediaAttachment>> {
    let channel_id = msg.channel_id.get();
    let message_id = msg.id.get();
    let base_dir = config
        .media_cache_dir
        .join(channel_id.to_string())
        .join(message_id.to_string());

    tokio::fs::create_dir_all(&base_dir)
        .await
        .map_err(BotError::Io)?;

    let mut results = Vec::new();

    // 1. Process regular Discord attachments
    for att in &msg.attachments {
        if is_video_mime_or_ext(&att.filename, att.content_type.as_deref()) {
            warn!(
                "Video attachment '{}' rejected/skipped: video processing is out of scope",
                att.filename
            );
            continue;
        }

        let is_img = is_image_mime_or_ext(&att.filename, att.content_type.as_deref());
        let is_aud = is_audio_mime_or_ext(&att.filename, att.content_type.as_deref());

        let (limit, desc) = if is_img {
            (config.media_max_image_bytes, "Image")
        } else if is_aud {
            (config.media_max_audio_bytes, "Audio")
        } else {
            (config.media_max_file_bytes, "File")
        };

        if (att.size as u64) > limit {
            warn!(
                "{} attachment '{}' ({} bytes) exceeds limit of {} bytes, skipping",
                desc, att.filename, att.size, limit
            );
            continue;
        }

        let safe_name = sanitize_filename(&att.filename);
        let target_path = base_dir.join(&safe_name);

        debug!(
            "Downloading attachment '{}' ({}) to {:?}",
            att.filename, desc, target_path
        );

        let resp = match client.get(&att.url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!(
                    "Failed to download attachment '{}' from {}: HTTP {}",
                    att.filename, att.url, r.status()
                );
                continue;
            }
            Err(e) => {
                warn!(
                    "Error downloading attachment '{}' from {}: {}",
                    att.filename, att.url, e
                );
                continue;
            }
        };

        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Failed reading attachment payload '{}': {}",
                    att.filename, e
                );
                continue;
            }
        };

        if bytes.len() as u64 > limit {
            warn!(
                "Downloaded attachment '{}' exceeds limit ({} > {} bytes), skipping",
                att.filename,
                bytes.len(),
                limit
            );
            continue;
        }

        if let Err(e) = tokio::fs::write(&target_path, &bytes).await {
            warn!(
                "Failed to write attachment to disk at {:?}: {}",
                target_path, e
            );
            continue;
        }

        if is_img {
            let ocr_text = ocr::ocr_image(&target_path, config, client)
                .await
                .unwrap_or(None);
            results.push(MediaAttachment::Image {
                path: target_path,
                name: att.filename.clone(),
                ocr_text,
            });
        } else if is_aud {
            let wav_path = target_path.with_extension("wav");
            let audio_for_stt = match convert_audio_to_wav_16k(
                &config.ffmpeg_path,
                &target_path,
                &wav_path,
            )
            .await
            {
                Ok(_) => wav_path,
                Err(e) => {
                    warn!(
                        "ffmpeg conversion failed for '{}': {}. Falling back to original audio for STT",
                        att.filename, e
                    );
                    target_path.clone()
                }
            };

            let transcript = transcribe::transcribe_audio(&audio_for_stt, config, client)
                .await
                .unwrap_or_else(|_| transcribe::TRANSCRIPTION_FALLBACK.to_string());

            results.push(MediaAttachment::Audio {
                path: target_path,
                name: att.filename.clone(),
                transcript,
            });
        } else {
            let mime = att.content_type.clone().unwrap_or_else(|| {
                mime_guess::from_path(&target_path)
                    .first_or_octet_stream()
                    .to_string()
            });

            results.push(MediaAttachment::File {
                path: target_path,
                name: att.filename.clone(),
                mime,
            });
        }
    }

    // 2. Process Discord stickers as images
    for sticker in &msg.sticker_items {
        if let Some(url) = sticker.image_url() {
            let safe_sticker_name = format!(
                "sticker_{}_{}.png",
                sticker.id,
                sanitize_filename(&sticker.name)
            );
            let sticker_path = base_dir.join(&safe_sticker_name);

            debug!(
                "Downloading sticker '{}' ({}) to {:?}",
                sticker.name, sticker.id, sticker_path
            );

            let resp = match client.get(&url).send().await {
                Ok(r) if r.status().is_success() => r,
                Ok(r) => {
                    warn!(
                        "Failed to download sticker '{}' from {}: HTTP {}",
                        sticker.name, url, r.status()
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        "Error downloading sticker '{}' from {}: {}",
                        sticker.name, url, e
                    );
                    continue;
                }
            };

            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    warn!(
                        "Failed reading sticker payload '{}': {}",
                        sticker.name, e
                    );
                    continue;
                }
            };

            if bytes.len() as u64 > config.media_max_image_bytes {
                warn!(
                    "Sticker '{}' exceeds image size limit ({} > {} bytes), skipping",
                    sticker.name,
                    bytes.len(),
                    config.media_max_image_bytes
                );
                continue;
            }

            if let Err(e) = tokio::fs::write(&sticker_path, &bytes).await {
                warn!(
                    "Failed to write sticker to disk at {:?}: {}",
                    sticker_path, e
                );
                continue;
            }

            let ocr_text = ocr::ocr_image(&sticker_path, config, client)
                .await
                .unwrap_or(None);

            results.push(MediaAttachment::Image {
                path: sticker_path,
                name: format!("Sticker: {}", sticker.name),
                ocr_text,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_classification() {
        assert!(is_video_mime_or_ext("clip.mp4", None));
        assert!(is_video_mime_or_ext("video.mov", None));
        assert!(is_video_mime_or_ext("recording.webm", None));
        assert!(is_video_mime_or_ext("file.bin", Some("video/mp4")));
        assert!(!is_video_mime_or_ext("photo.png", Some("image/png")));
        assert!(!is_video_mime_or_ext("voice.ogg", Some("audio/ogg")));
    }

    #[test]
    fn test_is_audio_classification() {
        assert!(is_audio_mime_or_ext("voice.ogg", None));
        assert!(is_audio_mime_or_ext("note.opus", None));
        assert!(is_audio_mime_or_ext("sound.wav", None));
        assert!(is_audio_mime_or_ext("song.mp3", None));
        assert!(is_audio_mime_or_ext("recording.m4a", None));
        assert!(is_audio_mime_or_ext("file.bin", Some("audio/ogg; codecs=opus")));
        assert!(!is_audio_mime_or_ext("photo.png", Some("image/png")));
    }

    #[test]
    fn test_is_image_classification() {
        assert!(is_image_mime_or_ext("photo.png", None));
        assert!(is_image_mime_or_ext("pic.jpg", None));
        assert!(is_image_mime_or_ext("image.jpeg", None));
        assert!(is_image_mime_or_ext("sticker.webp", None));
        assert!(is_image_mime_or_ext("anim.gif", None));
        assert!(is_image_mime_or_ext("file.bin", Some("image/jpeg")));
        assert!(!is_image_mime_or_ext("voice.ogg", Some("audio/ogg")));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("my cool picture (1).png"), "my_cool_picture__1_.png");
        assert_eq!(sanitize_filename(".."), "attachment.bin");
        assert_eq!(sanitize_filename(""), "attachment.bin");
        assert_eq!(sanitize_filename("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
    }
}

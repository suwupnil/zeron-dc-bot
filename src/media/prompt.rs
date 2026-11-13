use crate::media::MediaAttachment;
use std::path::Path;

fn to_absolute_path_string(path: &Path) -> String {
    if path.is_absolute() {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string()
    } else {
        let abs = std::env::current_dir()
            .map(|cd| cd.join(path))
            .unwrap_or_else(|_| path.to_path_buf());
        std::fs::canonicalize(&abs)
            .unwrap_or(abs)
            .display()
            .to_string()
    }
}

/// Builds a structured prompt for Zeron AI sessions including author attribution,
/// local file references, OCR output, and audio transcription.
pub fn build_zeron_prompt(
    user_text: &str,
    attachments: &[MediaAttachment],
    author: &str,
) -> String {
    let clean_text = user_text.trim();
    let has_text = !clean_text.is_empty();

    let mut images = Vec::new();
    let mut audios = Vec::new();
    let mut files = Vec::new();

    for att in attachments {
        match att {
            MediaAttachment::Image { .. } => images.push(att),
            MediaAttachment::Audio { .. } => audios.push(att),
            MediaAttachment::File { .. } => files.push(att),
        }
    }

    let mut sections = Vec::new();

    // 1. Text header if text was provided
    if has_text {
        sections.push(format!("{}: {}", author, clean_text));
    }

    // 2. Audio/Voice message sections
    for audio in &audios {
        if let MediaAttachment::Audio { transcript, .. } = audio {
            sections.push(format!(
                "[Voice message from {}]\nTranscript: {}",
                author,
                transcript.trim()
            ));
        }
    }

    // 3. Images section
    if !images.is_empty() {
        let mut img_block = String::new();
        img_block.push_str("See the attached image(s).\nAttached images (local files — open them to view):");
        for img in &images {
            if let MediaAttachment::Image { path, ocr_text, .. } = img {
                let abs = to_absolute_path_string(path);
                img_block.push_str(&format!("\n- {}", abs));
                if let Some(ocr) = ocr_text {
                    let trimmed_ocr = ocr.trim();
                    if !trimmed_ocr.is_empty() {
                        img_block.push_str(&format!("\n  OCR: {}", trimmed_ocr));
                    }
                }
            }
        }
        if !has_text && audios.is_empty() {
            sections.push(format!("{}: {}", author, img_block));
        } else {
            sections.push(img_block);
        }
    }

    // 4. Files section
    if !files.is_empty() {
        let mut file_block = String::new();
        file_block.push_str("See the attached file(s).\nAttached files (local files — open them to view):");
        for f in &files {
            if let MediaAttachment::File { path, .. } = f {
                let abs = to_absolute_path_string(path);
                file_block.push_str(&format!("\n- {}", abs));
            }
        }
        if !has_text && audios.is_empty() && images.is_empty() {
            sections.push(format!("{}: {}", author, file_block));
        } else {
            sections.push(file_block);
        }
    }

    if sections.is_empty() {
        format!("{}:", author)
    } else {
        sections.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_text_only() {
        let prompt = build_zeron_prompt("Hello world", &[], "Alice");
        assert_eq!(prompt, "Alice: Hello world");
    }

    #[test]
    fn test_empty_text_and_no_attachments() {
        let prompt = build_zeron_prompt("", &[], "Bob");
        assert_eq!(prompt, "Bob:");
    }

    #[test]
    fn test_image_with_ocr() {
        let atts = vec![MediaAttachment::Image {
            path: PathBuf::from("/tmp/test.png"),
            name: "test.png".to_string(),
            ocr_text: Some("Total: $42.00".to_string()),
        }];
        let prompt = build_zeron_prompt("What is the total?", &atts, "Alice");
        assert!(prompt.contains("Alice: What is the total?"));
        assert!(prompt.contains("See the attached image(s)."));
        assert!(prompt.contains("Attached images (local files — open them to view):"));
        assert!(prompt.contains("- /tmp/test.png"));
        assert!(prompt.contains("  OCR: Total: $42.00"));
    }

    #[test]
    fn test_image_without_ocr_and_no_text() {
        let atts = vec![MediaAttachment::Image {
            path: PathBuf::from("/tmp/scenery.jpg"),
            name: "scenery.jpg".to_string(),
            ocr_text: None,
        }];
        let prompt = build_zeron_prompt("", &atts, "Bob");
        assert!(prompt.starts_with("Bob: See the attached image(s)."));
        assert!(prompt.contains("- /tmp/scenery.jpg"));
        assert!(!prompt.contains("OCR:"));
    }

    #[test]
    fn test_voice_message() {
        let atts = vec![MediaAttachment::Audio {
            path: PathBuf::from("/tmp/voice.wav"),
            name: "voice.ogg".to_string(),
            transcript: "Deploy to production immediately.".to_string(),
        }];
        let prompt = build_zeron_prompt("", &atts, "Charlie");
        assert_eq!(
            prompt,
            "[Voice message from Charlie]\nTranscript: Deploy to production immediately."
        );
    }

    #[test]
    fn test_file_attachment() {
        let atts = vec![MediaAttachment::File {
            path: PathBuf::from("/tmp/data.csv"),
            name: "data.csv".to_string(),
            mime: "text/csv".to_string(),
        }];
        let prompt = build_zeron_prompt("Analyze this dataset", &atts, "Dave");
        assert!(prompt.contains("Dave: Analyze this dataset"));
        assert!(prompt.contains("See the attached file(s)."));
        assert!(prompt.contains("- /tmp/data.csv"));
    }
}

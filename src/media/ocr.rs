use crate::config::Config;
use crate::error::BotResult;
use std::path::Path;
use tracing::{debug, warn};

/// Attempt to refresh Cloudflare OAuth access token if a refresh token is configured
pub async fn refresh_cloudflare_oauth_token(
    refresh_token: &str,
    client: &reqwest::Client,
) -> Option<String> {
    debug!("Refreshing Cloudflare OAuth access token...");
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", "54d11594-84e4-41aa-b438-e81b8fa78ee7"),
        ("refresh_token", refresh_token),
    ];
    let resp = client
        .post("https://dash.cloudflare.com/oauth2/token")
        .form(&params)
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        let json: serde_json::Value = resp.json().await.ok()?;
        let token = json
            .get("access_token")
            .and_then(|t| t.as_str())
            .map(String::from);
        if token.is_some() {
            debug!("Successfully refreshed Cloudflare OAuth access token");
        }
        token
    } else {
        warn!(
            "Failed to refresh Cloudflare OAuth token (HTTP status: {})",
            resp.status()
        );
        None
    }
}

/// Performs optical character recognition / vision captioning on an image
/// using Cloudflare Workers AI vision models or Tesseract CLI.
pub async fn ocr_image(
    image_path: &Path,
    config: &Config,
    client: &reqwest::Client,
) -> BotResult<Option<String>> {
    let provider = config.ocr_provider.to_lowercase();

    match provider.as_str() {
        "none" => {
            debug!("OCR skipped: OCR_PROVIDER is set to 'none'");
            Ok(None)
        }
        "tesseract" => {
            debug!("Running Tesseract OCR on {:?}", image_path);
            let output = tokio::process::Command::new(&config.tesseract_path)
                .arg(image_path)
                .arg("stdout")
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(text))
                    }
                }
                Ok(out) => {
                    warn!(
                        "Tesseract OCR failed on {:?} (status {:?}): {}",
                        image_path,
                        out.status.code(),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    Ok(None)
                }
                Err(e) => {
                    warn!(
                        "Failed to execute Tesseract CLI at '{}': {}",
                        config.tesseract_path, e
                    );
                    Ok(None)
                }
            }
        }
        "cloudflare" => {
            let account_id = match config.cloudflare_account_id.as_deref() {
                Some(acct) => acct,
                None => {
                    warn!("Cloudflare OCR skipped: CLOUDFLARE_ACCOUNT_ID is missing");
                    return Ok(None);
                }
            };

            let mut active_token = match config.cloudflare_api_token.as_deref() {
                Some(token) => token.to_string(),
                None => {
                    if let Some(ref rt) = config.cloudflare_refresh_token {
                        if let Some(refreshed) = refresh_cloudflare_oauth_token(rt, client).await {
                            refreshed
                        } else {
                            warn!("Cloudflare OCR skipped: CLOUDFLARE_API_TOKEN is missing and refresh token failed");
                            return Ok(None);
                        }
                    } else {
                        warn!("Cloudflare OCR skipped: CLOUDFLARE_API_TOKEN is missing");
                        return Ok(None);
                    }
                }
            };

            let image_bytes = match tokio::fs::read(image_path).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to read image at {:?} for OCR: {}", image_path, e);
                    return Ok(None);
                }
            };

            if image_bytes.is_empty() {
                return Ok(None);
            }

            let endpoint = config.cloudflare_ai_endpoint.clone().unwrap_or_else(|| {
                format!(
                    "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
                    account_id, config.ocr_model
                )
            });

            debug!(
                "Sending OCR request to Cloudflare vision model {} at {}",
                config.ocr_model, endpoint
            );

            let payload = serde_json::json!({
                "prompt": "Transcribe all visible text in this image verbatim. If there is no text, describe what is in the image concisely.",
                "image": image_bytes,
                "max_tokens": 512
            });

            let mut resp = client
                .post(&endpoint)
                .bearer_auth(&active_token)
                .json(&payload)
                .send()
                .await;

            // If 401 Unauthorized occurs and we have a refresh token, refresh and retry once
            if let Ok(ref r) = resp {
                if r.status() == reqwest::StatusCode::UNAUTHORIZED {
                    if let Some(ref rt) = config.cloudflare_refresh_token {
                        warn!("Cloudflare OCR received 401 Unauthorized. Attempting OAuth refresh...");
                        if let Some(new_token) = refresh_cloudflare_oauth_token(rt, client).await {
                            active_token = new_token;
                            resp = client
                                .post(&endpoint)
                                .bearer_auth(&active_token)
                                .json(&payload)
                                .send()
                                .await;
                        }
                    }
                }
            }

            match resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    let desc = body
                        .get("result")
                        .and_then(|res| {
                            res.get("description")
                                .or_else(|| res.get("response"))
                                .or_else(|| res.get("text"))
                        })
                        .or_else(|| body.get("description"))
                        .or_else(|| body.get("response"))
                        .and_then(|v| v.as_str());

                    if let Some(text) = desc {
                        let trimmed = text.trim();
                        if trimmed.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(trimmed.to_string()))
                        }
                    } else {
                        Ok(None)
                    }
                }
                Ok(r) => {
                    warn!(
                        "Cloudflare OCR request returned HTTP status: {}",
                        r.status()
                    );
                    Ok(None)
                }
                Err(e) => {
                    warn!("Network error calling Cloudflare OCR: {}", e);
                    Ok(None)
                }
            }
        }
        unknown => {
            warn!("Unknown OCR_PROVIDER '{}', skipping OCR", unknown);
            Ok(None)
        }
    }
}

/// Heuristic check to determine if an image requires deeper visual understanding
/// (e.g. vision LLM analysis) beyond pure plain OCR text.
pub fn needs_visual_understanding(image_path: &Path, ocr_text: Option<&str>) -> bool {
    let text = match ocr_text {
        Some(t) => t.trim(),
        None => return true,
    };

    if text.is_empty() || text.chars().count() < 99 {
        return true;
    }

    let visual_keywords = [
        "chart",
        "diagram",
        "graph",
        "plot",
        "screenshot",
        "table",
        "figure",
        "ui",
        "button",
        "wireframe",
        "dashboard",
        "icon",
        "photo",
        "drawing",
        "illustration",
        "flowchart",
        "canvas",
    ];

    for word in text.split_whitespace() {
        let clean_word = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        for kw in visual_keywords {
            if clean_word == kw || (clean_word.starts_with(kw) && clean_word.len() <= kw.len() + 2)
            {
                return true;
            }
        }
    }

    if let Some(ext) = image_path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        if (ext_lower == "gif" || ext_lower == "webp") && text.chars().count() < 40 {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_needs_visual_understanding_no_ocr() {
        let path = PathBuf::from("diagram.png");
        assert!(needs_visual_understanding(&path, None));
    }

    #[test]
    fn test_needs_visual_understanding_short_text() {
        let path = PathBuf::from("logo.png");
        assert!(needs_visual_understanding(&path, Some("ACME Corp")));
    }

    #[test]
    fn test_needs_visual_understanding_with_chart_keyword() {
        let path = PathBuf::from("report.png");
        assert!(needs_visual_understanding(
            &path,
            Some(
                "Monthly revenue chart showing steady increase from Q1 to Q4 across all segments."
            )
        ));
    }

    #[test]
    fn test_needs_visual_understanding_plain_dense_text() {
        let path = PathBuf::from("document.png");
        let plain_text = "The quick brown fox jumps over the lazy dog. Section 1 outlines the contractual obligations of both parties in full detail without ambiguity.";
        assert!(!needs_visual_understanding(&path, Some(plain_text)));
    }

    #[tokio::test]
    async fn test_ocr_provider_none() {
        let mut config = Config::from_env().unwrap_or_else(|_| panic!("Failed to load config"));
        config.ocr_provider = "none".to_string();
        let client = reqwest::Client::new();
        let path = PathBuf::from("dummy.png");

        let res = ocr_image(&path, &config, &client)
            .await
            .expect("OCR should succeed");
        assert_eq!(res, None);
    }
}

use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseFollowup, CreateInteractionResponseMessage,
};
use std::collections::HashSet;
use tracing::{info, warn};

#[derive(Debug, thiserror::Error)]
pub enum LaunchModeError {
    #[error("MAGPIE_URL is not configured in the bot environment (.env). Please set MAGPIE_URL to your Cloudflare Worker URL.")]
    MissingMagpieUrl,

    #[error("UPDATE_SECRET is not configured in the bot environment (.env). Please set UPDATE_SECRET to match your Cloudflare Worker secret.")]
    MissingUpdateSecret,

    #[error("Worker rejected launch mode request (HTTP {status}): {body}")]
    Rejected {
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("Network error communicating with worker at {endpoint}: {source}")]
    Network {
        endpoint: String,
        source: reqwest::Error,
    },
}

fn is_transient_network_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect()
}

const MAX_HISTORY_AUTOCOMPLETE_CHOICES: usize = 25;
const DISCORD_AUTOCOMPLETE_MAX_LEN: usize = 100;

pub fn register() -> CreateCommand {
    CreateCommand::new("terminal")
        .description("Open a VM terminal session in Discord")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "command",
                "Optional command to run inside the terminal",
            )
            .required(false)
            .set_autocomplete(true),
        )
}

pub async fn notify_worker_launch_mode(
    state: &AppState,
    channel_id: u64,
    mode: &str,
) -> Result<(), LaunchModeError> {
    notify_worker_launch_mode_with_command(state, channel_id, mode, None).await
}

pub async fn notify_worker_launch_mode_with_command(
    state: &AppState,
    channel_id: u64,
    mode: &str,
    command: Option<&str>,
) -> Result<(), LaunchModeError> {
    let client = reqwest::Client::builder()
        .user_agent("zeron-dc-bot/1.0")
        .connect_timeout(std::time::Duration::from_millis(800))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    notify_worker_launch_mode_client_with_command(
        &client,
        state.config.magpie_url.as_deref(),
        state.config.update_secret.as_deref(),
        channel_id,
        mode,
        command,
    )
    .await
}

pub async fn notify_worker_launch_mode_client(
    client: &reqwest::Client,
    worker_url: Option<&str>,
    update_secret: Option<&str>,
    channel_id: u64,
    mode: &str,
) -> Result<(), LaunchModeError> {
    notify_worker_launch_mode_client_with_command(
        client,
        worker_url,
        update_secret,
        channel_id,
        mode,
        None,
    )
    .await
}

pub async fn notify_worker_launch_mode_client_with_command(
    client: &reqwest::Client,
    worker_url: Option<&str>,
    update_secret: Option<&str>,
    channel_id: u64,
    mode: &str,
    command: Option<&str>,
) -> Result<(), LaunchModeError> {
    let worker_url = match worker_url {
        Some(u) if !u.trim().is_empty() => u.trim(),
        _ => return Err(LaunchModeError::MissingMagpieUrl),
    };

    let secret = match update_secret {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => return Err(LaunchModeError::MissingUpdateSecret),
    };

    let endpoint = format!("{}/__set_launch_mode", worker_url.trim_end_matches('/'));
    let clean_command = command.map(str::trim).filter(|s| !s.is_empty());
    let body = if let Some(cmd) = clean_command {
        serde_json::json!({
            "channel_id": channel_id.to_string(),
            "mode": mode,
            "command": cmd,
        })
    } else {
        serde_json::json!({
            "channel_id": channel_id.to_string(),
            "mode": mode,
        })
    };

    let max_attempts = 2;
    let mut last_network_err = None;

    for attempt in 1..=max_attempts {
        let req = client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", secret))
            .json(&body)
            .timeout(std::time::Duration::from_millis(1400));

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                info!(
                    "Worker launch mode response: status={} channel_id={} mode={}",
                    status, channel_id, mode
                );

                if status.is_success() {
                    return Ok(());
                } else {
                    let text = resp.text().await.unwrap_or_default();
                    let trimmed = text.trim();
                    let body = if trimmed.is_empty() {
                        status
                            .canonical_reason()
                            .unwrap_or("No error details returned by worker")
                            .to_string()
                    } else if trimmed.chars().count() > 200 {
                        format!("{}...", trimmed.chars().take(200).collect::<String>())
                    } else {
                        trimmed.to_string()
                    };
                    warn!(
                        "Worker rejected launch mode request: status={} channel_id={} mode={} body={}",
                        status, channel_id, mode, body
                    );
                    return Err(LaunchModeError::Rejected {
                        status,
                        body,
                    });
                }
            }
            Err(e) => {
                let is_transient = is_transient_network_error(&e);
                warn!(
                    "Attempt {}/{} failed to coordinate activity launch mode with worker (transient={}): {}",
                    attempt, max_attempts, is_transient, e
                );
                last_network_err = Some(e);
                if !is_transient {
                    break;
                }
                if attempt < max_attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    Err(LaunchModeError::Network {
        endpoint,
        source: last_network_err.expect("At least one network attempt was performed"),
    })
}

/// Strips the leading history entry index (e.g. `  363  cmd` or `  363* cmd`) from a `history` line.
fn parse_history_line(line: &str) -> &str {
    let trimmed = line.trim_start();
    let digit_len = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digit_len > 0 {
        let after_digits = &trimmed[digit_len..];
        let after_marker = after_digits.strip_prefix('*').unwrap_or(after_digits);
        if after_marker.starts_with(char::is_whitespace) {
            return after_marker.trim();
        }
    }
    trimmed.trim()
}

/// Parses `history` command output into deduplicated commands sorted latest-first.
/// Keeps only the most recent occurrence of each unique command and limits to `limit` entries
/// within Discord's 100-character autocomplete choice limit.
pub fn parse_history_output(raw: &str, filter: &str, limit: usize) -> Vec<String> {
    let filter_lower = filter.trim().to_lowercase();
    let mut seen = HashSet::new();
    let mut results = Vec::new();

    for line in raw.lines().rev() {
        let cmd = parse_history_line(line);
        if cmd.is_empty() || cmd.chars().count() > DISCORD_AUTOCOMPLETE_MAX_LEN {
            continue;
        }
        if !seen.insert(cmd.to_string()) {
            continue;
        }
        if !filter_lower.is_empty() && !cmd.to_lowercase().contains(&filter_lower) {
            continue;
        }
        results.push(cmd.to_string());
        if results.len() >= limit {
            break;
        }
    }

    results
}

async fn run_shell_history(shell: &str, script: &str) -> Option<String> {
    let fut = tokio::process::Command::new(shell)
        .arg("-c")
        .arg(script)
        .output();

    match tokio::time::timeout(std::time::Duration::from_millis(1000), fut).await {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Ok(Ok(output)) => {
            warn!(
                "Shell history command '{}' exited with status {}: {}",
                shell,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Ok(Err(e)) => {
            warn!("Failed to spawn '{}' for terminal history: {}", shell, e);
            None
        }
        Err(_) => {
            warn!("Timed out retrieving terminal history from '{}'", shell);
            None
        }
    }
}

async fn get_recent_terminal_history(filter: &str, limit: usize) -> Vec<String> {
    let bash_out = run_shell_history(
        "bash",
        r#"HISTFILE="${HISTFILE:-$HOME/.bash_history}"; HISTTIMEFORMAT=""; set -o history; history"#,
    )
    .await;

    if let Some(raw) = bash_out {
        let parsed = parse_history_output(&raw, filter, limit);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    let zsh_out = run_shell_history(
        "zsh",
        r#"HISTFILE="${HISTFILE:-$HOME/.zsh_history}"; fc -R "$HISTFILE" 2>/dev/null; fc -l 1"#,
    )
    .await;

    match zsh_out {
        Some(raw) => parse_history_output(&raw, filter, limit),
        None => Vec::new(),
    }
}

pub async fn autocomplete(
    ctx: &Context,
    autocomplete: &CommandInteraction,
    _state: &AppState,
) -> BotResult<()> {
    let focused = autocomplete
        .data
        .options
        .iter()
        .find(|opt| opt.name == "command");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_string(),
            CommandDataOptionValue::Autocomplete { value, .. } => value.clone(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let commands =
        get_recent_terminal_history(&current_input, MAX_HISTORY_AUTOCOMPLETE_CHOICES).await;

    let choices: Vec<AutocompleteChoice> = commands
        .into_iter()
        .map(|cmd| AutocompleteChoice::new(cmd.clone(), cmd))
        .collect();

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    let initial_command = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "command")
        .and_then(|opt| opt.value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    info!(
        "User {} requested /terminal activity in channel {} (command={:?})",
        command.user.name, command.channel_id, initial_command
    );

    // Notify worker of launch mode for this channel so the iframe routes to terminal
    if let Err(e) = notify_worker_launch_mode_with_command(
        state,
        command.channel_id.get(),
        "terminal",
        initial_command,
    )
    .await
    {
        warn!(
            "Aborting /terminal launch due to worker coordination failure: {}",
            e
        );

        let embed = CreateEmbed::new()
            .title("❌ Terminal Launch Failed")
            .description(format!(
                "Could not set activity launch mode to **terminal**.\n\n\
                **Error:** {}\n\n\
                *Please verify that `MAGPIE_URL` and `UPDATE_SECRET` are properly configured in `.env` and match your Cloudflare Worker settings.*",
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
            warn!("Failed to send error response for /terminal: {}", msg_err);
        }

        return Ok(());
    }

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
                .embed(embed.clone())
                .ephemeral(true);

            if let Err(_msg_err) = command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await
            {
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_terminal_command_registration() {
        let cmd = register();
        let val = serde_json::to_value(&cmd).unwrap();
        assert_eq!(val["name"], "terminal");
        assert_eq!(val["description"], "Open a VM terminal session in Discord");
        let options = val["options"].as_array().expect("terminal should define options");
        assert_eq!(options.len(), 1);
        assert_eq!(options[0]["name"], "command");
        assert_eq!(options[0]["required"], false);
        assert_eq!(options[0]["autocomplete"], true);
    }

    #[test]
    fn test_parse_history_output_latest_first_dedup_and_limit() {
        let mut lines = Vec::new();
        for i in 1..=30 {
            lines.push(format!("{:>5}  cmd-{}", i, i));
        }
        // Duplicate an earlier command at the very end and add an annotated/padded line
        lines.push("   31* cmd-5  ".to_string());
        lines.push("   32  ".to_string()); // empty after trimming
        let raw = lines.join("\n");

        let parsed = parse_history_output(&raw, "", 25);
        assert_eq!(parsed.len(), 25);
        assert_eq!(parsed[0], "cmd-5");
        assert_eq!(parsed[1], "cmd-30");
        assert_eq!(parsed[24], "cmd-7");
        // Earlier cmd-5 should have been deduplicated
        assert_eq!(parsed.iter().filter(|c| *c == "cmd-5").count(), 1);
    }

    #[test]
    fn test_parse_history_output_filter_and_max_len() {
        let long_cmd = "x".repeat(101);
        let raw = format!(
            "  10  ls -la\n  11  magpie serve\n  12  {}\n  13  MAGPIE update\n",
            long_cmd
        );

        let all = parse_history_output(&raw, "", 25);
        assert_eq!(all, vec!["MAGPIE update", "magpie serve", "ls -la"]);

        let filtered = parse_history_output(&raw, "magpie", 25);
        assert_eq!(filtered, vec!["MAGPIE update", "magpie serve"]);
    }

    #[tokio::test]
    async fn test_notify_worker_missing_magpie_url() {
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, None, Some("secret"), 12345, "terminal").await;
        assert!(matches!(res, Err(LaunchModeError::MissingMagpieUrl)));

        let res_empty = notify_worker_launch_mode_client(&client, Some("   "), Some("secret"), 12345, "terminal").await;
        assert!(matches!(res_empty, Err(LaunchModeError::MissingMagpieUrl)));
    }

    #[tokio::test]
    async fn test_notify_worker_missing_update_secret() {
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some("http://127.0.0.1"), None, 12345, "terminal").await;
        assert!(matches!(res, Err(LaunchModeError::MissingUpdateSecret)));

        let res_empty = notify_worker_launch_mode_client(&client, Some("http://127.0.0.1"), Some("   "), 12345, "terminal").await;
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
    async fn test_notify_worker_success() {
        let (base_url, handle) = spawn_mock_server(200, "OK", r#"{"success":true,"channel_id":"12345","mode":"terminal"}"#).await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret-token"), 12345, "terminal").await;
        let _ = handle.await;
        assert!(res.is_ok(), "Expected Ok(()), got {:?}", res);
    }

    #[tokio::test]
    async fn test_notify_worker_rejected_unauthorized() {
        let (base_url, handle) = spawn_mock_server(401, "Unauthorized", "Unauthorized").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("bad-secret"), 12345, "terminal").await;
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
    async fn test_notify_worker_rejected_bad_request() {
        let (base_url, handle) = spawn_mock_server(400, "Bad Request", "Missing or invalid channel_id").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret"), 12345, "terminal").await;
        let _ = handle.await;
        match res {
            Err(LaunchModeError::Rejected { status, body }) => {
                assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
                assert_eq!(body, "Missing or invalid channel_id");
            }
            other => panic!("Expected Rejected with 400, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_notify_worker_rejected_service_unavailable() {
        let (base_url, handle) = spawn_mock_server(503, "Service Unavailable", "Mode store unavailable").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret"), 12345, "terminal").await;
        let _ = handle.await;
        match res {
            Err(LaunchModeError::Rejected { status, body }) => {
                assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body, "Mode store unavailable");
            }
            other => panic!("Expected Rejected with 503, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_notify_worker_rejected_empty_body() {
        // When worker returns 403 Forbidden with empty body, it should fallback to status canonical reason
        let (base_url, handle) = spawn_mock_server(403, "Forbidden", "").await;
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(&client, Some(&base_url), Some("secret"), 12345, "terminal").await;
        let _ = handle.await;
        match res {
            Err(LaunchModeError::Rejected { status, body }) => {
                assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
                assert_eq!(body, "Forbidden");
            }
            other => panic!("Expected Rejected with 403 Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_notify_worker_network_error_retries() {
        // Connect to an unused port that immediately refuses connection
        let client = reqwest::Client::new();
        let res = notify_worker_launch_mode_client(
            &client,
            Some("http://127.0.0.1:1"), // port 1 is reserved and not listening
            Some("secret"),
            12345,
            "terminal",
        )
        .await;
        match res {
            Err(LaunchModeError::Network { endpoint, .. }) => {
                assert!(endpoint.contains("/__set_launch_mode"));
            }
            other => panic!("Expected Network error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_notify_worker_with_command_includes_command_in_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            let req_text = String::from_utf8_lossy(&buf[..n]).to_string();
            let resp = r#"{"success":true,"channel_id":"12345","mode":"terminal","command":"htop"}"#;
            let http_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp.len(),
                resp
            );
            socket.write_all(http_response.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();
            req_text
        });

        let client = reqwest::Client::new();
        let base_url = format!("http://{}", addr);
        let res = notify_worker_launch_mode_client_with_command(
            &client,
            Some(&base_url),
            Some("secret-token"),
            12345,
            "terminal",
            Some("  htop  "),
        )
        .await;
        assert!(res.is_ok(), "Expected Ok(()), got {:?}", res);

        let raw_req = handle.await.unwrap();
        assert!(raw_req.contains(r#""command":"htop""#));
        assert!(raw_req.contains(r#""mode":"terminal""#));
    }

    #[tokio::test]
    async fn test_notify_worker_non_transient_error_no_retry() {
        // Passing an invalid URL scheme triggers a builder error which is non-transient and fails immediately
        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let res = notify_worker_launch_mode_client(
            &client,
            Some("http://invalid url with spaces"),
            Some("secret"),
            12345,
            "terminal",
        )
        .await;
        let elapsed = start.elapsed();

        match res {
            Err(LaunchModeError::Network { endpoint, .. }) => {
                assert!(endpoint.contains("/__set_launch_mode"));
                // Should fail immediately on attempt 1 without sleeping for retries (<100ms)
                assert!(elapsed < std::time::Duration::from_millis(150));
            }
            other => panic!("Expected Network error for invalid URL, got {:?}", other),
        }
    }
}


use crate::error::BotResult;
use crate::state::{AppState, SessionOrigin};
use serenity::all::{Channel, ChannelId, ChannelType, Context, EditChannel};
use std::collections::BTreeSet;
use std::path::Path;
use tracing::{info, warn};

// Built-in skills list
pub const DEFAULT_SKILLS: &[&str] = &[
    "cloudflare",
    "hyperframes-cli",
    "reddit-automation",
    "vercel-react-best-practices",
    "code-refactoring",
    "unit-testing",
    "security-audit",
    "git-workflow",
    "doc-gen",
];

// Built-in MCP servers list
pub const DEFAULT_MCP_SERVERS: &[&str] = &[
    "zeron-engine",
    "github",
    "sqlite",
    "postgres",
    "filesystem",
    "memory",
    "puppeteer",
    "brave-search",
    "fetch",
    "slack",
    "docker",
    "kubernetes",
];

/// Collect available skills and MCP servers from installed filesystem paths and Zeron MCP plugins
pub async fn get_available_skills_and_mcp(state: &AppState) -> (Vec<String>, Vec<String>) {
    let mut skills_set = BTreeSet::new();
    let mut mcp_set = BTreeSet::new();

    // 1. Built-in standard skills
    for s in DEFAULT_SKILLS {
        skills_set.insert(s.to_string());
    }

    // 2. Discover local filesystem skills
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let possible_skill_dirs = [
        format!("{}/.zeron/skills", home),
        format!("{}/.config/skills", home),
        format!("{}/.gemini/config/skills", home),
        format!("{}/.gemini/antigravity-cli/builtin/skills", home),
        format!("{}/.codex/skills", home),
        "./skills".to_string(),
    ];

    for dir in &possible_skill_dirs {
        let p = Path::new(dir);
        if p.exists() && p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !name.starts_with('.') {
                                skills_set.insert(name);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Built-in standard MCP servers
    for m in DEFAULT_MCP_SERVERS {
        mcp_set.insert(m.to_string());
    }

    // 4. Installed plugins from Zeron MCP daemon
    if let Ok(plugins) = state.zeron.list_plugins().await {
        for p in plugins {
            mcp_set.insert(p.id);
        }
    }

    (
        skills_set.into_iter().collect(),
        mcp_set.into_iter().collect(),
    )
}

/// Update Discord channel topic according to specifications:
/// - First line is always the current model in bold (`**Model: ...**`)
/// - For `/spawn` sessions: followed by available Skills and MCP servers
/// - For `@` tag sessions: only the bold model line
pub async fn update_channel_topic(
    ctx: &Context,
    channel_id: u64,
    state: &AppState,
) -> BotResult<()> {
    let session = match state.get_session(channel_id).await {
        Some(s) if !s.is_closed => s,
        _ => return Ok(()),
    };

    // Discord threads do not have channel topics
    match ctx.http.get_channel(ChannelId::new(channel_id)).await {
        Ok(Channel::Guild(gc)) => {
            if matches!(
                gc.kind,
                ChannelType::PublicThread | ChannelType::PrivateThread | ChannelType::NewsThread
            ) {
                return Ok(());
            }
        }
        _ => return Ok(()),
    };

    let model_name = session
        .model
        .clone()
        .or_else(|| state.config.default_model.clone())
        .unwrap_or_else(|| session.harness.clone());

    let first_line = format!("**Model: {}**", model_name);

    let topic_content = if session.origin == SessionOrigin::Spawn {
        let (skills, mcps) = get_available_skills_and_mcp(state).await;
        let mut text = format!(
            "{}\nSkills: {}\nMCP: {}",
            first_line,
            skills.join(", "),
            mcps.join(", ")
        );
        // Discord topic max length is 1024 characters
        if text.len() > 1024 {
            text.truncate(1020);
            text.push_str("...");
        }
        text
    } else {
        first_line
    };

    info!(
        "Updating topic for channel {} (origin: {:?}): {}",
        channel_id, session.origin, topic_content
    );

    let edit = EditChannel::new().topic(topic_content);
    if let Err(e) = ChannelId::new(channel_id).edit(&ctx.http, edit).await {
        warn!(
            "Failed to update channel topic for channel {}: {} (check bot Manage Channels permissions)",
            channel_id, e
        );
    }

    Ok(())
}

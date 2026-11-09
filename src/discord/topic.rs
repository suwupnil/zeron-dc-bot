use crate::error::BotResult;
use crate::state::AppState;
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
        format!("{}/.gemini/antigravity-cli/builtin/skills", home),
        format!("{}/.gemini/config/skills", home),
        format!("{}/.hermes/hermes-agent/skills", home),
        format!("{}/.zeron/skills", home),
        format!("{}/.config/skills", home),
        format!("{}/.codex/skills", home),
        "./skills".to_string(),
    ];

    for dir in &possible_skill_dirs {
        scan_skills_recursive(Path::new(dir), &mut skills_set, 0, 3);
    }

    // 3. Built-in standard MCP servers
    for m in DEFAULT_MCP_SERVERS {
        mcp_set.insert(m.to_string());
    }

    // 4. Discover MCP server configuration files (.mcp.json, mcp_config.json)
    discover_mcps_from_filesystem(&home, &mut mcp_set);

    // 5. Installed plugins from Zeron MCP daemon
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

fn scan_skills_recursive(
    dir: &Path,
    skills: &mut BTreeSet<String>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth || !dir.exists() || !dir.is_dir() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if ft.is_dir() {
                    let sub_path = entry.path();
                    // If directory contains a SKILL.md, it is a skill
                    if sub_path.join("SKILL.md").exists()
                        || sub_path.join("skill.md").exists()
                        || sub_path.join("skills.md").exists()
                    {
                        skills.insert(name);
                    } else {
                        // Recursively scan subcategories (e.g. skills/creative/p5js)
                        scan_skills_recursive(&sub_path, skills, depth + 1, max_depth);
                    }
                }
            }
        }
    }
}

fn discover_mcps_from_filesystem(home: &str, mcps: &mut BTreeSet<String>) {
    // 1. Check ~/.gemini/config/mcp_config.json
    let gemini_mcp = Path::new(home).join(".gemini/config/mcp_config.json");
    if let Ok(content) = std::fs::read_to_string(&gemini_mcp) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(servers) = val.get("mcpServers").and_then(|s| s.as_object()) {
                for key in servers.keys() {
                    mcps.insert(key.clone());
                }
            }
        }
    }

    // 2. Check ~/.codex/.tmp/plugins/plugins/*/.mcp.json
    let codex_plugins = Path::new(home).join(".codex/.tmp/plugins/plugins");
    if codex_plugins.exists() && codex_plugins.is_dir() {
        if let Ok(entries) = std::fs::read_dir(codex_plugins) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.join(".mcp.json").exists() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    mcps.insert(name);
                }
            }
        }
    }
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
        .unwrap_or_else(|| "default".to_string());

    let effort_name = session.effort.as_deref().unwrap_or("default");

    let topic_content = format!("{}/{}.{}", session.harness, model_name, effort_name);

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

/// Clear/empty the channel topic when a session is closed or unlinked
pub async fn clear_channel_topic(ctx: &Context, channel_id: u64) -> BotResult<()> {
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

    info!("Clearing topic for closed channel {}", channel_id);
    let edit = EditChannel::new().topic("");
    if let Err(e) = ChannelId::new(channel_id).edit(&ctx.http, edit).await {
        warn!(
            "Failed to clear channel topic for channel {}: {} (check bot Manage Channels permissions)",
            channel_id, e
        );
    }

    Ok(())
}

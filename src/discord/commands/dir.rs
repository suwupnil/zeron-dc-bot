use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use std::path::{Path, PathBuf};

pub fn register() -> CreateCommand {
    CreateCommand::new("dir")
        .description("Set the project directory for this channel / thread on the host VM")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "path",
                "Absolute or relative path to the project directory on the host VM",
            )
            .required(true)
            .set_autocomplete(true),
        )
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let path_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "path")
        .and_then(|opt| opt.value.as_str());

    let raw_path = match path_option {
        Some(p) => p,
        None => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `path`");
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    let expanded_path_str = shellexpand::tilde(raw_path).to_string();
    let path = PathBuf::from(&expanded_path_str);

    // Validate path on target VM
    if !path.exists() {
        let resp = CreateInteractionResponseMessage::new().content(format!(
            "❌ **Path not found on host VM:** `{}`\nPlease check that the folder exists on the machine running Zeron.",
            expanded_path_str
        ));
        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
            .await?;
        return Ok(());
    }

    if !path.is_dir() {
        let resp = CreateInteractionResponseMessage::new().content(format!(
            "❌ **Not a directory:** `{}`\nPlease provide a directory path.",
            expanded_path_str
        ));
        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
            .await?;
        return Ok(());
    }

    let canonical_str = path
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(expanded_path_str);

    let channel_id = command.channel_id.get();

    // Ensure session exists or create it
    let session = match state.get_session(channel_id).await {
        Some(_) => state.set_project_dir(channel_id, &canonical_str).await?,
        None => {
            let _ = state
                .get_or_create_channel_session(
                    channel_id,
                    "general",
                    command.guild_id.map(|g| g.get()),
                )
                .await?;
            state.set_project_dir(channel_id, &canonical_str).await?
        }
    };

    let resp_text = format!(
        "📁 **Project Directory Set**\n- **Session:** `{}`\n- **Target Path:** `{}`\n- **Kind:** `{}`",
        session.chat_id, session.project_dir, session.kind
    );

    let resp = CreateInteractionResponseMessage::new().content(resp_text);
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
        .await?;

    Ok(())
}

pub async fn autocomplete(
    ctx: &Context,
    autocomplete: &CommandInteraction,
    state: &AppState,
) -> BotResult<()> {
    let focused = autocomplete
        .data
        .options
        .iter()
        .find(|opt| opt.name == "path");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str(),
            _ => "",
        },
        None => "",
    };

    let mut suggestions: Vec<String> = Vec::new();

    // 1. Check known Zeron projects from MCP list_projects
    if let Ok(projects) = state.zeron.list_projects().await {
        for proj in projects {
            if let Some(path) = proj.path {
                if path.to_lowercase().contains(&current_input.to_lowercase()) {
                    suggestions.push(path);
                }
            }
        }
    }

    // 2. Local filesystem path autocomplete
    let expanded = shellexpand::tilde(current_input).to_string();
    let current_path = Path::new(&expanded);

    let (dir_to_scan, prefix) = if current_input.is_empty() {
        (state.config.default_project_dir.clone(), "")
    } else if current_path.is_dir() {
        (current_path.to_path_buf(), "")
    } else {
        let parent = current_path
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf();
        let file_name = current_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        (parent, file_name)
    };

    if dir_to_scan.exists() && dir_to_scan.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir_to_scan) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !name.starts_with('.') && (prefix.is_empty() || name.to_lowercase().starts_with(&prefix.to_lowercase())) {
                            suggestions.push(entry.path().to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    // Default suggestions if list is empty
    if suggestions.is_empty() {
        suggestions.push(state.config.default_project_dir.to_string_lossy().to_string());
        if let Ok(cwd) = std::env::current_dir() {
            suggestions.push(cwd.to_string_lossy().to_string());
        }
    }

    suggestions.sort();
    suggestions.dedup();
    suggestions.truncate(25);

    let choices: Vec<AutocompleteChoice> = suggestions
        .into_iter()
        .map(|p| {
            let label = if p.len() > 100 {
                format!("...{}", &p[p.len() - 97..])
            } else {
                p.clone()
            };
            AutocompleteChoice::new(label, p)
        })
        .collect();

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

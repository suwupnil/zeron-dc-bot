use crate::discord::topic;
use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use std::path::Path;

pub fn register() -> CreateCommand {
    CreateCommand::new("spawn")
        .description("Start a brand new project")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "directory",
                "Host directory of the project",
            )
            .required(true)
            .set_autocomplete(true),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    let dir_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "directory")
        .and_then(|opt| opt.value.as_str());

    let dir_input = match dir_option {
        Some(d) => d.trim(),
        None => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `directory`");
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    let expanded = shellexpand::tilde(dir_input);
    let path = std::path::PathBuf::from(expanded.as_ref());

    // If directory does not exist, create it
    if !path.exists() {
        if let Err(e) = tokio::fs::create_dir_all(&path).await {
            let resp = CreateInteractionResponseMessage::new().content(format!(
                "❌ Failed to create directory `{}`: {}",
                path.display(),
                e
            ));
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    }

    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.clone())
        .to_string_lossy()
        .to_string();

    let channel_id = command.channel_id.get();
    let channel_name = command
        .channel_id
        .to_channel(&ctx.http)
        .await
        .ok()
        .and_then(|c| match c {
            serenity::all::Channel::Guild(gc) => Some(gc.name),
            _ => None,
        })
        .unwrap_or_else(|| "project".to_string());

    let guild_id = command.guild_id.map(|g| g.get());

    let session = state
        .spawn_project_session(channel_id, &channel_name, guild_id, &canonical_path)
        .await?;

    // Update channel topic with model, skills, and MCP servers
    let _ = topic::update_channel_topic(ctx, channel_id, state).await;

    let resp_text = format!(
        "🚀 **Zeron Session Spawned**\n\
         - **Session ID:** `{}`\n\
         - **Directory:** `{}`\n\
         - **Harness:** `{}`\n\
         - **Active Model:** `{}`",
        session.chat_id,
        session.project_dir,
        session.harness,
        session.model.as_deref().unwrap_or("harness default")
    );

    let resp = CreateInteractionResponseMessage::new()
        .content(resp_text)
        .ephemeral(true);
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
        .find(|opt| opt.name == "directory");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().trim(),
            _ => "",
        },
        None => "",
    };

    let search_dir = if current_input.is_empty() {
        state.config.default_project_dir.clone()
    } else {
        let expanded = shellexpand::tilde(current_input);
        let path = Path::new(expanded.as_ref());
        if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| state.config.default_project_dir.clone())
        }
    };

    let mut choices: Vec<AutocompleteChoice> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    let path_str = entry.path().to_string_lossy().to_string();
                    let name = entry.file_name().to_string_lossy().to_string();

                    if !name.starts_with('.')
                        && (current_input.is_empty()
                            || path_str
                                .to_lowercase()
                                .contains(&current_input.to_lowercase()))
                    {
                        choices.push(AutocompleteChoice::new(path_str.clone(), path_str));
                    }
                }
            }
        }
    }

    choices.truncate(25);

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

// Known common / built-in MCP servers and tools
const DEFAULT_MCP_SERVERS: &[(&str, &str)] = &[
    ("zeron-engine", "Core Zeron host & workspace automation MCP server"),
    ("github", "GitHub repository, issues, and PR management MCP server"),
    ("sqlite", "Local SQLite database querying and inspection MCP server"),
    ("postgres", "PostgreSQL database client MCP server"),
    ("filesystem", "Local VM filesystem exploration MCP server"),
    ("memory", "Knowledge graph and persistent memory MCP server"),
    ("puppeteer", "Browser automation and web scraping MCP server"),
    ("brave-search", "Brave search engine API MCP server"),
    ("fetch", "HTTP fetch and web resource inspector MCP server"),
    ("slack", "Slack messaging and channel integration MCP server"),
    ("docker", "Docker container and image management MCP server"),
    ("kubernetes", "Kubernetes cluster administration MCP server"),
];

pub fn register() -> CreateCommand {
    CreateCommand::new("mcp")
        .description("Select or enable an MCP (Model Context Protocol) server for this session")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "server",
                "Name of the MCP server to attach to the current session",
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
    let server_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "server")
        .and_then(|opt| opt.value.as_str());

    let server_name = match server_option {
        Some(s) => s.trim(),
        None => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `server`");
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    let channel_id = command.channel_id.get();
    let session = match state.get_session(channel_id).await {
        Some(s) => s,
        None => {
            state
                .get_or_create_channel_session(
                    channel_id,
                    "general",
                    command.guild_id.map(|g| g.get()),
                )
                .await?
        }
    };

    // Notify the Zeron agent in this session that the MCP server has been selected
    let prompt_msg = format!(
        "[System: Discord user selected MCP server '{}'. Please initialize or verify access to this MCP toolset if needed.]",
        server_name
    );
    let _ = state.zeron.send_message(&session.chat_id, &prompt_msg, false).await;

    let resp_text = format!(
        "🔌 **MCP Server Selected**\n- **Session:** `{}`\n- **MCP Server:** `{}`\n- **Status:** Active for this conversation\n- **Kind:** `{}`",
        session.chat_id, server_name, session.kind
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
        .find(|opt| opt.name == "server");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_lowercase(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let mut choices: Vec<AutocompleteChoice> = Vec::new();

    // 1. Plugins from Zeron
    if let Ok(plugins) = state.zeron.list_plugins().await {
        for p in plugins {
            let label = format!("Plugin: {}", p.name.unwrap_or_else(|| p.id.clone()));
            if current_input.is_empty()
                || p.id.to_lowercase().contains(&current_input)
                || label.to_lowercase().contains(&current_input)
            {
                choices.push(AutocompleteChoice::new(label, p.id));
            }
        }
    }

    // 2. Default standard MCP servers
    for (id, desc) in DEFAULT_MCP_SERVERS {
        if choices.len() >= 25 {
            break;
        }
        let label = format!("{} - {}", id, desc);
        let display_label = if label.len() > 100 {
            format!("{}...", &label[..97])
        } else {
            label
        };

        if current_input.is_empty()
            || id.to_lowercase().contains(&current_input)
            || desc.to_lowercase().contains(&current_input)
        {
            choices.push(AutocompleteChoice::new(display_label, id.to_string()));
        }
    }

    choices.truncate(25);

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

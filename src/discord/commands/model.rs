use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("model")
        .description("Change the AI model for this channel / thread")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "name",
                "Model identifier from available Zeron models",
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
    let model_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "name")
        .and_then(|opt| opt.value.as_str());

    let model_name = match model_option {
        Some(m) => m.trim(),
        None => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `name`");
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    let channel_id = command.channel_id.get();

    // Ensure session exists or create it
    let session = match state.get_session(channel_id).await {
        Some(_) => state.set_model(channel_id, model_name).await?,
        None => {
            let _ = state
                .get_or_create_channel_session(
                    channel_id,
                    "general",
                    command.guild_id.map(|g| g.get()),
                )
                .await?;
            state.set_model(channel_id, model_name).await?
        }
    };

    let resp_text = format!(
        "🤖 **Model Updated**\n- **Session:** `{}`\n- **Harness:** `{}`\n- **Active Model:** `{}`\n- **Kind:** `{}`",
        session.chat_id,
        session.harness,
        session.model.as_deref().unwrap_or("default"),
        session.kind
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
        .find(|opt| opt.name == "name");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_lowercase(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let channel_id = autocomplete.channel_id.get();
    let session = state.get_session(channel_id).await;
    let harness = session
        .as_ref()
        .map(|s| s.harness.as_str())
        .unwrap_or(&state.config.default_harness);

    // Fetch models from Zeron MCP list_models
    let mut choices: Vec<AutocompleteChoice> = Vec::new();

    // Primary harness models
    if let Ok(models) = state.zeron.list_models(harness).await {
        for m in models {
            let label = m.label.unwrap_or_else(|| m.id.clone());
            let display_name = format!("{} ({})", label, harness);
            if current_input.is_empty()
                || m.id.to_lowercase().contains(&current_input)
                || display_name.to_lowercase().contains(&current_input)
            {
                choices.push(AutocompleteChoice::new(display_name, m.id));
            }
        }
    }

    // If still room, query connected harnesses
    if choices.len() < 25 {
        if let Ok(harnesses) = state.zeron.list_harnesses().await {
            for h in harnesses {
                if h.id != harness && h.connected {
                    if let Ok(models) = state.zeron.list_models(&h.id).await {
                        for m in models {
                            if choices.len() >= 25 {
                                break;
                            }
                            let label = m.label.unwrap_or_else(|| m.id.clone());
                            let display_name = format!("{} ({})", label, h.id);
                            if current_input.is_empty()
                                || m.id.to_lowercase().contains(&current_input)
                                || display_name.to_lowercase().contains(&current_input)
                            {
                                choices.push(AutocompleteChoice::new(display_name, m.id));
                            }
                        }
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

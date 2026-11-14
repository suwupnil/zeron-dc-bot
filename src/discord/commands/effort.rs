use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    EditInteractionResponse,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("effort")
        .description("Change effort")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "level", "thinking level")
                .required(true)
                .set_autocomplete(true),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    // Immediately defer ephemeral response
    command.defer_ephemeral(&ctx.http).await?;

    let level_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "level")
        .and_then(|opt| opt.value.as_str());

    let level_name = match level_option {
        Some(lvl) => lvl.trim(),
        None => {
            let edit = EditInteractionResponse::new()
                .content("❌ Missing required argument: `level`");
            command.edit_response(&ctx.http, edit).await?;
            return Ok(());
        }
    };

    let channel_id = command.channel_id.get();
    let existing = state.get_session(channel_id).await;

    let session = match existing {
        Some(s) if !s.is_closed => state.set_effort(channel_id, level_name).await?,
        _ => {
            let edit = EditInteractionResponse::new().content(
                "❌ No active Zeron session found for this channel.\n\
                 Use `/spawn <directory>` to spawn a project session or tag the bot to start a conversation.",
            );
            command.edit_response(&ctx.http, edit).await?;
            return Ok(());
        }
    };

    // Update topic to reflect <harness>/<model>.<effort>
    let _ = crate::discord::topic::update_channel_topic(ctx, channel_id, state).await;

    let edit = EditInteractionResponse::new().content(format!(
        "Effort level set to `{}`. Topic updated to `{}/{}.{}`",
        level_name,
        session.harness,
        session.model.as_deref().unwrap_or("default"),
        level_name
    ));
    command.edit_response(&ctx.http, edit).await?;

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
        .find(|opt| opt.name == "level");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_lowercase(),
            CommandDataOptionValue::Autocomplete { value, .. } => value.to_lowercase(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let channel_id = autocomplete.channel_id.get();
    let session = state.get_session(channel_id).await;

    let (harness, model) = match &session {
        Some(s) => (s.harness.clone(), s.model.clone()),
        None => (state.config.default_harness.clone(), None),
    };

    let mut levels: Vec<String> = Vec::new();

    if let Ok(models) = state.zeron.list_models(&harness).await {
        if let Some(target_model) = model {
            if let Some(m) = models.iter().find(|m| m.id == target_model) {
                levels = m.reasoning_levels.clone();
            }
        }
        if levels.is_empty() && !models.is_empty() {
            levels = models[0].reasoning_levels.clone();
        }
    }

    if levels.is_empty() {
        if let Ok(harnesses) = state.zeron.list_harnesses().await {
            if let Some(h) = harnesses.iter().find(|h| h.id == harness) {
                levels = h.reasoning_levels.clone();
            }
        }
    }

    if levels.is_empty() {
        levels = vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "max".to_string(),
        ];
    }

    let pref_effort = {
        let prefs = state.preferences.read().await;
        prefs.last_effort_by_harness.get(&harness).cloned()
    };

    let mut top_choices = Vec::new();
    let mut other_choices = Vec::new();

    for lvl in levels {
        if current_input.is_empty() || lvl.to_lowercase().contains(&current_input) {
            let is_top = pref_effort.as_deref() == Some(&lvl);
            let choice = AutocompleteChoice::new(lvl.clone(), lvl);
            if is_top {
                top_choices.push(choice);
            } else {
                other_choices.push(choice);
            }
        }
    }

    let mut choices: Vec<AutocompleteChoice> = Vec::new();
    choices.extend(top_choices);
    choices.extend(other_choices);
    choices.truncate(25);

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

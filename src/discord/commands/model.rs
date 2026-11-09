use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

pub fn register() -> CreateCommand {
    CreateCommand::new("model")
        .description("Change the harness, model or the effort level")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "harness", "Choose your harness")
                .required(true)
                .set_autocomplete(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "model",
                "Models in the selected harness (optional)",
            )
            .required(false)
            .set_autocomplete(true),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "effort", "Thinking level (optional)")
                .required(false)
                .set_autocomplete(true),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, state: &AppState) -> BotResult<()> {
    let harness_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "harness")
        .and_then(|opt| opt.value.as_str());

    let model_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "model")
        .and_then(|opt| opt.value.as_str());

    let effort_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "effort")
        .and_then(|opt| opt.value.as_str());

    let harness_name = match harness_option {
        Some(h) if !h.trim().is_empty() => h.trim(),
        _ => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `harness`")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    // If model is not chosen, fallback to last chosen for this harness, or default for this harness
    let resolved_model = match model_option {
        Some(m) if !m.trim().is_empty() => m.trim().to_string(),
        _ => state
            .get_preferred_model_for_harness(harness_name)
            .await
            .unwrap_or_else(|| "default".to_string()),
    };

    // If effort is not chosen, fallback to last chosen for this harness
    let resolved_effort = match effort_option {
        Some(e) if !e.trim().is_empty() => Some(e.trim().to_string()),
        _ => state.get_preferred_effort_for_harness(harness_name).await,
    };

    let channel_id = command.channel_id.get();

    // Verify session exists and is active (sessions should only be created via /spawn or @tag)
    let existing = state.get_session(channel_id).await;
    match existing {
        Some(s) if !s.is_closed => {
            state
                .set_harness_model_and_effort(
                    channel_id,
                    harness_name,
                    &resolved_model,
                    resolved_effort.as_deref(),
                )
                .await?;
        }
        _ => {
            let resp = CreateInteractionResponseMessage::new()
                .content(
                    "❌ No active Zeron session found for this channel.\n\
                     Use `/spawn <directory>` to spawn a project session or tag the bot to start a conversation.",
                )
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(resp))
                .await?;
            return Ok(());
        }
    };

    // Update channel topic to reflect newly selected <harness>/<model>.<effort>
    let _ = crate::discord::topic::update_channel_topic(ctx, channel_id, state).await;

    // Requirement 1: on model change, don't send the "🤖 Model Updated" message. Only update the topic.
    // Acknowledge interaction silently (ephemeral)
    let resp = CreateInteractionResponseMessage::new()
        .content(format!(
            "✅ Channel topic updated to `{}/{}.{}`",
            harness_name,
            resolved_model,
            resolved_effort.as_deref().unwrap_or("default")
        ))
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
    // Determine which field is currently focused by the user
    let focused_option =
        autocomplete
            .data
            .options
            .iter()
            .find(|opt| matches!(&opt.value, CommandDataOptionValue::Autocomplete { .. }))
            .or_else(|| {
                autocomplete.data.options.iter().find(|opt| {
                    opt.name == "effort" || opt.name == "model" || opt.name == "harness"
                })
            });

    let focused_name = focused_option
        .map(|opt| opt.name.as_str())
        .unwrap_or("harness");

    let current_input = match focused_option {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_lowercase(),
            CommandDataOptionValue::Autocomplete { value, .. } => value.to_lowercase(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let mut choices: Vec<AutocompleteChoice> = Vec::new();

    if focused_name == "harness" {
        // Query available harnesses from Zeron
        let pref_harness = {
            let prefs = state.preferences.read().await;
            prefs.last_harness.clone()
        };

        if let Ok(harnesses) = state.zeron.list_harnesses().await {
            let mut top_choices = Vec::new();
            let mut other_choices = Vec::new();

            for h in harnesses {
                // Rule 1: Exclude internal mock testing fixture
                if h.id == "mock" {
                    continue;
                }

                // Rule 2: DO NOT show the harness at all if not installed
                if !h.installed {
                    continue;
                }

                // Rule 3: If not signed in, show "(not signed in)"
                // Rule 4: Just the harness name if all status true
                let is_signed_in = h.signed_in.unwrap_or(false);
                let display_name = if !is_signed_in {
                    format!("{} (not signed in)", h.name)
                } else {
                    h.name.clone()
                };

                if current_input.is_empty()
                    || h.id.to_lowercase().contains(&current_input)
                    || h.name.to_lowercase().contains(&current_input)
                    || display_name.to_lowercase().contains(&current_input)
                {
                    let is_top = pref_harness.as_deref() == Some(&h.id);
                    let choice = AutocompleteChoice::new(display_name, h.id);
                    if is_top {
                        top_choices.push(choice);
                    } else {
                        other_choices.push(choice);
                    }
                }
            }

            choices.extend(top_choices);
            choices.extend(other_choices);
        }
    } else if focused_name == "model" {
        // "model" field autocomplete: list models in the chosen harness
        let selected_harness = autocomplete
            .data
            .options
            .iter()
            .find(|opt| opt.name == "harness")
            .and_then(|opt| match &opt.value {
                CommandDataOptionValue::String(s) if !s.trim().is_empty() => {
                    Some(s.trim().to_string())
                }
                _ => None,
            });

        let channel_id = autocomplete.channel_id.get();
        let session = state.get_session(channel_id).await;

        let harness = selected_harness.unwrap_or_else(|| {
            session
                .as_ref()
                .map(|s| s.harness.clone())
                .unwrap_or_else(|| state.config.default_harness.clone())
        });

        let pref_model = {
            let prefs = state.preferences.read().await;
            prefs.last_model_by_harness.get(&harness).cloned()
        };

        if let Ok(models) = state.zeron.list_models(&harness).await {
            let mut top_choices = Vec::new();
            let mut other_choices = Vec::new();

            for m in models {
                let label = m.label.unwrap_or_else(|| m.id.clone());
                let display_name = format!("{} ({})", label, harness);
                if current_input.is_empty()
                    || m.id.to_lowercase().contains(&current_input)
                    || display_name.to_lowercase().contains(&current_input)
                {
                    let is_top = pref_model.as_deref() == Some(&m.id);
                    let choice = AutocompleteChoice::new(display_name, m.id);
                    if is_top {
                        top_choices.push(choice);
                    } else {
                        other_choices.push(choice);
                    }
                }
            }

            choices.extend(top_choices);
            choices.extend(other_choices);
        }
    } else if focused_name == "effort" {
        // "effort" field autocomplete: dynamically fetch possible levels for harness / model
        let selected_harness = autocomplete
            .data
            .options
            .iter()
            .find(|opt| opt.name == "harness")
            .and_then(|opt| match &opt.value {
                CommandDataOptionValue::String(s) if !s.trim().is_empty() => {
                    Some(s.trim().to_string())
                }
                _ => None,
            });

        let selected_model = autocomplete
            .data
            .options
            .iter()
            .find(|opt| opt.name == "model")
            .and_then(|opt| match &opt.value {
                CommandDataOptionValue::String(s) if !s.trim().is_empty() => {
                    Some(s.trim().to_string())
                }
                _ => None,
            });

        let channel_id = autocomplete.channel_id.get();
        let session = state.get_session(channel_id).await;

        let harness = selected_harness.unwrap_or_else(|| {
            session
                .as_ref()
                .map(|s| s.harness.clone())
                .unwrap_or_else(|| state.config.default_harness.clone())
        });

        let pref_effort = {
            let prefs = state.preferences.read().await;
            prefs.last_effort_by_harness.get(&harness).cloned()
        };

        let mut levels: Vec<String> = Vec::new();

        // 1. Try to fetch model-specific reasoning levels from list_models
        if let Ok(models) = state.zeron.list_models(&harness).await {
            if let Some(target_model) = selected_model {
                if let Some(m) = models.iter().find(|m| m.id == target_model) {
                    levels = m.reasoning_levels.clone();
                }
            }
            if levels.is_empty() && !models.is_empty() {
                // fallback to first model's levels if model not yet selected
                levels = models[0].reasoning_levels.clone();
            }
        }

        // 2. If still empty, check harness reasoning levels from list_harnesses
        if levels.is_empty() {
            if let Ok(harnesses) = state.zeron.list_harnesses().await {
                if let Some(h) = harnesses.iter().find(|h| h.id == harness) {
                    levels = h.reasoning_levels.clone();
                }
            }
        }

        // 3. Fallback standard levels if harness reports none
        if levels.is_empty() {
            levels = vec![
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "max".to_string(),
            ];
        }

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

        choices.extend(top_choices);
        choices.extend(other_choices);
    }

    choices.truncate(25);

    let response = CreateAutocompleteResponse::new().set_choices(choices);
    autocomplete
        .create_response(&ctx.http, CreateInteractionResponse::Autocomplete(response))
        .await?;

    Ok(())
}

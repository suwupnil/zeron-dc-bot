use crate::error::BotResult;
use crate::state::AppState;
use serenity::all::{
    AutocompleteChoice, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateAutocompleteResponse, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use std::path::Path;

// Built-in standard skills catalog
const DEFAULT_SKILLS: &[(&str, &str)] = &[
    ("cloudflare", "Cloudflare infrastructure, Workers, KV, and networking setup"),
    ("hyperframes-cli", "HyperFrames CLI scaffold, edit timeline, render locally/cloud"),
    ("reddit-automation", "Authentic Reddit marketing and community discovery"),
    ("vercel-react-best-practices", "Vercel Next.js and React performance optimization guidelines"),
    ("code-refactoring", "Refactoring and code cleanup following idiomatic conventions"),
    ("unit-testing", "Comprehensive unit and integration testing suite generation"),
    ("security-audit", "Vulnerability scanning, dependency checking, and sandboxing analysis"),
    ("git-workflow", "Git commits, branching, rebasing, and pull request crafting"),
    ("doc-gen", "API documentation, README guides, and architecture diagrams"),
];

pub fn register() -> CreateCommand {
    CreateCommand::new("skills")
        .description("Choose and activate an agent skill for this channel / thread")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "skill",
                "Identifier of the skill or workflow to activate",
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
    let skill_option = command
        .data
        .options
        .iter()
        .find(|opt| opt.name == "skill")
        .and_then(|opt| opt.value.as_str());

    let skill_name = match skill_option {
        Some(s) => s.trim(),
        None => {
            let resp = CreateInteractionResponseMessage::new()
                .content("❌ Missing required argument: `skill`");
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

    // Inject skill directive to the Zeron session
    let prompt_msg = format!(
        "[System directive: Active skill enabled: '{}'. Prioritize the specialized rules, workflows, and guidelines associated with this skill during this session.]",
        skill_name
    );
    let _ = state.zeron.send_message(&session.chat_id, &prompt_msg, false).await;

    let resp_text = format!(
        "✨ **Skill Activated**\n- **Session:** `{}`\n- **Skill:** `{}`\n- **Status:** Injected into agent context\n- **Kind:** `{}`",
        session.chat_id, skill_name, session.kind
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
    _state: &AppState,
) -> BotResult<()> {
    let focused = autocomplete
        .data
        .options
        .iter()
        .find(|opt| opt.name == "skill");

    let current_input = match focused {
        Some(opt) => match &opt.value {
            CommandDataOptionValue::String(s) => s.as_str().to_lowercase(),
            _ => String::new(),
        },
        None => String::new(),
    };

    let mut choices: Vec<AutocompleteChoice> = Vec::new();

    // 1. Scan filesystem for local skill folders if present
    let possible_skill_dirs = [
        "/Users/swap/.gemini/config/skills",
        "./skills",
    ];

    for dir in &possible_skill_dirs {
        let p = Path::new(dir);
        if p.exists() && p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !name.starts_with('.') && (current_input.is_empty() || name.to_lowercase().contains(&current_input)) {
                                choices.push(AutocompleteChoice::new(format!("Local Skill: {}", name), name));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Default built-in skills
    for (id, desc) in DEFAULT_SKILLS {
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

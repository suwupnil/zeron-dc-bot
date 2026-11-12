use crate::discord::commands::{
    close, effort, help, interrupt, magpie, mcp, model, resume, skills, spawn, status, terminal,
};
use crate::discord::formatter::MessageChunker;
use crate::discord::topic;
use crate::state::{AppState, SessionOrigin};
use serenity::all::{
    ActivityData, Channel, ChannelType, Context, CreateMessage, EventHandler, GuildChannel,
    GuildId, Interaction, Message, Ready,
};
use tracing::{debug, error, info, warn};

fn extract_text_only(raw_content: &str) -> String {
    let mut words = Vec::new();
    for word in raw_content.split_whitespace() {
        let lower = word.to_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            let is_media_url = lower.contains("cdn.discordapp.com/attachments")
                || lower.contains("media.discordapp.net/attachments")
                || lower.contains("tenor.com")
                || lower.contains("giphy.com")
                || lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".gif")
                || lower.ends_with(".webp")
                || lower.ends_with(".mp4")
                || lower.ends_with(".webm")
                || lower.ends_with(".mov");
            if is_media_url {
                continue;
            }
        }
        words.push(word);
    }
    words.join(" ")
}

pub struct Handler {
    pub state: AppState,
}

impl Handler {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(
            "Logged in to Discord as: {} (ID: {})",
            ready.user.name, ready.user.id
        );

        ctx.set_activity(Some(ActivityData::watching("Zeron AI sessions")));

        let commands = vec![
            spawn::register(),
            close::register(),
            resume::register(),
            model::register(),
            effort::register(),
            skills::register(),
            mcp::register(),
            magpie::register(),
            terminal::register(),
            status::register(),
            interrupt::register(),
            help::register(),
        ];

        let joined_guilds: Vec<u64> = ready.guilds.iter().map(|g| g.id.get()).collect();
        info!(
            "Bot is currently in {} guild(s): {:?}",
            joined_guilds.len(),
            joined_guilds
        );

        if let Some(guild_id_u64) = self.state.config.guild_id {
            let guild_id = GuildId::new(guild_id_u64);
            if !joined_guilds.is_empty() && !joined_guilds.contains(&guild_id_u64) {
                warn!(
                    "Configured GUILD_ID ({}) is NOT in joined guilds {:?}. Please verify bot membership.",
                    guild_id_u64, joined_guilds
                );
            }

            info!(
                "Registering slash commands for development Guild: {}",
                guild_id
            );
            match guild_id.set_commands(&ctx.http, commands.clone()).await {
                Ok(registered) => {
                    info!(
                        "Registered {} guild slash commands successfully",
                        registered.len()
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to register guild slash commands: {}. Attempting global registration fallback...",
                        e
                    );
                    match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
                        Ok(registered) => {
                            info!(
                                "Registered {} global slash commands as fallback",
                                registered.len()
                            );
                        }
                        Err(ge) => {
                            error!("Failed to register global slash commands: {}", ge);
                        }
                    }
                }
            }
        } else {
            info!("Registering slash commands globally...");
            match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
                Ok(registered) => {
                    info!(
                        "Registered {} global slash commands successfully",
                        registered.len()
                    );
                }
                Err(e) => {
                    error!("Failed to register global slash commands: {}", e);
                }
            }
        }
    }

    // Do NOT listen for channel creations: sessions are only spawned via /spawn or when @tagged.

    async fn thread_create(&self, ctx: Context, thread: GuildChannel) {
        let thread_id = thread.id.get();
        let parent_id = match thread.parent_id {
            Some(pid) => pid.get(),
            None => {
                warn!(
                    "Thread {} has no parent_id; skipping side chat creation",
                    thread_id
                );
                return;
            }
        };

        // Only fork side chat if parent channel has an active project session (/spawn)
        let parent_session = match self.state.get_session(parent_id).await {
            Some(s) if !s.is_closed => s,
            _ => {
                debug!(
                    "Parent channel {} has no active session; skipping side chat for thread {}",
                    parent_id, thread_id
                );
                return;
            }
        };

        if parent_session.origin != SessionOrigin::Spawn {
            debug!(
                "Parent session in channel {} is casual (@tag); skipping side chat for thread {}",
                parent_id, thread_id
            );
            return;
        }

        info!(
            "New Discord thread detected: '{}' ({}) under parent project channel {}",
            thread.name, thread_id, parent_id
        );

        let guild_id = thread.guild_id.get();

        match self
            .state
            .get_or_create_thread_session(thread_id, &thread.name, parent_id, None, Some(guild_id))
            .await
        {
            Ok(session) => {
                info!(
                    "Forked Zeron side chat {} for thread '{}' (parent: {:?})",
                    session.chat_id, thread.name, session.parent_chat_id
                );

                let intro = format!(
                    "🧵 **Zeron Side Chat Initialized**\n\
                     - **Side Chat ID:** `{}`\n\
                     - **Parent Chat ID:** `{}`\n\
                     - **Working Directory:** `{}`\n\
                     This thread branches context from the parent channel. You can test changes, run tasks, and explore code here without polluting the main channel's session.",
                    session.chat_id,
                    session.parent_chat_id.as_deref().unwrap_or("unknown"),
                    session.project_dir
                );

                let _ = thread
                    .id
                    .send_message(&ctx.http, CreateMessage::new().content(intro))
                    .await;
            }
            Err(e) => {
                error!("Failed to fork side chat for thread {}: {}", thread.name, e);
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                let cmd_name = command.data.name.as_str();
                debug!("Handling slash command: /{}", cmd_name);

                // Check if channel has an active session
                let channel_id = command.channel_id.get();
                let existing_session = self.state.get_session(channel_id).await;
                let has_active_session = existing_session.as_ref().is_some_and(|s| !s.is_closed);

                // Requirement 4: Don't allow any command other than /spawn, /resume, /help, /magpie, /terminal if no zeron session is linked
                if !has_active_session
                    && cmd_name != "spawn"
                    && cmd_name != "resume"
                    && cmd_name != "help"
                    && cmd_name != "magpie"
                    && cmd_name != "terminal"
                {
                    let resp = serenity::all::CreateInteractionResponseMessage::new()
                        .content(
                            "❌ No active Zeron session linked with this channel.\n\
                             Please use `/spawn <directory>` to start a new session or `/resume [session_id]` to resume one.",
                        )
                        .ephemeral(true);
                    let _ = command
                        .create_response(
                            &ctx.http,
                            serenity::all::CreateInteractionResponse::Message(resp),
                        )
                        .await;
                    return;
                }

                let res = match cmd_name {
                    "spawn" => spawn::run(&ctx, &command, &self.state).await,
                    "close" => close::run(&ctx, &command, &self.state).await,
                    "resume" => resume::run(&ctx, &command, &self.state).await,
                    "model" => model::run(&ctx, &command, &self.state).await,
                    "effort" => effort::run(&ctx, &command, &self.state).await,
                    "skills" => skills::run(&ctx, &command, &self.state).await,
                    "mcp" => mcp::run(&ctx, &command, &self.state).await,
                    "magpie" => magpie::run(&ctx, &command, &self.state).await,
                    "terminal" => terminal::run(&ctx, &command, &self.state).await,
                    "status" => status::run(&ctx, &command, &self.state).await,
                    "interrupt" => interrupt::run(&ctx, &command, &self.state).await,
                    "help" => help::run(&ctx, &command).await,
                    _ => {
                        warn!("Unknown slash command: /{}", cmd_name);
                        Ok(())
                    }
                };

                if let Err(e) = res {
                    error!("Error executing slash command /{}: {}", cmd_name, e);
                }
            }

            Interaction::Autocomplete(autocomplete) => {
                let cmd_name = autocomplete.data.name.as_str();
                debug!("Handling autocomplete for command: /{}", cmd_name);

                let res = match cmd_name {
                    "spawn" => spawn::autocomplete(&ctx, &autocomplete, &self.state).await,
                    "model" => model::autocomplete(&ctx, &autocomplete, &self.state).await,
                    "effort" => effort::autocomplete(&ctx, &autocomplete, &self.state).await,
                    _ => Ok(()),
                };

                if let Err(e) = res {
                    warn!("Autocomplete error for /{}: {}", cmd_name, e);
                }
            }

            _ => {}
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages sent by bots
        if msg.author.bot {
            return;
        }

        let content = msg.content.trim();
        if content.is_empty() {
            return;
        }

        let bot_user_id = ctx.cache.current_user().id;
        let is_mentioned = msg.mentions.iter().any(|u| u.id == bot_user_id);

        // Check if message is a Discord reply to the bot
        let is_reply_to_bot = if let Some(ref r) = msg.referenced_message {
            r.author.id == bot_user_id
        } else if let Some(ref mref) = msg.message_reference {
            if let Some(msg_id) = mref.message_id {
                ctx.http
                    .get_message(msg.channel_id, msg_id)
                    .await
                    .map(|m| m.author.id == bot_user_id)
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let is_explicit = is_mentioned || is_reply_to_bot;
        let channel_id = msg.channel_id.get();

        // Check channel type
        let channel_info = msg.channel_id.to_channel(&ctx.http).await.ok();
        let (channel_name, is_thread, parent_channel_id) = match &channel_info {
            Some(Channel::Guild(gc)) => {
                let is_th = matches!(
                    gc.kind,
                    ChannelType::PublicThread
                        | ChannelType::PrivateThread
                        | ChannelType::NewsThread
                );
                (gc.name.clone(), is_th, gc.parent_id.map(|p| p.get()))
            }
            _ => ("chat".to_string(), false, None),
        };

        let existing_session = self.state.get_session(channel_id).await;
        let is_active = existing_session.as_ref().is_some_and(|s| !s.is_closed);

        // If channel is not currently active / monitored
        if !is_active {
            if is_mentioned && !is_thread {
                // Requirement 2a: bot is tagged for the first time in a chat (or after /close)
                // Spawns a casual archived session without any project.
                let guild_id = msg.guild_id.map(|g| g.get());
                info!(
                    "Spawning casual Zeron session for channel #{} due to @tag mention",
                    channel_name
                );
                if let Err(e) = self
                    .state
                    .spawn_tag_session(channel_id, &channel_name, guild_id)
                    .await
                {
                    error!("Failed to spawn casual tag session: {}", e);
                    let _ = msg
                        .reply(
                            &ctx.http,
                            format!("❌ Failed to initialize chat session: {}", e),
                        )
                        .await;
                    return;
                }
                // Update channel topic with model name on the first line
                let _ = topic::update_channel_topic(&ctx, channel_id, &self.state).await;
            } else {
                // Not mentioned or thread without session: do not monitor or respond
                return;
            }
        }

        // If channel IS active, but bot was NOT explicitly mentioned or replied-to:
        // Use the decision model (Cloudflare Workers AI or heuristic fallback)
        if is_active && !is_explicit {
            // Fetch last 10 messages from channel (excluding media, only sending text part)
            let get_msgs = serenity::all::GetMessages::new().limit(10);
            let history = msg
                .channel_id
                .messages(&ctx.http, get_msgs)
                .await
                .unwrap_or_default();

            let mut context_msgs: Vec<(String, String)> = Vec::new();
            for m in history.into_iter().rev() {
                let text_only = extract_text_only(&m.content);
                if !text_only.is_empty() {
                    context_msgs.push((m.author.name, text_only));
                }
            }

            let context_refs: Vec<(&str, &str)> = context_msgs
                .iter()
                .map(|(a, t)| (a.as_str(), t.as_str()))
                .collect();

            let text_content = extract_text_only(content);
            let bot_name = ctx.cache.current_user().name.clone();
            let should_respond = self
                .state
                .decision
                .should_respond(&context_refs, &text_content, Some(&bot_name))
                .await;

            if !should_respond {
                debug!(
                    "Decision model determined bot should not respond in channel {}",
                    channel_id
                );
                return;
            }
        }

        // Clean user prompt
        let clean_prompt = if is_mentioned {
            let mention_str = format!("<@{}>", bot_user_id);
            let mention_nick_str = format!("<@!{}>", bot_user_id);
            content
                .replace(&mention_str, "")
                .replace(&mention_nick_str, "")
                .trim()
                .to_string()
        } else if content.starts_with(&self.state.config.command_prefix) {
            content
                .trim_start_matches(&self.state.config.command_prefix)
                .trim()
                .to_string()
        } else {
            content.to_string()
        };

        let clean_text_prompt = extract_text_only(&clean_prompt);
        if clean_text_prompt.is_empty() {
            return;
        }

        // Retrieve or fork session for this channel or thread
        let guild_id = msg.guild_id.map(|g| g.get());
        let session = if is_thread {
            let parent_id = parent_channel_id.unwrap_or(channel_id);
            match self
                .state
                .get_or_create_thread_session(channel_id, &channel_name, parent_id, None, guild_id)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = msg
                        .reply(&ctx.http, format!("❌ Thread session unavailable: {}", e))
                        .await;
                    return;
                }
            }
        } else {
            match self.state.get_session(channel_id).await {
                Some(s) if !s.is_closed => s,
                _ => return,
            }
        };

        let is_first_tag_turn =
            session.origin == SessionOrigin::Tag && session.created_at == session.last_active_at;
        let handoff_context_opt = self.state.take_handoff_context(channel_id).await;
        self.state.touch_session(channel_id).await;

        let typing = msg.channel_id.start_typing(&ctx.http);

        info!(
            "Routing message from {} to Zeron session {} (origin: {:?}, kind: {})",
            msg.author.name, session.chat_id, session.origin, session.kind
        );

        let formatted_user_msg = if let Some(handoff) = handoff_context_opt {
            // If the user recently switched models/harnesses, inject the handoff context
            format!(
                "{}\n\n[Current User Message]:\n{}: {}",
                handoff, msg.author.name, clean_text_prompt
            )
        } else if is_first_tag_turn {
            format!(
                "[System instruction: Be conversational, natural and human-like. Do not use rigid formatting, bulleted lists, headers, or robotic grammar. Look like human generated, and send concise messages only unless asked otherwise.]\n\n{}: {}",
                msg.author.name, clean_text_prompt
            )
        } else {
            format!("{}: {}", msg.author.name, clean_text_prompt)
        };

        // Send to Zeron MCP
        let send_res = self
            .state
            .zeron
            .send_message(&session.chat_id, &formatted_user_msg, true)
            .await;

        drop(typing);

        match send_res {
            Ok(resp) => {
                let is_spawn_origin = session.origin == SessionOrigin::Spawn;

                // Read latest chat transcript messages including reasoning when origin is Spawn
                let (reply_text, reasoning_opt) = match self
                    .state
                    .zeron
                    .read_chat(&session.chat_id, 5, is_spawn_origin)
                    .await
                {
                    Ok(transcript) => {
                        let mut assistant_texts = Vec::new();
                        let mut latest_reasoning = None;

                        // Walk backwards from newest message to collect only the assistant's
                        // response to the current turn (stopping when reaching the user message)
                        for m in transcript.messages.iter().rev() {
                            if let Some(role) = m.get("role").and_then(|r| r.as_str()) {
                                if role == "assistant" {
                                    if latest_reasoning.is_none() {
                                        if let Some(r) = m.get("reasoning").and_then(|r| r.as_str())
                                        {
                                            let trimmed_r = r.trim();
                                            if !trimmed_r.is_empty() {
                                                latest_reasoning = Some(trimmed_r.to_string());
                                            }
                                        }
                                    }

                                    if let Some(text) = m.get("text").and_then(|t| t.as_str()) {
                                        assistant_texts.push(text.to_string());
                                    } else if let Some(c) =
                                        m.get("content").and_then(|c| c.as_str())
                                    {
                                        assistant_texts.push(c.to_string());
                                    }
                                } else if role == "user" {
                                    break;
                                }
                            }
                        }

                        assistant_texts.reverse();

                        let txt = if !assistant_texts.is_empty() {
                            assistant_texts.join("\n\n")
                        } else if let Some(r) = resp.reply {
                            r
                        } else {
                            "*(Action executed by Zeron)*".to_string()
                        };

                        (txt, latest_reasoning)
                    }
                    Err(_) => (
                        resp.reply
                            .unwrap_or_else(|| "*(Turn finished in Zeron)*".to_string()),
                        None,
                    ),
                };

                // Requirement 5: show thought process in Discord only in chats spawned by /spawn
                // All thought lines prefixed with -# so that it appears small in Discord
                let mut full_output = String::new();
                if is_spawn_origin {
                    if let Some(reasoning) = reasoning_opt {
                        let trimmed = reasoning.trim();
                        if !trimmed.is_empty() {
                            for line in trimmed.lines() {
                                let l = line.trim();
                                if l.is_empty() {
                                    full_output.push_str("-# \n");
                                } else {
                                    full_output.push_str(&format!("-# {}\n", l));
                                }
                            }
                            full_output.push('\n');
                        }
                    }
                }
                full_output.push_str(&reply_text);

                if full_output.trim().is_empty() {
                    full_output = "*(Done)*".to_string();
                }

                let can_reference = matches!(
                    msg.kind,
                    serenity::all::MessageType::Regular | serenity::all::MessageType::InlineReply
                );

                let chunks = MessageChunker::split_message(&full_output, 1950);
                for (i, chunk) in chunks.iter().enumerate() {
                    if chunk.trim().is_empty() {
                        continue;
                    }
                    let mut create_msg = CreateMessage::new().content(chunk);
                    if i == 0 && can_reference {
                        create_msg = create_msg.reference_message(&msg);
                    }
                    if let Err(e) = msg.channel_id.send_message(&ctx.http, create_msg).await {
                        warn!("Failed to send Discord reply with reference: {}. Retrying without reference...", e);
                        let fallback_msg = CreateMessage::new().content(chunk);
                        if let Err(fe) = msg.channel_id.send_message(&ctx.http, fallback_msg).await
                        {
                            error!("Failed to send Discord reply chunk: {}", fe);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Zeron send_message error: {}", e);
                let err_msg = format!(
                    "⚠️ **Zeron Error:** {}\nPlease verify that Zeron Engine is running locally.",
                    e
                );
                let _ = msg.reply(&ctx.http, err_msg).await;
            }
        }
    }
}

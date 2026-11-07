use crate::discord::commands::{dir, help, interrupt, mcp, model, reset, skills, status};
use crate::discord::formatter::MessageChunker;
use crate::state::AppState;
use serenity::all::{
    ActivityData, Channel, ChannelType, Context, CreateMessage, EventHandler, GuildChannel,
    GuildId, Interaction, Message, Ready,
};
use tracing::{debug, error, info, warn};

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
        info!("Logged in to Discord as: {} (ID: {})", ready.user.name, ready.user.id);

        ctx.set_activity(Some(ActivityData::watching("Zeron AI sessions")));

        let commands = vec![
            dir::register(),
            model::register(),
            mcp::register(),
            skills::register(),
            status::register(),
            reset::register(),
            interrupt::register(),
            help::register(),
        ];

        let joined_guilds: Vec<u64> = ready.guilds.iter().map(|g| g.id.get()).collect();
        info!("Bot is currently in {} guild(s): {:?}", joined_guilds.len(), joined_guilds);

        if let Some(guild_id_u64) = self.state.config.guild_id {
            let guild_id = GuildId::new(guild_id_u64);
            if !joined_guilds.is_empty() && !joined_guilds.contains(&guild_id_u64) {
                warn!(
                    "Configured GUILD_ID ({}) is NOT in the list of joined guilds {:?}. Please verify that the bot has joined this server or that GUILD_ID is not a channel/user ID.",
                    guild_id_u64, joined_guilds
                );
            }

            info!("Registering slash commands for development Guild: {}", guild_id);
            match guild_id.set_commands(&ctx.http, commands.clone()).await {
                Ok(registered) => {
                    info!("Registered {} guild slash commands successfully", registered.len());
                }
                Err(e) => {
                    error!("Failed to register guild slash commands: {}. Attempting global registration fallback...", e);
                    match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
                        Ok(registered) => {
                            info!("Registered {} global slash commands as fallback", registered.len());
                        }
                        Err(ge) => {
                            error!("Failed to register global slash commands: {}", ge);
                        }
                    }
                }
            }
        } else {
            info!("Registering slash commands globally (this can take up to an hour to propagate)...");
            match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
                Ok(registered) => {
                    info!("Registered {} global slash commands successfully", registered.len());
                }
                Err(e) => {
                    error!("Failed to register global slash commands: {}", e);
                }
            }
        }
    }

    async fn channel_create(&self, ctx: Context, channel: GuildChannel) {
        if !self.state.config.auto_create_sessions {
            return;
        }

        // If it's a thread, let thread_create handle it
        let is_thread = matches!(
            channel.kind,
            ChannelType::PublicThread | ChannelType::PrivateThread | ChannelType::NewsThread
        );
        if is_thread {
            return;
        }

        let channel_id = channel.id.get();
        let guild_id = channel.guild_id.get();
        info!("New text channel detected: #{} ({})", channel.name, channel_id);

        match self
            .state
            .get_or_create_channel_session(channel_id, &channel.name, Some(guild_id))
            .await
        {
            Ok(session) => {
                info!(
                    "Initialized Zeron chat session {} for channel #{}",
                    session.chat_id, channel.name
                );
                let welcome = format!(
                    "🤖 **Zeron Agent Session Initialized**\n\
                     - **Chat ID:** `{}`\n\
                     - **Working Dir:** `{}`\n\
                     - **Harness:** `{}`\n\
                     Use `/help` to see available commands or type a message to begin.",
                    session.chat_id, session.project_dir, session.harness
                );
                let _ = channel
                    .id
                    .send_message(&ctx.http, CreateMessage::new().content(welcome))
                    .await;
            }
            Err(e) => {
                error!("Failed to auto-create Zeron session for #{}: {}", channel.name, e);
            }
        }
    }

    async fn thread_create(&self, ctx: Context, thread: GuildChannel) {
        let thread_id = thread.id.get();
        let parent_id = match thread.parent_id {
            Some(pid) => pid.get(),
            None => {
                warn!("Thread {} has no parent_id; skipping side chat creation", thread_id);
                return;
            }
        };

        info!(
            "New Discord thread detected: '{}' ({}) under parent channel {}",
            thread.name, thread_id, parent_id
        );

        let guild_id = thread.guild_id.get();

        match self
            .state
            .get_or_create_thread_session(
                thread_id,
                &thread.name,
                parent_id,
                None,
                Some(guild_id),
            )
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
                     This thread branches context from the parent channel. You can run tasks, test changes, and ask questions here without interrupting or polluting the main channel's session.",
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

                let res = match cmd_name {
                    "dir" => dir::run(&ctx, &command, &self.state).await,
                    "model" => model::run(&ctx, &command, &self.state).await,
                    "mcp" => mcp::run(&ctx, &command, &self.state).await,
                    "skills" => skills::run(&ctx, &command, &self.state).await,
                    "status" => status::run(&ctx, &command, &self.state).await,
                    "reset" => reset::run(&ctx, &command, &self.state).await,
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
                    "dir" => dir::autocomplete(&ctx, &autocomplete, &self.state).await,
                    "model" => model::autocomplete(&ctx, &autocomplete, &self.state).await,
                    "mcp" => mcp::autocomplete(&ctx, &autocomplete, &self.state).await,
                    "skills" => skills::autocomplete(&ctx, &autocomplete, &self.state).await,
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

        // Check if message is a command or chat targeting the bot
        let bot_user_id = ctx.cache.current_user().id;
        let is_mentioned = msg.mentions.iter().any(|u| u.id == bot_user_id);
        let has_prefix = content.starts_with(&self.state.config.command_prefix);

        // Check channel type
        let channel_info = msg.channel_id.to_channel(&ctx.http).await.ok();
        let (channel_name, is_thread, parent_channel_id) = match &channel_info {
            Some(Channel::Guild(gc)) => {
                let is_th = matches!(
                    gc.kind,
                    ChannelType::PublicThread | ChannelType::PrivateThread | ChannelType::NewsThread
                );
                (gc.name.clone(), is_th, gc.parent_id.map(|p| p.get()))
            }
            _ => ("chat".to_string(), false, None),
        };

        let channel_id = msg.channel_id.get();

        // If not in a thread and not mentioned and doesn't start with prefix, check if channel already has an active session
        // In Discord, bots often reply when mentioned or when inside a dedicated thread/channel
        let clean_prompt = if is_mentioned {
            let mention_str = format!("<@{}>", bot_user_id);
            let mention_nick_str = format!("<@!{}>", bot_user_id);
            content
                .replace(&mention_str, "")
                .replace(&mention_nick_str, "")
                .trim()
                .to_string()
        } else if has_prefix {
            content
                .trim_start_matches(&self.state.config.command_prefix)
                .trim()
                .to_string()
        } else if is_thread {
            // Inside threads, all messages go to the thread side chat!
            content.to_string()
        } else {
            // In main channels, respond if session exists or if prefix/mention used
            if self.state.get_session(channel_id).await.is_none() {
                return;
            }
            content.to_string()
        };

        if clean_prompt.is_empty() {
            return;
        }

        // Retrieve or initialize session
        let guild_id = msg.guild_id.map(|g| g.get());
        let session = if is_thread {
            let parent_id = parent_channel_id.unwrap_or(channel_id);
            match self
                .state
                .get_or_create_thread_session(
                    channel_id,
                    &channel_name,
                    parent_id,
                    None,
                    guild_id,
                )
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = msg
                        .reply(&ctx.http, format!("❌ Failed to initialize thread session: {}", e))
                        .await;
                    return;
                }
            }
        } else {
            match self
                .state
                .get_or_create_channel_session(channel_id, &channel_name, guild_id)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = msg
                        .reply(&ctx.http, format!("❌ Failed to initialize channel session: {}", e))
                        .await;
                    return;
                }
            }
        };

        self.state.touch_session(channel_id).await;

        // Show typing indicator while Zeron processes
        let typing = msg.channel_id.start_typing(&ctx.http);

        info!(
            "Routing message from {} to Zeron session {} (kind: {})",
            msg.author.name, session.chat_id, session.kind
        );

        let formatted_user_msg = format!("{}: {}", msg.author.name, clean_prompt);

        // Send to Zeron and wait for turn
        let send_res = self
            .state
            .zeron
            .send_message(&session.chat_id, &formatted_user_msg, true)
            .await;

        drop(typing);

        match send_res {
            Ok(resp) => {
                // If reply was directly returned in SendMessageResponse
                let reply_text = if let Some(reply) = resp.reply {
                    reply
                } else {
                    // Otherwise fetch latest transcript from read_chat
                    match self.state.zeron.read_chat(&session.chat_id, 3).await {
                        Ok(transcript) => {
                            let mut assistant_texts = Vec::new();
                            for m in transcript.messages.iter().rev() {
                                if let Some(role) = m.get("role").and_then(|r| r.as_str()) {
                                    if role == "assistant" {
                                        if let Some(text) = m.get("text").and_then(|t| t.as_str()) {
                                            assistant_texts.push(text.to_string());
                                        } else if let Some(content) = m.get("content").and_then(|c| c.as_str()) {
                                            assistant_texts.push(content.to_string());
                                        }
                                    }
                                }
                            }
                            if !assistant_texts.is_empty() {
                                assistant_texts.join("\n\n")
                            } else {
                                "*(Action executed by Zeron)*".to_string()
                            }
                        }
                        Err(_) => "*(Turn finished in Zeron)*".to_string(),
                    }
                };

                let chunks = MessageChunker::split_message(&reply_text, 1950);
                for (i, chunk) in chunks.iter().enumerate() {
                    let mut create_msg = CreateMessage::new().content(chunk);
                    if i == 0 {
                        create_msg = create_msg.reference_message(&msg);
                    }
                    if let Err(e) = msg.channel_id.send_message(&ctx.http, create_msg).await {
                        error!("Failed to send Discord reply chunk: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Zeron send_message error: {}", e);
                let err_msg = format!("⚠️ **Zeron Error:** {}\nPlease verify that Zeron Engine is running locally.", e);
                let _ = msg.reply(&ctx.http, err_msg).await;
            }
        }
    }
}

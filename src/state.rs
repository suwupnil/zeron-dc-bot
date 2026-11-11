use crate::config::Config;
use crate::decision::DecisionClient;
use crate::error::{BotError, BotResult};
use crate::zeron::models::CreateChatArgs;
use crate::zeron::ZeronClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SessionOrigin {
    #[serde(rename = "spawn")]
    #[default]
    Spawn,
    #[serde(rename = "tag")]
    Tag,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_harness: Option<String>,
    #[serde(default)]
    pub last_model_by_harness: HashMap<String, String>,
    #[serde(default)]
    pub last_effort_by_harness: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub chat_id: String,
    pub kind: String, // "chat" or "side"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_chat_id: Option<String>,
    pub channel_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_channel_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<u64>,
    pub title: String,
    pub project_dir: String,
    pub harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub created_at: u64,
    pub last_active_at: u64,
    #[serde(default)]
    pub origin: SessionOrigin,
    #[serde(default)]
    pub is_closed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_context: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub zeron: ZeronClient,
    pub decision: DecisionClient,
    pub sessions: Arc<RwLock<HashMap<u64, SessionData>>>,
    pub preferences: Arc<RwLock<Preferences>>,
    storage_path: PathBuf,
    preferences_path: PathBuf,
}

impl AppState {
    pub fn new(config: Config, zeron: ZeronClient) -> Self {
        let storage_path = config.session_storage_path.clone();
        let preferences_path = storage_path
            .parent()
            .map(|p| p.join("preferences.json"))
            .unwrap_or_else(|| PathBuf::from("preferences.json"));
        let decision = DecisionClient::new(
            config.cloudflare_account_id.clone(),
            config.cloudflare_api_token.clone(),
            config.cloudflare_model.clone(),
            config.cloudflare_ai_endpoint.clone(),
        );
        Self {
            config,
            zeron,
            decision,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            preferences: Arc::new(RwLock::new(Preferences::default())),
            storage_path,
            preferences_path,
        }
    }

    /// Load persisted sessions and preferences from file on startup
    pub async fn load_sessions(&self) -> BotResult<()> {
        if self.preferences_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&self.preferences_path).await {
                if let Ok(prefs) = serde_json::from_str::<Preferences>(&content) {
                    let mut p = self.preferences.write().await;
                    *p = prefs;
                    info!("Restored preferences from {:?}", self.preferences_path);
                }
            }
        }

        if !self.storage_path.exists() {
            return Ok(());
        }

        match tokio::fs::read_to_string(&self.storage_path).await {
            Ok(content) => match serde_json::from_str::<HashMap<u64, SessionData>>(&content) {
                Ok(loaded) => {
                    let count = loaded.len();
                    let mut map = self.sessions.write().await;
                    *map = loaded;
                    info!(
                        "Restored {} Discord-Zeron session mappings from {:?}",
                        count, self.storage_path
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to deserialize session state file {:?}: {}",
                        self.storage_path, e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read session state file {:?}: {}",
                    self.storage_path, e
                );
            }
        }
        Ok(())
    }

    /// Persist current sessions to disk
    pub async fn save_sessions(&self) -> BotResult<()> {
        let map = self.sessions.read().await;
        let serialized = serde_json::to_string_pretty(&*map)?;
        if let Some(parent) = self.storage_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.storage_path, serialized).await?;
        debug!("Saved {} sessions to {:?}", map.len(), self.storage_path);
        Ok(())
    }

    /// Persist preferences to disk
    pub async fn save_preferences(&self) -> BotResult<()> {
        let prefs = self.preferences.read().await;
        let serialized = serde_json::to_string_pretty(&*prefs)?;
        if let Some(parent) = self.preferences_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.preferences_path, serialized).await?;
        debug!("Saved preferences to {:?}", self.preferences_path);
        Ok(())
    }

    /// Record chosen harness, model, and effort in user preferences
    pub async fn record_choice(
        &self,
        harness: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> BotResult<()> {
        let mut prefs = self.preferences.write().await;
        prefs.last_harness = Some(harness.to_string());
        if let Some(m) = model {
            prefs
                .last_model_by_harness
                .insert(harness.to_string(), m.to_string());
        }
        if let Some(e) = effort {
            prefs
                .last_effort_by_harness
                .insert(harness.to_string(), e.to_string());
        }
        drop(prefs);
        self.save_preferences().await
    }

    /// Get last chosen harness or fallback to default
    pub async fn get_preferred_harness(&self) -> String {
        let prefs = self.preferences.read().await;
        if let Some(ref h) = prefs.last_harness {
            return h.clone();
        }
        drop(prefs);
        self.resolve_harness().await
    }

    /// Get last chosen model for a given harness, or fallback to default model for that harness
    pub async fn get_preferred_model_for_harness(&self, harness: &str) -> Option<String> {
        let prefs = self.preferences.read().await;
        if let Some(m) = prefs.last_model_by_harness.get(harness) {
            return Some(m.clone());
        }
        drop(prefs);

        // Fallback: check config if default_harness matches
        if harness == self.config.default_harness {
            if let Some(ref m) = self.config.default_model {
                return Some(m.clone());
            }
        }

        // Fallback: query Zeron models catalog and take the first model
        if let Ok(models) = self.zeron.list_models(harness).await {
            if let Some(first) = models.into_iter().next() {
                return Some(first.id);
            }
        }

        None
    }

    /// Get last chosen effort for a given harness
    pub async fn get_preferred_effort_for_harness(&self, harness: &str) -> Option<String> {
        let prefs = self.preferences.read().await;
        prefs.last_effort_by_harness.get(harness).cloned()
    }

    /// Retrieve an existing session by channel ID
    pub async fn get_session(&self, channel_id: u64) -> Option<SessionData> {
        let map = self.sessions.read().await;
        map.get(&channel_id).cloned()
    }

    /// Determine active or connected harness
    pub async fn resolve_harness(&self) -> String {
        if let Ok(harnesses) = self.zeron.list_harnesses().await {
            if harnesses
                .iter()
                .any(|h| h.id == self.config.default_harness && h.connected)
            {
                self.config.default_harness.clone()
            } else if let Some(connected) = harnesses.iter().find(|h| h.connected) {
                connected.id.clone()
            } else {
                self.config.default_harness.clone()
            }
        } else {
            self.config.default_harness.clone()
        }
    }

    /// Spawn a project session assigned to a specific directory (via `/spawn`)
    /// with optional harness, model, and effort overrides (defaulting to last chosen or defaults)
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_project_session_with_config(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
        project_dir: &str,
        harness_opt: Option<&str>,
        model_opt: Option<&str>,
        effort_opt: Option<&str>,
    ) -> BotResult<SessionData> {
        // If an old session existed, archive it
        let old_chat_id = {
            let mut map = self.sessions.write().await;
            map.remove(&channel_id).map(|s| s.chat_id)
        };
        if let Some(old_id) = old_chat_id {
            let _ = self.zeron.archive_chat(&old_id, true).await;
        }

        // Determine harness: override -> last preferred -> resolve default
        let selected_harness = match harness_opt {
            Some(h) if !h.trim().is_empty() => h.trim().to_string(),
            _ => self.get_preferred_harness().await,
        };

        // Determine model: override -> last preferred for that harness -> default for that harness
        let selected_model = match model_opt {
            Some(m) if !m.trim().is_empty() => Some(m.trim().to_string()),
            _ => {
                self.get_preferred_model_for_harness(&selected_harness)
                    .await
            }
        };

        // Determine effort: override -> last preferred effort for that harness
        let selected_effort = match effort_opt {
            Some(e) if !e.trim().is_empty() => Some(e.trim().to_string()),
            _ => {
                self.get_preferred_effort_for_harness(&selected_harness)
                    .await
            }
        };

        // Record choices in persistent preferences
        let _ = self
            .record_choice(
                &selected_harness,
                selected_model.as_deref(),
                selected_effort.as_deref(),
            )
            .await;

        info!(
            "Spawning new project Zeron session for #{} ({}) in directory {} [{}/{:?}.{:?}]",
            channel_name,
            channel_id,
            project_dir,
            selected_harness,
            selected_model,
            selected_effort
        );

        let title = format!("#{} (Project)", channel_name);

        let create_args = CreateChatArgs {
            kind: Some("chat".to_string()),
            title: Some(title.clone()),
            cwd: Some(project_dir.to_string()),
            harness: Some(selected_harness.clone()),
            model: selected_model.clone(),
            reasoning: selected_effort.clone(),
            ..Default::default()
        };

        let resp = self.zeron.create_chat(create_args).await?;
        let now = current_timestamp();

        let session = SessionData {
            chat_id: resp.chat_id,
            kind: "chat".to_string(),
            parent_chat_id: None,
            channel_id,
            parent_channel_id: None,
            guild_id,
            title,
            project_dir: project_dir.to_string(),
            harness: selected_harness,
            model: selected_model,
            effort: selected_effort,
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Spawn,
            is_closed: false,
            handoff_context: None,
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(channel_id, session.clone());
        }

        self.save_sessions().await?;
        Ok(session)
    }

    /// Spawn a project session assigned to a specific directory (via `/spawn`) using defaults
    pub async fn spawn_project_session(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
        project_dir: &str,
    ) -> BotResult<SessionData> {
        self.spawn_project_session_with_config(
            channel_id,
            channel_name,
            guild_id,
            project_dir,
            None,
            None,
            None,
        )
        .await
    }

    /// Spawn a casual conversational session when tagged (@bot) without any project
    pub async fn spawn_tag_session(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
    ) -> BotResult<SessionData> {
        // If an old session existed, archive it
        let old_chat_id = {
            let mut map = self.sessions.write().await;
            map.remove(&channel_id).map(|s| s.chat_id)
        };
        if let Some(old_id) = old_chat_id {
            let _ = self.zeron.archive_chat(&old_id, true).await;
        }

        info!(
            "Spawning new casual tag Zeron session for #{} ({})",
            channel_name, channel_id
        );

        let default_harness = self.resolve_harness().await;
        let default_model = self.config.default_model.clone();
        let title = format!("#{} (Casual)", channel_name);

        let create_args = CreateChatArgs {
            kind: Some("chat".to_string()),
            title: Some(title.clone()),
            project: None,
            cwd: None,
            harness: Some(default_harness.clone()),
            model: default_model.clone(),
            ..Default::default()
        };

        let resp = self.zeron.create_chat(create_args).await?;

        // Archive session without project as specified in Requirement 2a
        let _ = self.zeron.archive_chat(&resp.chat_id, true).await;

        let now = current_timestamp();

        let session = SessionData {
            chat_id: resp.chat_id,
            kind: "chat".to_string(),
            parent_chat_id: None,
            channel_id,
            parent_channel_id: None,
            guild_id,
            title,
            project_dir: String::new(),
            harness: default_harness,
            model: default_model,
            effort: None,
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Tag,
            is_closed: false,
            handoff_context: None,
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(channel_id, session.clone());
        }

        self.save_sessions().await?;
        Ok(session)
    }

    /// Close active session and stop monitoring channel
    pub async fn close_session(&self, channel_id: u64) -> BotResult<SessionData> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&channel_id)
            .ok_or_else(|| BotError::Session("No active session found for this channel".into()))?;

        if session.is_closed {
            return Err(BotError::Session("Session is already closed".into()));
        }

        session.is_closed = true;
        let chat_id = session.chat_id.clone();
        let cloned = session.clone();
        drop(map);

        let _ = self.zeron.archive_chat(&chat_id, true).await;
        self.save_sessions().await?;
        Ok(cloned)
    }

    /// Resume monitoring a previously closed session, optionally attaching a specific Zeron session ID
    pub async fn resume_session(
        &self,
        channel_id: u64,
        specific_session_id: Option<&str>,
    ) -> BotResult<SessionData> {
        let mut map = self.sessions.write().await;

        if let Some(target_id) = specific_session_id {
            let target_id = target_id.trim();
            // Check if ANY OTHER channel is already linked to this session ID actively
            for (cid, s) in map.iter() {
                if *cid != channel_id && s.chat_id == target_id && !s.is_closed {
                    return Err(BotError::Session(format!(
                        "Session `{}` is already linked and active in another channel (<#{}>)",
                        target_id, cid
                    )));
                }
            }

            // Find session metadata if previously saved anywhere
            let previous_meta = map.values().find(|s| s.chat_id == target_id).cloned();

            // Terminate any active session on the current channel first
            if let Some(current) = map.get_mut(&channel_id) {
                current.is_closed = true;
            }

            let now = current_timestamp();
            let resumed_session = match previous_meta {
                Some(mut meta) => {
                    meta.channel_id = channel_id;
                    meta.is_closed = false;
                    meta.last_active_at = now;
                    meta
                }
                None => {
                    // Create an entry for the provided session id
                    SessionData {
                        chat_id: target_id.to_string(),
                        kind: "chat".to_string(),
                        parent_chat_id: None,
                        channel_id,
                        parent_channel_id: None,
                        guild_id: None,
                        title: format!("Resumed {}", target_id),
                        project_dir: self
                            .config
                            .default_project_dir
                            .to_string_lossy()
                            .to_string(),
                        harness: self.config.default_harness.clone(),
                        model: None,
                        effort: None,
                        created_at: now,
                        last_active_at: now,
                        origin: SessionOrigin::Spawn,
                        is_closed: false,
                        handoff_context: None,
                    }
                }
            };

            map.insert(channel_id, resumed_session.clone());
            drop(map);

            let _ = self.zeron.archive_chat(target_id, false).await;
            self.save_sessions().await?;
            return Ok(resumed_session);
        }

        let session = map.get_mut(&channel_id).ok_or_else(|| {
            BotError::Session(
                "No session found for this channel to resume. Provide a session id or /spawn."
                    .into(),
            )
        })?;

        if !session.is_closed {
            return Err(BotError::Session("Session is already active".into()));
        }

        session.is_closed = false;
        session.last_active_at = current_timestamp();
        let chat_id = session.chat_id.clone();
        let cloned = session.clone();
        drop(map);

        let _ = self.zeron.archive_chat(&chat_id, false).await;
        self.save_sessions().await?;
        Ok(cloned)
    }

    /// Get or create a main Zeron chat session (legacy/fallback helper)
    pub async fn get_or_create_channel_session(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
    ) -> BotResult<SessionData> {
        {
            let map = self.sessions.read().await;
            if let Some(session) = map.get(&channel_id) {
                if !session.is_closed {
                    return Ok(session.clone());
                }
            }
        }

        let default_dir = self
            .config
            .default_project_dir
            .to_string_lossy()
            .to_string();
        self.spawn_project_session(channel_id, channel_name, guild_id, &default_dir)
            .await
    }

    /// Get or create a side Zeron chat session for a Discord thread
    pub async fn get_or_create_thread_session(
        &self,
        thread_id: u64,
        thread_name: &str,
        parent_channel_id: u64,
        _parent_channel_name: Option<&str>,
        guild_id: Option<u64>,
    ) -> BotResult<SessionData> {
        {
            let map = self.sessions.read().await;
            if let Some(session) = map.get(&thread_id) {
                if !session.is_closed {
                    return Ok(session.clone());
                }
            }
        }

        // Ensure parent channel has an active project session
        let parent_session = self.get_session(parent_channel_id).await.ok_or_else(|| {
            BotError::Session("Parent channel has no Zeron session to branch from".into())
        })?;

        if parent_session.is_closed {
            return Err(BotError::Session(
                "Parent channel session is closed; cannot fork side chat".into(),
            ));
        }

        if parent_session.origin == SessionOrigin::Tag {
            return Err(BotError::Session(
                "Tag/casual sessions do not support side chats in threads".into(),
            ));
        }

        info!(
            "Forking side chat in Zeron for Discord thread '{}' ({}) from parent session {}",
            thread_name, thread_id, parent_session.chat_id
        );

        let title = format!("Thread: {} (Discord)", thread_name);

        let resp = self
            .zeron
            .create_side_chat(
                &parent_session.chat_id,
                Some(&title),
                Some(&parent_session.project_dir),
                parent_session.model.as_deref(),
                Some(&parent_session.harness),
            )
            .await?;

        let now = current_timestamp();

        let session = SessionData {
            chat_id: resp.chat_id,
            kind: "side".to_string(),
            parent_chat_id: Some(parent_session.chat_id),
            channel_id: thread_id,
            parent_channel_id: Some(parent_channel_id),
            guild_id,
            title,
            project_dir: parent_session.project_dir,
            harness: parent_session.harness,
            model: parent_session.model,
            effort: parent_session.effort,
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Spawn,
            is_closed: false,
            handoff_context: None,
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(thread_id, session.clone());
        }

        self.save_sessions().await?;
        Ok(session)
    }

    /// Update working directory for a channel/thread session
    pub async fn set_project_dir(&self, channel_id: u64, new_dir: &str) -> BotResult<SessionData> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&channel_id)
            .ok_or_else(|| BotError::Session("No active session found for this channel".into()))?;

        session.project_dir = new_dir.to_string();
        session.last_active_at = current_timestamp();
        let cloned = session.clone();
        drop(map);

        self.save_sessions().await?;
        Ok(cloned)
    }

    /// Update harness, model and effort for a channel/thread session.
    /// This archives the previous Zeron chat session and creates a fresh chat session
    /// in Zeron with the newly selected harness, model, and reasoning level, ensuring that
    /// the AI runner actually switches to the requested agent.
    pub async fn set_harness_model_and_effort(
        &self,
        channel_id: u64,
        harness_name: &str,
        model_name: &str,
        effort: Option<&str>,
    ) -> BotResult<SessionData> {
        let (
            old_chat_id,
            kind,
            parent_chat_id,
            _parent_channel_id,
            _guild_id,
            title,
            project_dir,
            origin,
        ) = {
            let map = self.sessions.read().await;
            let session = map.get(&channel_id).ok_or_else(|| {
                BotError::Session("No active session found for this channel".into())
            })?;

            (
                session.chat_id.clone(),
                session.kind.clone(),
                session.parent_chat_id.clone(),
                session.parent_channel_id,
                session.guild_id,
                session.title.clone(),
                session.project_dir.clone(),
                session.origin,
            )
        };

        // Extract recent conversation transcript from the old chat so the new model inherits
        // full memory of the conversation, exactly like Zeron's native provider handoff.
        let handoff_context = match self.zeron.read_chat(&old_chat_id, 30, false).await {
            Ok(transcript) if !transcript.messages.is_empty() => {
                let mut history_lines = Vec::new();
                for m in transcript.messages.iter() {
                    let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    // Ignore empty system switch markers
                    if role == "system" {
                        continue;
                    }
                    if let Some(text) = m.get("text").and_then(|t| t.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            if role == "assistant" {
                                history_lines.push(format!("Assistant: {}", trimmed));
                            } else {
                                history_lines.push(format!("User: {}", trimmed));
                            }
                        }
                    }
                }

                if !history_lines.is_empty() {
                    let formatted_history = history_lines.join("\n\n");
                    Some(format!(
                        "[System instruction: This conversation was started with a different AI provider and you are now taking over. \
The earlier messages are below as context; answer requests as the continuation of that conversation. \
Tool calls or actions from earlier ran on the previous provider; continue naturally from where the conversation left off.]\n\n\
--- Prior Conversation History ---\n\
{}\n\
--- End Prior Conversation History ---",
                        formatted_history
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };

        // Try creating the new Zeron chat session with the updated harness/model.
        // If Zeron daemon is unavailable (e.g. in offline unit tests), gracefully fall back
        // to retaining the existing chat_id while still updating the metadata.
        let new_chat_id_opt = if kind == "side" {
            if let Some(parent_id) = parent_chat_id.as_deref() {
                match self
                    .zeron
                    .create_side_chat(
                        parent_id,
                        Some(&title),
                        if project_dir.is_empty() {
                            None
                        } else {
                            Some(&project_dir)
                        },
                        Some(model_name),
                        Some(harness_name),
                    )
                    .await
                {
                    Ok(side_resp) => Some(side_resp.chat_id),
                    Err(e) => {
                        warn!("Could not spawn updated side chat in Zeron: {:?}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            // Main standalone chat
            let create_args = CreateChatArgs {
                kind: Some("chat".to_string()),
                title: Some(title.clone()),
                cwd: if project_dir.is_empty() {
                    None
                } else {
                    Some(project_dir.clone())
                },
                harness: Some(harness_name.to_string()),
                model: Some(model_name.to_string()),
                reasoning: effort.map(|e| e.to_string()),
                ..Default::default()
            };
            match self.zeron.create_chat(create_args).await {
                Ok(resp) => {
                    // If it was a casual tag session, archive it in Zeron so it doesn't clutter projects
                    if origin == SessionOrigin::Tag {
                        let _ = self.zeron.archive_chat(&resp.chat_id, true).await;
                    }
                    Some(resp.chat_id)
                }
                Err(e) => {
                    warn!("Could not spawn updated chat in Zeron: {:?}", e);
                    None
                }
            }
        };

        // If a new chat session was successfully created in Zeron, archive the old one
        if new_chat_id_opt.is_some() {
            let _ = self.zeron.archive_chat(&old_chat_id, true).await;
        }

        let now = current_timestamp();
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&channel_id)
            .ok_or_else(|| BotError::Session("No active session found for this channel".into()))?;

        if let Some(new_id) = new_chat_id_opt {
            session.chat_id = new_id;
        }
        session.harness = harness_name.to_string();
        session.model = Some(model_name.to_string());
        session.effort = effort.map(|e| e.to_string());
        session.handoff_context = handoff_context;
        session.last_active_at = now;
        let cloned = session.clone();
        drop(map);

        self.save_sessions().await?;
        let _ = self
            .record_choice(harness_name, Some(model_name), effort)
            .await;
        Ok(cloned)
    }

    /// Update effort level for an existing session
    pub async fn set_effort(&self, channel_id: u64, effort: &str) -> BotResult<SessionData> {
        let (harness_name, model_name) = {
            let map = self.sessions.read().await;
            let session = map.get(&channel_id).ok_or_else(|| {
                BotError::Session("No active session found for this channel".into())
            })?;
            (
                session.harness.clone(),
                session
                    .model
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
            )
        };

        self.set_harness_model_and_effort(channel_id, &harness_name, &model_name, Some(effort))
            .await
    }

    /// Update harness and model for a channel/thread session
    pub async fn set_harness_and_model(
        &self,
        channel_id: u64,
        harness_name: &str,
        model_name: &str,
    ) -> BotResult<SessionData> {
        self.set_harness_model_and_effort(channel_id, harness_name, model_name, None)
            .await
    }

    /// Update model for a channel/thread session
    pub async fn set_model(&self, channel_id: u64, model_name: &str) -> BotResult<SessionData> {
        let (harness_name, effort) = {
            let map = self.sessions.read().await;
            let session = map.get(&channel_id).ok_or_else(|| {
                BotError::Session("No active session found for this channel".into())
            })?;
            (session.harness.clone(), session.effort.clone())
        };

        self.set_harness_model_and_effort(channel_id, &harness_name, model_name, effort.as_deref())
            .await
    }

    /// Retrieve and clear any pending handoff context for the given channel.
    pub async fn take_handoff_context(&self, channel_id: u64) -> Option<String> {
        let mut map = self.sessions.write().await;
        if let Some(session) = map.get_mut(&channel_id) {
            let ctx = session.handoff_context.take();
            if ctx.is_some() {
                let _ = self.save_sessions().await;
            }
            ctx
        } else {
            None
        }
    }

    /// Reset / recreate a session for a channel or thread
    pub async fn reset_session(
        &self,
        channel_id: u64,
        name: &str,
        guild_id: Option<u64>,
        is_thread: bool,
        parent_channel_id: Option<u64>,
    ) -> BotResult<SessionData> {
        let old_chat_id = {
            let mut map = self.sessions.write().await;
            map.remove(&channel_id).map(|s| s.chat_id)
        };

        if let Some(old_id) = old_chat_id {
            let _ = self.zeron.archive_chat(&old_id, true).await;
        }

        if is_thread {
            let parent_id = parent_channel_id.unwrap_or(0);
            self.get_or_create_thread_session(channel_id, name, parent_id, None, guild_id)
                .await
        } else {
            self.get_or_create_channel_session(channel_id, name, guild_id)
                .await
        }
    }

    /// Mark session as active
    pub async fn touch_session(&self, channel_id: u64) {
        let mut map = self.sessions.write().await;
        if let Some(session) = map.get_mut(&channel_id) {
            session.last_active_at = current_timestamp();
        }
    }
}

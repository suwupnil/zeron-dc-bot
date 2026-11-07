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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionOrigin {
    #[serde(rename = "spawn")]
    Spawn,
    #[serde(rename = "tag")]
    Tag,
}

impl Default for SessionOrigin {
    fn default() -> Self {
        SessionOrigin::Spawn
    }
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
    pub created_at: u64,
    pub last_active_at: u64,
    #[serde(default)]
    pub origin: SessionOrigin,
    #[serde(default)]
    pub is_closed: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub zeron: ZeronClient,
    pub decision: DecisionClient,
    pub sessions: Arc<RwLock<HashMap<u64, SessionData>>>,
    storage_path: PathBuf,
}

impl AppState {
    pub fn new(config: Config, zeron: ZeronClient) -> Self {
        let storage_path = config.session_storage_path.clone();
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
            storage_path,
        }
    }

    /// Load persisted sessions from file on startup
    pub async fn load_sessions(&self) -> BotResult<()> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        match tokio::fs::read_to_string(&self.storage_path).await {
            Ok(content) => {
                match serde_json::from_str::<HashMap<u64, SessionData>>(&content) {
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
                }
            }
            Err(e) => {
                warn!("Failed to read session state file {:?}: {}", self.storage_path, e);
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

    /// Retrieve an existing session by channel ID
    pub async fn get_session(&self, channel_id: u64) -> Option<SessionData> {
        let map = self.sessions.read().await;
        map.get(&channel_id).cloned()
    }

    /// Determine active or connected harness
    pub async fn resolve_harness(&self) -> String {
        if let Ok(harnesses) = self.zeron.list_harnesses().await {
            if harnesses.iter().any(|h| h.id == self.config.default_harness && h.connected) {
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
    pub async fn spawn_project_session(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
        project_dir: &str,
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
            "Spawning new project Zeron session for #{} ({}) in directory {}",
            channel_name, channel_id, project_dir
        );

        let default_harness = self.resolve_harness().await;
        let default_model = self.config.default_model.clone();
        let title = format!("#{} (Project)", channel_name);

        let create_args = CreateChatArgs {
            kind: Some("chat".to_string()),
            title: Some(title.clone()),
            cwd: Some(project_dir.to_string()),
            harness: Some(default_harness.clone()),
            model: default_model.clone(),
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
            harness: default_harness,
            model: default_model,
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Spawn,
            is_closed: false,
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(channel_id, session.clone());
        }

        self.save_sessions().await?;
        Ok(session)
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

        let system_instructions = "[System instructions: Be conversational, natural and human-like. Do not use rigid markdown formatting, bulleted lists, headers, or robotic grammar. Keep your responses concise unless specifically asked otherwise.]".to_string();

        let create_args = CreateChatArgs {
            kind: Some("chat".to_string()),
            title: Some(title.clone()),
            project: None,
            cwd: None,
            harness: Some(default_harness.clone()),
            model: default_model.clone(),
            prompt: Some(system_instructions),
            wait: Some(false),
            ..Default::default()
        };

        let resp = self.zeron.create_chat(create_args).await?;

        // Archive session without project as specified
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
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Tag,
            is_closed: false,
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
        let session = map.get_mut(&channel_id).ok_or_else(|| {
            BotError::Session("No active session found for this channel".into())
        })?;

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

    /// Resume monitoring a previously closed session
    pub async fn resume_session(&self, channel_id: u64) -> BotResult<SessionData> {
        let mut map = self.sessions.write().await;
        let session = map.get_mut(&channel_id).ok_or_else(|| {
            BotError::Session("No session found for this channel to resume".into())
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

        let default_dir = self.config.default_project_dir.to_string_lossy().to_string();
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
            created_at: now,
            last_active_at: now,
            origin: SessionOrigin::Spawn,
            is_closed: false,
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

    /// Update model for a channel/thread session
    pub async fn set_model(&self, channel_id: u64, model_name: &str) -> BotResult<SessionData> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&channel_id)
            .ok_or_else(|| BotError::Session("No active session found for this channel".into()))?;

        session.model = Some(model_name.to_string());
        session.last_active_at = current_timestamp();
        let cloned = session.clone();
        drop(map);

        self.save_sessions().await?;
        Ok(cloned)
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

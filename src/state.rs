use crate::config::Config;
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
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub zeron: ZeronClient,
    pub sessions: Arc<RwLock<HashMap<u64, SessionData>>>,
    storage_path: PathBuf,
}

impl AppState {
    pub fn new(config: Config, zeron: ZeronClient) -> Self {
        let storage_path = config.session_storage_path.clone();
        Self {
            config,
            zeron,
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
                        info!("Restored {} Discord-Zeron session mappings from {:?}", count, self.storage_path);
                    }
                    Err(e) => {
                        warn!("Failed to deserialize session state file {:?}: {}", self.storage_path, e);
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

    /// Get or create a main Zeron chat session for a text channel
    pub async fn get_or_create_channel_session(
        &self,
        channel_id: u64,
        channel_name: &str,
        guild_id: Option<u64>,
    ) -> BotResult<SessionData> {
        {
            let map = self.sessions.read().await;
            if let Some(session) = map.get(&channel_id) {
                return Ok(session.clone());
            }
        }

        info!(
            "Initializing new Zeron session for text channel #{} ({})",
            channel_name, channel_id
        );

        let default_dir = self.config.default_project_dir.to_string_lossy().to_string();
        let default_harness = {
            if let Ok(harnesses) = self.zeron.list_harnesses().await {
                if harnesses.iter().any(|h| h.id == self.config.default_harness && h.connected) {
                    self.config.default_harness.clone()
                } else if let Some(connected) = harnesses.iter().find(|h| h.connected) {
                    info!(
                        "Configured harness '{}' is not connected; defaulting to connected harness '{}'",
                        self.config.default_harness, connected.id
                    );
                    connected.id.clone()
                } else {
                    self.config.default_harness.clone()
                }
            } else {
                self.config.default_harness.clone()
            }
        };
        let default_model = self.config.default_model.clone();

        let title = format!("#{} (Discord)", channel_name);

        let create_args = CreateChatArgs {
            kind: Some("chat".to_string()),
            title: Some(title.clone()),
            cwd: Some(default_dir.clone()),
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
            project_dir: default_dir,
            harness: default_harness,
            model: default_model,
            created_at: now,
            last_active_at: now,
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(channel_id, session.clone());
        }

        let _ = self.save_sessions().await;
        Ok(session)
    }

    /// Get or create a side Zeron chat session for a Discord thread
    pub async fn get_or_create_thread_session(
        &self,
        thread_id: u64,
        thread_name: &str,
        parent_channel_id: u64,
        parent_channel_name: Option<&str>,
        guild_id: Option<u64>,
    ) -> BotResult<SessionData> {
        {
            let map = self.sessions.read().await;
            if let Some(session) = map.get(&thread_id) {
                return Ok(session.clone());
            }
        }

        // Ensure parent channel has an active session to branch from
        let parent_name = parent_channel_name.unwrap_or("channel");
        let parent_session = self
            .get_or_create_channel_session(parent_channel_id, parent_name, guild_id)
            .await?;

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
        };

        {
            let mut map = self.sessions.write().await;
            map.insert(thread_id, session.clone());
        }

        let _ = self.save_sessions().await;
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

        let _ = self.save_sessions().await;
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

        let _ = self.save_sessions().await;
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
            let _ = self.zeron.archive_chat(&old_id).await;
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

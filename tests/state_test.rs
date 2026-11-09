use std::path::PathBuf;
use tempfile::tempdir;
use zeron_dc_bot::config::Config;
use zeron_dc_bot::state::{AppState, SessionData, SessionOrigin};
use zeron_dc_bot::zeron::ZeronClient;

fn make_test_config(storage_path: PathBuf) -> Config {
    Config {
        discord_token: "mock_token".to_string(),
        application_id: Some(12345),
        guild_id: None,
        zeron_path: PathBuf::from("/mock/zeron"),
        default_harness: "codex".to_string(),
        default_model: Some("gpt-6.1-sol".to_string()),
        default_project_dir: PathBuf::from("/tmp/test-project"),
        auto_create_sessions: true,
        command_prefix: "!z".to_string(),
        session_storage_path: storage_path,
        cloudflare_account_id: None,
        cloudflare_api_token: None,
        cloudflare_model: "@cf/cloudflare/clef-flash".to_string(),
        cloudflare_ai_endpoint: None,
        magpie_url: None,
    }
}

#[tokio::test]
async fn test_state_persistence_and_restore() {
    let temp_dir = tempdir().unwrap();
    let session_file = temp_dir.path().join("sessions.json");

    let config = make_test_config(session_file.clone());
    let zeron = ZeronClient::new(&config.zeron_path);
    let state = AppState::new(config.clone(), zeron.clone());

    // Manually insert a session into state
    {
        let mut map = state.sessions.write().await;
        map.insert(
            111222333,
            SessionData {
                chat_id: "chat-uuid-1".to_string(),
                kind: "chat".to_string(),
                parent_chat_id: None,
                channel_id: 111222333,
                parent_channel_id: None,
                guild_id: Some(999),
                title: "#general".to_string(),
                project_dir: "/tmp/project-a".to_string(),
                harness: "codex".to_string(),
                model: Some("gpt-6.1-sol".to_string()),
                effort: None,
                created_at: 1000,
                last_active_at: 1000,
                origin: SessionOrigin::Spawn,
                is_closed: false,
                handoff_context: None,
            },
        );
    }

    // Save to disk
    state
        .save_sessions()
        .await
        .expect("Failed to save sessions");
    assert!(session_file.exists());

    // Create a new AppState instance pointing to the same file and load
    let state2 = AppState::new(config, zeron);
    state2
        .load_sessions()
        .await
        .expect("Failed to load sessions");

    let restored = state2.get_session(111222333).await;
    assert!(restored.is_some());
    let s = restored.unwrap();
    assert_eq!(s.chat_id, "chat-uuid-1");
    assert_eq!(s.project_dir, "/tmp/project-a");
    assert_eq!(s.model, Some("gpt-6.1-sol".to_string()));
    assert_eq!(s.origin, SessionOrigin::Spawn);
    assert!(!s.is_closed);
}

#[tokio::test]
async fn test_state_dir_and_model_update() {
    let temp_dir = tempdir().unwrap();
    let session_file = temp_dir.path().join("sessions.json");

    let config = make_test_config(session_file);
    let zeron = ZeronClient::new(&config.zeron_path);
    let state = AppState::new(config, zeron);

    {
        let mut map = state.sessions.write().await;
        map.insert(
            444555,
            SessionData {
                chat_id: "chat-uuid-2".to_string(),
                kind: "chat".to_string(),
                parent_chat_id: None,
                channel_id: 444555,
                parent_channel_id: None,
                guild_id: None,
                title: "#dev".to_string(),
                project_dir: "/old/dir".to_string(),
                harness: "codex".to_string(),
                model: None,
                effort: None,
                created_at: 2000,
                last_active_at: 2000,
                origin: SessionOrigin::Spawn,
                is_closed: false,
                handoff_context: None,
            },
        );
    }

    // Update dir
    let updated = state
        .set_project_dir(444555, "/new/project/dir")
        .await
        .unwrap();
    assert_eq!(updated.project_dir, "/new/project/dir");

    // Update model
    let updated_model = state.set_model(444555, "claude-sonnet-5").await.unwrap();
    assert_eq!(updated_model.model, Some("claude-sonnet-5".to_string()));
}

#[tokio::test]
async fn test_state_close_and_resume_marking() {
    let temp_dir = tempdir().unwrap();
    let session_file = temp_dir.path().join("sessions.json");

    let config = make_test_config(session_file.clone());
    let zeron = ZeronClient::new(&config.zeron_path);
    let state = AppState::new(config.clone(), zeron.clone());

    {
        let mut map = state.sessions.write().await;
        map.insert(
            777888,
            SessionData {
                chat_id: "chat-uuid-3".to_string(),
                kind: "chat".to_string(),
                parent_chat_id: None,
                channel_id: 777888,
                parent_channel_id: None,
                guild_id: None,
                title: "#project".to_string(),
                project_dir: "/tmp/project-c".to_string(),
                harness: "codex".to_string(),
                model: None,
                effort: None,
                created_at: 3000,
                last_active_at: 3000,
                origin: SessionOrigin::Spawn,
                is_closed: false,
                handoff_context: None,
            },
        );
    }

    // Close session directly in state
    {
        let mut map = state.sessions.write().await;
        let s = map.get_mut(&777888).unwrap();
        s.is_closed = true;
    }
    state.save_sessions().await.unwrap();

    // Reload state
    let state2 = AppState::new(config, zeron);
    state2.load_sessions().await.unwrap();

    let s = state2.get_session(777888).await.unwrap();
    assert!(s.is_closed);
    assert_eq!(s.origin, SessionOrigin::Spawn);
}

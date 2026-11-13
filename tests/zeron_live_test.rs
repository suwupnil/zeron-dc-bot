use std::path::Path;
use zeron_dc_bot::zeron::models::CreateChatArgs;
use zeron_dc_bot::zeron::ZeronClient;

#[tokio::test]
async fn test_live_zeron_mcp_handshake_and_tools() {
    let zeron_path = Path::new("/Applications/Zeron.app/Contents/MacOS/zeron");
    if !zeron_path.exists() {
        eprintln!(
            "Zeron binary not found at {:?}, skipping live test",
            zeron_path
        );
        return;
    }

    let client = ZeronClient::new(zeron_path);

    // 1. Connect and initialize handshake
    let conn_res = client.connect().await;
    assert!(
        conn_res.is_ok(),
        "Failed to connect to Zeron MCP: {:?}",
        conn_res.err()
    );

    // 2. Query harnesses
    let harnesses = client
        .list_harnesses()
        .await
        .expect("Failed to list harnesses");
    assert!(!harnesses.is_empty(), "Expected at least one harness");
    let has_codex = harnesses.iter().any(|h| h.id == "codex");
    assert!(has_codex, "Expected 'codex' harness to be present");

    // 3. Query models for codex
    let models = client
        .list_models("codex")
        .await
        .expect("Failed to list models for codex");
    assert!(!models.is_empty(), "Expected models for codex");
    let has_sol = models
        .iter()
        .any(|m| m.id.contains("sol") || m.id.contains("gpt"));
    assert!(has_sol, "Expected GPT/Sol model in codex models");

    // 4. Query projects
    let projects_res = client.list_projects().await;
    assert!(projects_res.is_ok(), "list_projects should succeed");

    // 5. Create a temporary chat session with whatever harness is available
    let available_harness = harnesses
        .iter()
        .find(|h| h.available)
        .map(|h| h.id.clone())
        .unwrap_or_else(|| "codex".to_string());

    let create_args = CreateChatArgs {
        kind: Some("chat".to_string()),
        title: Some("CI Integration Test Chat".to_string()),
        harness: Some(available_harness.clone()),
        ..Default::default()
    };
    let chat = match client.create_chat(create_args).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Skipping live chat creation because no harness is active: {:?}",
                e
            );
            return;
        }
    };
    eprintln!("Created parent chat: {:?}", chat);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!chat.chat_id.is_empty());

    // 6. Create a side chat branching from it
    let side_chat = client
        .create_side_chat(
            &chat.chat_id,
            Some("CI Side Chat"),
            None,
            None,
            Some(&available_harness),
        )
        .await
        .expect("create_side_chat failed");
    assert!(!side_chat.chat_id.is_empty());
    assert_eq!(side_chat.parent_chat_id, Some(chat.chat_id.clone()));

    // 7. Verify switching harness/model updates chat_id and creates a fresh session
    let temp_dir = tempfile::tempdir().unwrap();
    let session_file = temp_dir.path().join("sessions.json");
    let config = zeron_dc_bot::config::Config {
        discord_token: "mock_token".to_string(),
        application_id: Some(12345),
        guild_id: None,
        zeron_path: zeron_path.to_path_buf(),
        default_harness: available_harness.clone(),
        default_model: None,
        default_project_dir: std::path::PathBuf::from("/tmp"),
        auto_create_sessions: true,
        command_prefix: "!z".to_string(),
        session_storage_path: session_file,
        cloudflare_account_id: None,
        cloudflare_api_token: None,
        cloudflare_refresh_token: None,
        cloudflare_model: "@cf/cloudflare/clef-flash".to_string(),
        cloudflare_ai_endpoint: None,
        magpie_url: None,
        update_secret: None,
        media_cache_dir: std::path::PathBuf::from("./media_cache"),
        media_max_image_bytes: 25_000_000,
        media_max_audio_bytes: 25_000_000,
        media_max_file_bytes: 50_000_000,
        ffmpeg_path: "ffmpeg".to_string(),
        stt_provider: "cloudflare".to_string(),
        stt_model: "@cf/openai/whisper".to_string(),
        groq_api_key: None,
        openai_api_key: None,
        ocr_provider: "cloudflare".to_string(),
        ocr_model: "@cf/llava-hf/llava-1.5-7b-hf".to_string(),
        tesseract_path: "tesseract".to_string(),
    };
    let state = zeron_dc_bot::state::AppState::new(config, client.clone());
    let initial_session = state
        .spawn_project_session(999888, "test-switch", None, "/tmp")
        .await
        .expect("spawn_project_session failed");
    let initial_chat_id = initial_session.chat_id.clone();

    // Query valid model for the available harness
    let valid_model = client
        .list_models(&available_harness)
        .await
        .ok()
        .and_then(|m| m.into_iter().next())
        .map(|m| m.id)
        .unwrap_or_else(|| "default".to_string());

    // Now switch harness/model
    let switched = state
        .set_harness_model_and_effort(999888, &available_harness, &valid_model, None)
        .await
        .expect("set_harness_model_and_effort failed");
    assert_ne!(
        switched.chat_id, initial_chat_id,
        "chat_id should change when model/harness is updated"
    );
    assert_eq!(switched.harness, available_harness);
    assert_eq!(switched.model, Some(valid_model));

    // Clean up
    let _ = client.archive_chat(&switched.chat_id, true).await;
    let _ = client.archive_chat(&initial_chat_id, true).await;
    let _ = client.archive_chat(&side_chat.chat_id, true).await;
    let _ = client.archive_chat(&chat.chat_id, true).await;
}

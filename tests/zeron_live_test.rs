use std::path::Path;
use zeron_dc_bot::zeron::models::CreateChatArgs;
use zeron_dc_bot::zeron::ZeronClient;

#[tokio::test]
async fn test_live_zeron_mcp_handshake_and_tools() {
    let zeron_path = Path::new("/Applications/Zeron.app/Contents/MacOS/zeron");
    if !zeron_path.exists() {
        eprintln!("Zeron binary not found at {:?}, skipping live test", zeron_path);
        return;
    }

    let client = ZeronClient::new(zeron_path);

    // 1. Connect and initialize handshake
    let conn_res = client.connect().await;
    assert!(conn_res.is_ok(), "Failed to connect to Zeron MCP: {:?}", conn_res.err());

    // 2. Query harnesses
    let harnesses = client.list_harnesses().await.expect("Failed to list harnesses");
    assert!(!harnesses.is_empty(), "Expected at least one harness");
    let has_codex = harnesses.iter().any(|h| h.id == "codex");
    assert!(has_codex, "Expected 'codex' harness to be present");

    // 3. Query models for codex
    let models = client.list_models("codex").await.expect("Failed to list models for codex");
    assert!(!models.is_empty(), "Expected models for codex");
    let has_sol = models.iter().any(|m| m.id.contains("sol") || m.id.contains("gpt"));
    assert!(has_sol, "Expected GPT/Sol model in codex models");

    // 4. Query projects
    let projects_res = client.list_projects().await;
    assert!(projects_res.is_ok(), "list_projects should succeed");

    // 5. Create a temporary chat session
    let create_args = CreateChatArgs {
        kind: Some("chat".to_string()),
        title: Some("CI Integration Test Chat".to_string()),
        harness: Some("codex".to_string()),
        ..Default::default()
    };
    let chat = client.create_chat(create_args).await.expect("create_chat failed");
    assert!(!chat.chat_id.is_empty());

    // 6. Create a side chat branching from it
    let side_chat = client
        .create_side_chat(&chat.chat_id, Some("CI Side Chat"), None, None, Some("codex"))
        .await
        .expect("create_side_chat failed");
    assert!(!side_chat.chat_id.is_empty());
    assert_eq!(side_chat.parent_chat_id, Some(chat.chat_id.clone()));

    // 7. Clean up by archiving both
    let _ = client.archive_chat(&side_chat.chat_id, true).await;
    let _ = client.archive_chat(&chat.chat_id, true).await;
}

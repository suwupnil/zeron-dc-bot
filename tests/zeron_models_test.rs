use serde_json::json;
use zeron_dc_bot::zeron::models::{
    CreateChatArgs, CreateChatResponse, JsonRpcRequest, JsonRpcResponse, SendMessageArgs,
    ToolCallResult,
};

#[test]
fn test_jsonrpc_request_serialization() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(42),
        method: "tools/call".to_string(),
        params: Some(json!({
            "name": "create_chat",
            "arguments": {
                "kind": "side",
                "parent": "parent-123",
                "title": "Side Branch"
            }
        })),
    };

    let serialized = serde_json::to_string(&req).unwrap();
    assert!(serialized.contains("\"id\":42"));
    assert!(serialized.contains("\"method\":\"tools/call\""));
    assert!(serialized.contains("\"parent\":\"parent-123\""));
}

#[test]
fn test_jsonrpc_response_deserialization() {
    let json_data = r#"{
        "jsonrpc": "2.0",
        "id": 42,
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": "{\"chatId\":\"chat-abc-123\",\"kind\":\"side\",\"parentChatId\":\"parent-123\"}"
                }
            ],
            "structuredContent": {
                "chatId": "chat-abc-123",
                "kind": "side",
                "parentChatId": "parent-123"
            },
            "isError": false
        }
    }"#;

    let resp: JsonRpcResponse = serde_json::from_str(json_data).unwrap();
    assert_eq!(resp.id, Some(42));
    assert!(resp.error.is_none());

    let result = resp.result.unwrap();
    let tool_res: ToolCallResult = serde_json::from_value(result).unwrap();
    assert!(!tool_res.is_error);

    let structured = tool_res.structured_content.unwrap();
    let chat_res: CreateChatResponse = serde_json::from_value(structured).unwrap();
    assert_eq!(chat_res.chat_id, "chat-abc-123");
    assert_eq!(chat_res.kind, Some("side".to_string()));
    assert_eq!(chat_res.parent_chat_id, Some("parent-123".to_string()));
}

#[test]
fn test_create_chat_args_side_chat() {
    let args = CreateChatArgs {
        kind: Some("side".to_string()),
        parent: Some("parent-uuid".to_string()),
        title: Some("Thread side chat".to_string()),
        cwd: Some("/path/to/project".to_string()),
        model: Some("gpt-6.1-sol".to_string()),
        harness: Some("codex".to_string()),
        ..Default::default()
    };

    let val = serde_json::to_value(&args).unwrap();
    assert_eq!(val["kind"], "side");
    assert_eq!(val["parent"], "parent-uuid");
    assert_eq!(val["title"], "Thread side chat");
    assert_eq!(val["cwd"], "/path/to/project");
}

#[test]
fn test_send_message_serialization() {
    let args = SendMessageArgs {
        chat: "chat-xyz".to_string(),
        text: "Please refactor the error handling module".to_string(),
        mode: Some("auto".to_string()),
        wait: Some(true),
        timeout_secs: Some(600),
    };

    let val = serde_json::to_value(&args).unwrap();
    assert_eq!(val["chat"], "chat-xyz");
    assert_eq!(val["wait"], true);
    assert_eq!(val["mode"], "auto");
}

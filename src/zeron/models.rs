use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    #[serde(default)]
    pub content: Vec<ToolCallContent>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateChatArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>, // "chat", "side", "subagent"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatResponse {
    #[serde(rename = "chatId", default)]
    pub chat_id: String,
    pub kind: Option<String>,
    #[serde(rename = "deviceId")]
    pub device_id: Option<String>,
    pub project: Option<String>,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "parentChatId")]
    pub parent_chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageArgs {
    pub chat: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>, // "auto", "run", "steer", "queue"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    #[serde(rename = "chatId")]
    pub chat_id: Option<String>,
    pub title: Option<String>,
    pub reply: Option<String>,
    pub sent: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitForTurnArgs {
    pub chat: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WaitForTurnResponse {
    pub status: Option<String>,
    pub replies: Option<Vec<String>>,
    #[serde(rename = "pendingInput")]
    pub pending_input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadChatArgs {
    pub chat: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tools: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadChatResponse {
    #[serde(rename = "chatId")]
    pub chat_id: Option<String>,
    pub title: Option<String>,
    pub total: Option<u64>,
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub connected: bool,
    #[serde(rename = "signedIn")]
    pub signed_in: Option<bool>,
    #[serde(rename = "reasoningLevels", default)]
    pub reasoning_levels: Vec<String>,
    #[serde(rename = "accessModes", default)]
    pub access_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "reasoningLevels", default)]
    pub reasoning_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDetails {
    pub id: String,
    pub title: Option<String>,
    pub project: Option<String>,
    pub cwd: Option<String>,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "parentChatId")]
    pub parent_chat_id: Option<String>,
    #[serde(rename = "childKind")]
    pub child_kind: Option<String>,
    pub archived: Option<bool>,
}

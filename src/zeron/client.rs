use crate::error::{BotError, BotResult};
use crate::zeron::models::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{debug, info, warn};

pub struct ZeronClientInner {
    stdin: Mutex<ChildStdin>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<BotResult<JsonRpcResponse>>>>>,
    next_id: AtomicU64,
    _child: Mutex<Child>,
}

#[derive(Clone)]
pub struct ZeronClient {
    binary_path: PathBuf,
    inner: Arc<RwLock<Option<Arc<ZeronClientInner>>>>,
}

impl ZeronClient {
    pub fn new<P: AsRef<Path>>(binary_path: P) -> Self {
        Self {
            binary_path: binary_path.as_ref().to_path_buf(),
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Ensure the MCP child process is running and initialized
    pub async fn connect(&self) -> BotResult<()> {
        let mut inner_guard = self.inner.write().await;
        if inner_guard.is_some() {
            return Ok(());
        }

        info!(
            "Spawning Zeron MCP daemon process: {:?}",
            self.binary_path
        );

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            BotError::ZeronProcess(format!(
                "Failed to spawn Zeron at {:?}: {}",
                self.binary_path, e
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BotError::ZeronProcess("Failed to open stdin for zeron mcp".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BotError::ZeronProcess("Failed to open stdout for zeron mcp".into()))?;
        let stderr = child.stderr.take();

        // Spawn a background task to log stderr
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    debug!(target: "zeron::stderr", "{}", line);
                }
            });
        }

        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<BotResult<JsonRpcResponse>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = Arc::clone(&pending_requests);

        // Spawn background task to process stdout JSON-RPC responses
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                debug!(target: "zeron::stdout", "{}", trimmed);

                match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    Ok(resp) => {
                        if let Some(id) = resp.id {
                            let mut map = pending_clone.lock().await;
                            if let Some(tx) = map.remove(&id) {
                                let _ = tx.send(Ok(resp));
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            target: "zeron::stdout",
                            "Failed to parse JSON-RPC response from zeron mcp: {}. Raw: {}",
                            e, trimmed
                        );
                    }
                }
            }
            warn!("Zeron stdout stream ended");
            // Fail all pending
            let mut map = pending_clone.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(BotError::ZeronProcess(
                    "Zeron process stdout closed unexpectedly".into(),
                )));
            }
        });

        let inner = Arc::new(ZeronClientInner {
            stdin: Mutex::new(stdin),
            pending_requests,
            next_id: AtomicU64::new(1),
            _child: Mutex::new(child),
        });

        // Initialize handshake
        let init_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "zeron-dc-bot",
                    "version": "0.1.0"
                }
            })),
        };

        // Send init
        let init_res = Self::send_raw_request_internal(&inner, init_req).await?;
        if let Some(err) = init_res.error {
            return Err(BotError::ZeronMcp(format!(
                "Initialize error: {} ({})",
                err.message, err.code
            )));
        }

        // Send notifications/initialized
        let notify = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        Self::send_notification_internal(&inner, notify).await?;

        info!("Successfully initialized Zeron MCP connection");
        *inner_guard = Some(inner);
        Ok(())
    }

    async fn get_inner(&self) -> BotResult<Arc<ZeronClientInner>> {
        {
            let guard = self.inner.read().await;
            if let Some(inner) = guard.as_ref() {
                return Ok(Arc::clone(inner));
            }
        }
        // If not connected, connect
        self.connect().await?;
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| BotError::ZeronProcess("Failed to acquire Zeron connection".into()))
    }

    async fn send_raw_request_internal(
        inner: &ZeronClientInner,
        req: JsonRpcRequest,
    ) -> BotResult<JsonRpcResponse> {
        let req_id = req
            .id
            .ok_or_else(|| BotError::ZeronMcp("Request missing id".into()))?;

        let (tx, rx) = oneshot::channel();
        {
            let mut map = inner.pending_requests.lock().await;
            map.insert(req_id, tx);
        }

        let mut serialized = serde_json::to_string(&req)?;
        serialized.push('\n');

        {
            let mut stdin = inner.stdin.lock().await;
            stdin.write_all(serialized.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait with a configurable timeout (default 30 seconds for normal requests)
        match tokio::time::timeout(Duration::from_secs(600), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => {
                let mut map = inner.pending_requests.lock().await;
                map.remove(&req_id);
                Err(BotError::ZeronMcp("Channel dropped before response received".into()))
            }
            Err(_) => {
                let mut map = inner.pending_requests.lock().await;
                map.remove(&req_id);
                Err(BotError::Timeout)
            }
        }
    }

    async fn send_notification_internal(
        inner: &ZeronClientInner,
        req: JsonRpcRequest,
    ) -> BotResult<()> {
        let mut serialized = serde_json::to_string(&req)?;
        serialized.push('\n');
        let mut stdin = inner.stdin.lock().await;
        stdin.write_all(serialized.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Generic MCP tools/call invoker
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> BotResult<serde_json::Value> {
        let inner = self.get_inner().await?;
        let id = inner.next_id.fetch_add(1, Ordering::SeqCst);

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            })),
        };

        let response = Self::send_raw_request_internal(&inner, req).await?;

        if let Some(err) = response.error {
            return Err(BotError::ZeronMcp(format!(
                "MCP tool '{}' returned error {}: {}",
                tool_name, err.code, err.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| BotError::ZeronMcp("Missing result in response".into()))?;

        let tool_res: ToolCallResult = serde_json::from_value(result.clone()).map_err(|e| {
            BotError::ZeronMcp(format!(
                "Failed to deserialize ToolCallResult for '{}': {}. Raw: {}",
                tool_name, e, result
            ))
        })?;

        if tool_res.is_error {
            let error_text = tool_res
                .content
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Err(BotError::ZeronMcp(format!(
                "Tool '{}' reported error: {}",
                tool_name, error_text
            )));
        }

        // Return structuredContent if present
        if let Some(structured) = tool_res.structured_content {
            return Ok(structured);
        }

        // Otherwise parse content[0].text as JSON
        if let Some(first_content) = tool_res.content.first() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&first_content.text) {
                return Ok(parsed);
            }
            return Ok(serde_json::Value::String(first_content.text.clone()));
        }

        Ok(serde_json::Value::Null)
    }

    /// Initialize a new main chat session in Zeron
    pub async fn create_chat(&self, args: CreateChatArgs) -> BotResult<CreateChatResponse> {
        let val = serde_json::to_value(&args)?;
        let res = self.call_tool("create_chat", val).await?;
        let chat_res: CreateChatResponse = serde_json::from_value(res.clone()).map_err(|e| {
            BotError::ZeronMcp(format!(
                "Failed to parse create_chat response: {}. Raw: {}",
                e, res
            ))
        })?;
        Ok(chat_res)
    }

    /// Initialize a side chat in Zeron branching from an existing parent chat
    pub async fn create_side_chat(
        &self,
        parent_chat_id: &str,
        title: Option<&str>,
        cwd: Option<&str>,
        model: Option<&str>,
        harness: Option<&str>,
    ) -> BotResult<CreateChatResponse> {
        let args = CreateChatArgs {
            kind: Some("side".to_string()),
            parent: Some(parent_chat_id.to_string()),
            title: title.map(|s| s.to_string()),
            cwd: cwd.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            harness: harness.map(|s| s.to_string()),
            ..Default::default()
        };
        self.create_chat(args).await
    }

    /// Send a message to a Zeron chat session
    pub async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        wait: bool,
    ) -> BotResult<SendMessageResponse> {
        let args = SendMessageArgs {
            chat: chat_id.to_string(),
            text: text.to_string(),
            mode: Some("auto".to_string()),
            wait: Some(wait),
            timeout_secs: Some(600),
        };
        let val = serde_json::to_value(&args)?;
        let res = self.call_tool("send_message", val).await?;
        let send_res: SendMessageResponse = serde_json::from_value(res.clone()).map_err(|e| {
            BotError::ZeronMcp(format!(
                "Failed to parse send_message response: {}. Raw: {}",
                e, res
            ))
        })?;
        Ok(send_res)
    }

    /// Wait for a turn to complete and collect assistant replies
    pub async fn wait_for_turn(
        &self,
        chat_id: &str,
        timeout_secs: Option<u64>,
    ) -> BotResult<WaitForTurnResponse> {
        let args = WaitForTurnArgs {
            chat: chat_id.to_string(),
            timeout_secs,
        };
        let val = serde_json::to_value(&args)?;
        let res = self.call_tool("wait_for_turn", val).await?;
        let wait_res: WaitForTurnResponse = serde_json::from_value(res.clone()).unwrap_or_default();
        Ok(wait_res)
    }

    /// Read transcript messages from a chat
    pub async fn read_chat(
        &self,
        chat_id: &str,
        limit: u32,
        include_reasoning: bool,
    ) -> BotResult<ReadChatResponse> {
        let args = ReadChatArgs {
            chat: chat_id.to_string(),
            limit: Some(limit),
            offset: Some(0),
            include_reasoning: Some(include_reasoning),
            include_tools: Some(true),
        };
        let val = serde_json::to_value(&args)?;
        let res = self.call_tool("read_chat", val).await?;
        let read_res: ReadChatResponse = serde_json::from_value(res.clone()).map_err(|e| {
            BotError::ZeronMcp(format!("Failed to parse read_chat: {}. Raw: {}", e, res))
        })?;
        Ok(read_res)
    }

    /// Retrieve chat metadata and status
    pub async fn get_chat(&self, chat_id: &str) -> BotResult<ChatDetails> {
        let args = serde_json::json!({ "chat": chat_id });
        let res = self.call_tool("get_chat", args).await?;
        let details: ChatDetails = serde_json::from_value(res.clone()).map_err(|e| {
            BotError::ZeronMcp(format!("Failed to parse get_chat: {}. Raw: {}", e, res))
        })?;
        Ok(details)
    }

    /// Interrupt a running turn in a chat
    pub async fn interrupt_chat(&self, chat_id: &str) -> BotResult<()> {
        let args = serde_json::json!({ "chat": chat_id });
        let _ = self.call_tool("interrupt_chat", args).await?;
        Ok(())
    }

    /// Archive or unarchive a chat
    pub async fn archive_chat(&self, chat_id: &str, archived: bool) -> BotResult<()> {
        let args = serde_json::json!({ "chat": chat_id, "archived": archived });
        let _ = self.call_tool("archive_chat", args).await?;
        Ok(())
    }

    /// Query available harnesses
    pub async fn list_harnesses(&self) -> BotResult<Vec<HarnessInfo>> {
        let res = self
            .call_tool("list_harnesses", serde_json::json!({}))
            .await?;
        let harnesses_val = res
            .get("harnesses")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let harnesses: Vec<HarnessInfo> = serde_json::from_value(harnesses_val)?;
        Ok(harnesses)
    }

    /// Query available models for a given harness
    pub async fn list_models(&self, harness: &str) -> BotResult<Vec<ModelInfo>> {
        let res = self
            .call_tool("list_models", serde_json::json!({ "harness": harness }))
            .await?;
        let models_val = res
            .get("models")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let models: Vec<ModelInfo> = serde_json::from_value(models_val)?;
        Ok(models)
    }

    /// Query registered projects
    pub async fn list_projects(&self) -> BotResult<Vec<ProjectInfo>> {
        let res = self
            .call_tool("list_projects", serde_json::json!({}))
            .await?;
        let projects_val = res
            .get("projects")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let projects: Vec<ProjectInfo> = serde_json::from_value(projects_val)?;
        Ok(projects)
    }

    /// Query registered plugins
    pub async fn list_plugins(&self) -> BotResult<Vec<PluginInfo>> {
        let res = self
            .call_tool("list_plugins", serde_json::json!({}))
            .await?;
        let plugins_val = res
            .get("plugins")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let plugins: Vec<PluginInfo> = serde_json::from_value(plugins_val)?;
        Ok(plugins)
    }
}

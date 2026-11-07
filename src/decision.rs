use reqwest::Client;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct DecisionClient {
    http: Client,
    pub account_id: Option<String>,
    pub api_token: Option<String>,
    pub model: String,
    pub endpoint: Option<String>,
}

impl DecisionClient {
    pub fn new(
        account_id: Option<String>,
        api_token: Option<String>,
        model: String,
        endpoint: Option<String>,
    ) -> Self {
        Self {
            http: Client::new(),
            account_id,
            api_token,
            model,
            endpoint,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.endpoint.is_some() || (self.account_id.is_some() && self.api_token.is_some())
    }

    /// Evaluate whether the assistant should intervene and reply to the latest message.
    pub async fn should_respond(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
    ) -> bool {
        if self.is_configured() {
            match self.call_cloudflare(messages_context, latest_message).await {
                Ok(decision) => {
                    info!("Cloudflare decision model verdict: {}", decision);
                    return decision;
                }
                Err(e) => {
                    warn!(
                        "Cloudflare decision model request failed: {}. Falling back to heuristic.",
                        e
                    );
                }
            }
        }

        let verdict = Self::fallback_heuristic(messages_context, latest_message);
        info!("Decision fallback heuristic verdict: {}", verdict);
        verdict
    }

    async fn call_cloudflare(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let url = if let Some(ref ep) = self.endpoint {
            ep.clone()
        } else {
            let acct = self.account_id.as_deref().unwrap_or_default();
            format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
                acct, self.model
            )
        };

        let mut formatted_context = String::new();
        for (author, text) in messages_context {
            formatted_context.push_str(&format!("{}: {}\n", author, text));
        }

        let system_prompt = "You are a binary classification decision model for an AI coding and assistant bot in a multi-user Discord channel.\n\
Users in the channel frequently talk amongst themselves. In those cases, the bot must NOT respond.\n\
The bot should only respond if a user is asking a question, asking for help/code/explanations, directing a task to the bot, or seeking assistant input.\n\
If people are talking to each other or having conversations that do not require an AI assistant, reply 'NO'.\n\
Respond strictly with ONLY the single word 'YES' or 'NO'.";

        let user_prompt = format!(
            "Recent channel text messages (up to 10):\n{}\n\nLatest message: \"{}\"\nShould the AI bot respond? (YES/NO):",
            formatted_context, latest_message
        );

        let body = serde_json::json!({
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "max_tokens": 10
        });

        let mut req = self.http.post(&url).json(&body);
        if let Some(ref token) = self.api_token {
            req = req.bearer_auth(token);
        }

        let res = req.timeout(std::time::Duration::from_secs(5)).send().await?;
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            return Err(format!("Cloudflare API error status {}: {}", status, text).into());
        }

        let json: serde_json::Value = res.json().await?;
        let response_text = json
            .pointer("/result/response")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("response").and_then(|v| v.as_str()))
            .unwrap_or_default()
            .trim();

        debug!("Cloudflare decision raw response: {}", response_text);
        let upper = response_text.to_uppercase();
        Ok(upper.starts_with("YES"))
    }

    /// Smart fallback heuristic when Cloudflare Workers AI credentials are not configured.
    pub fn fallback_heuristic(
        _messages_context: &[(&str, &str)],
        latest_message: &str,
    ) -> bool {
        let text = latest_message.trim().to_lowercase();
        if text.is_empty() {
            return false;
        }

        // Direct mention or assistant-targeted cues
        let direct_cues = [
            "zeron", "bot", "assistant", "ai",
            "can you", "could you", "would you", "please",
            "how do i", "how to", "what is", "why does", "fix this",
            "explain", "help me", "write a", "create a", "build a",
            "debug", "refactor",
        ];

        for cue in &direct_cues {
            if text.contains(cue) {
                return true;
            }
        }

        // Question mark with interrogative starters
        if text.contains('?') {
            let question_starters = [
                "what", "how", "why", "where", "when", "who", "which",
                "can", "is", "does", "do", "should", "could", "would",
            ];
            for q in &question_starters {
                if text.starts_with(q) || text.contains(&format!(" {}", q)) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_heuristic_queries() {
        assert!(DecisionClient::fallback_heuristic(&[], "zeron can you help me with this?"));
        assert!(DecisionClient::fallback_heuristic(&[], "How do I build this rust binary?"));
        assert!(DecisionClient::fallback_heuristic(&[], "What is the capital of France?"));
        assert!(DecisionClient::fallback_heuristic(&[], "please explain this error message"));
    }

    #[test]
    fn test_fallback_heuristic_casual_chatter() {
        assert!(!DecisionClient::fallback_heuristic(&[], "hey guys are we having lunch today"));
        assert!(!DecisionClient::fallback_heuristic(&[], "yeah sounds good to me"));
        assert!(!DecisionClient::fallback_heuristic(&[], "brb in 5 mins"));
        assert!(!DecisionClient::fallback_heuristic(&[], "good morning team"));
        assert!(!DecisionClient::fallback_heuristic(&[], "haha lol"));
    }
}

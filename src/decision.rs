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

        let is_clef = self.model.to_lowercase().contains("clef");

        let body = if is_clef {
            // Clef decision model native API format (state + typed questions)
            let state = format!(
                "Recent channel text messages:\n{}\nLatest message:\n{}",
                formatted_context.trim(),
                latest_message.trim()
            );

            serde_json::json!({
                "state": state,
                "questions": {
                    "should_respond": {
                        "type": "noul",
                        "instructions": "Should the AI assistant bot respond to this latest message? Answer yes if the user is asking the AI assistant for help, code, answers, or directing a task to the bot. Answer no if users are having a conversation amongst themselves, talking to each other, or do not require an AI response."
                    }
                }
            })
        } else {
            // Standard instruct / chat completion format (e.g. Llama-3.1, Gemma)
            let system_prompt = "You are a binary classification decision model for an AI coding and assistant bot in a multi-user Discord channel.\n\
Users in the channel frequently talk amongst themselves. In those cases, the bot must NOT respond.\n\
The bot should only respond if a user is asking a question to the assistant, asking for help/code/explanations, directing a task to the bot, or seeking assistant input.\n\
If people are talking to each other or having conversations that do not require an AI assistant, reply 'NO'.\n\
Respond strictly with ONLY the single word 'YES' or 'NO'.";

            let user_prompt = format!(
                "Recent channel text messages:\n{}\n\nLatest message: \"{}\"\nShould the AI bot respond? (YES/NO):",
                formatted_context.trim(),
                latest_message.trim()
            );

            serde_json::json!({
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "max_tokens": 10
            })
        };

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

        // 1. Check for Clef decision model response format: result.answers.should_respond
        if let Some(answers) = json.pointer("/result/answers/should_respond") {
            if let Some(prob) = answers.get("probability").and_then(|p| p.as_f64()) {
                debug!("Clef decision probability: {}", prob);
                return Ok(prob >= 0.5);
            }
            if let Some(choice) = answers.get("choice").and_then(|c| c.as_str()) {
                debug!("Clef decision choice: {}", choice);
                return Ok(choice.eq_ignore_ascii_case("yes"));
            }
        }

        // 2. Check for standard LLM text response format: result.response or response
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
    /// Distinguishes between multi-user conversation and messages intended for the bot.
    pub fn fallback_heuristic(
        messages_context: &[(&str, &str)],
        latest_message: &str,
    ) -> bool {
        let text = latest_message.trim();
        if text.is_empty() {
            return false;
        }

        let lower = text.to_lowercase();

        // 1. Direct address to the bot by name or alias (as words)
        let bot_names = ["zeron", "jaadu", "bot", "assistant"];
        for word in lower.split(|c: char| !c.is_alphanumeric() && c != '@') {
            if bot_names.contains(&word) {
                return true;
            }
        }

        // 2. Direct task imperatives or coding assistance requests
        let task_cues = [
            "fix this", "fix the bug", "debug this", "explain this error",
            "write a function", "write a script", "write code", "generate a",
            "refactor this", "review this code", "how do i fix", "how to solve",
            "can you code", "implement a", "create a function",
        ];
        for cue in &task_cues {
            if lower.contains(cue) {
                return true;
            }
        }

        // 3. Conversation continuation: if the immediately preceding message was from the bot,
        // an interrogative follow-up or clarifying request is intended for the bot.
        if let Some((prev_author, _)) = messages_context.last() {
            let prev_author_lower = prev_author.to_lowercase();
            let is_prev_bot = prev_author_lower.contains("jaadu")
                || prev_author_lower.contains("zeron")
                || prev_author_lower.contains("bot");

            if is_prev_bot {
                let follow_up_starters = [
                    "what about", "how about", "can you", "could you", "why does",
                    "and what", "also", "does that", "is there", "why",
                ];
                for starter in &follow_up_starters {
                    if lower.starts_with(starter) {
                        return true;
                    }
                }
            }
        }

        // Otherwise, assume multi-user casual conversation where the bot should remain silent
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_heuristic_queries() {
        assert!(DecisionClient::fallback_heuristic(&[], "zeron can you help me with this?"));
        assert!(DecisionClient::fallback_heuristic(&[], "jaadu please fix this error"));
        assert!(DecisionClient::fallback_heuristic(&[], "hey bot, how do I solve this?"));
        assert!(DecisionClient::fallback_heuristic(&[], "fix the bug in the auth service"));
        assert!(DecisionClient::fallback_heuristic(&[], "write a function to parse json"));
        assert!(DecisionClient::fallback_heuristic(&[], "explain this error: NullPointerException"));
    }

    #[test]
    fn test_fallback_heuristic_casual_chatter() {
        assert!(!DecisionClient::fallback_heuristic(&[], "hey guys are we having lunch today?"));
        assert!(!DecisionClient::fallback_heuristic(&[], "what time is the meeting?"));
        assert!(!DecisionClient::fallback_heuristic(&[], "did you push the commit?"));
        assert!(!DecisionClient::fallback_heuristic(&[], "where are you going?"));
        assert!(!DecisionClient::fallback_heuristic(&[], "how was your weekend?"));
        assert!(!DecisionClient::fallback_heuristic(&[], "yeah sounds good to me"));
        assert!(!DecisionClient::fallback_heuristic(&[], "brb in 5 mins"));
        assert!(!DecisionClient::fallback_heuristic(&[], "good morning team"));
        assert!(!DecisionClient::fallback_heuristic(&[], "haha lol"));
    }

    #[test]
    fn test_fallback_heuristic_conversation_continuation() {
        let bot_context = [
            ("Alice", "zeron how do I run tests?"),
            ("jaadu", "Use cargo test in your terminal."),
        ];

        // Follow-up question directed to bot after bot replied
        assert!(DecisionClient::fallback_heuristic(&bot_context, "and what about release mode?"));
        assert!(DecisionClient::fallback_heuristic(&bot_context, "can you give an example?"));

        // Peer chatter after bot replied
        let peer_context = [
            ("Alice", "zeron how do I run tests?"),
            ("jaadu", "Use cargo test in your terminal."),
            ("Bob", "hey Alice are you joining the call?"),
        ];
        assert!(!DecisionClient::fallback_heuristic(&peer_context, "what did you say?"));
    }
}

use reqwest::Client;
use tracing::{debug, info, warn};

const DECISION_INSTRUCTIONS: &str = "You are deciding whether an AI bot in this Discord channel should respond to the latest message. The bot participates in conversations naturally and helpfully, in normal everyday chats, discussions, casual talk, or when someone asks a question or brings up a topic suitable for the bot to chime in. Answer YES if the message is directed to the bot, asks a question, seeks thoughts/opinions, continues an active conversation with the bot, or invites helpful/friendly AI participation. Answer NO only if people are clearly having an exclusive private/personal conversation with each other where an AI response would be intrusive or unwanted.";

#[derive(Clone)]
pub struct DecisionClient {
    http: Client,
    pub account_id: Option<String>,
    pub api_token: Option<String>,
    pub refresh_token: Option<String>,
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
            refresh_token: None,
            model,
            endpoint,
        }
    }

    pub fn with_refresh_token(mut self, refresh_token: Option<String>) -> Self {
        self.refresh_token = refresh_token;
        self
    }

    pub fn is_configured(&self) -> bool {
        self.endpoint.is_some() || (self.account_id.is_some() && self.api_token.is_some())
    }

    /// Evaluate whether the assistant should intervene and reply to the latest message.
    pub async fn should_respond(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
        bot_name: Option<&str>,
    ) -> bool {
        self.should_respond_with_media(messages_context, latest_message, bot_name, None)
            .await
    }

    /// Evaluate whether the assistant should intervene and reply to the latest message,
    /// optionally considering media summary context (e.g. "2 images, 1 voice note").
    pub async fn should_respond_with_media(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
        bot_name: Option<&str>,
        media_summary: Option<&str>,
    ) -> bool {
        if self.is_configured() {
            match self
                .call_cloudflare(messages_context, latest_message, media_summary)
                .await
            {
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

        let verdict = Self::fallback_heuristic_with_media(
            messages_context,
            latest_message,
            bot_name,
            media_summary,
        );
        info!("Decision fallback heuristic verdict: {}", verdict);
        verdict
    }

    async fn call_cloudflare(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
        media_summary: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let is_clef = self.model.to_lowercase().contains("clef");
        if is_clef {
            self.evaluate_with_clef(messages_context, latest_message, media_summary)
                .await
        } else {
            self.evaluate_with_instruct(messages_context, latest_message, media_summary)
                .await
        }
    }

    /// Evaluate using Cloudflare Clef decision model native API format (state + typed questions).
    pub async fn evaluate_with_clef(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
        media_summary: Option<&str>,
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

        // Format latest message with media summary if present (e.g. "[user sent 2 images, 1 voice note]")
        let latest_part = match media_summary {
            Some(media) if !media.trim().is_empty() => {
                let m = media.trim();
                let media_tag = if m.starts_with('[') && m.ends_with(']') {
                    m.to_string()
                } else if m.to_lowercase().starts_with("user sent") {
                    format!("[{}]", m)
                } else {
                    format!("[user sent {}]", m)
                };

                let trimmed_msg = latest_message.trim();
                if trimmed_msg.is_empty() {
                    media_tag
                } else {
                    format!("{} {}", trimmed_msg, media_tag)
                }
            }
            _ => latest_message.trim().to_string(),
        };

        let state = format!(
            "Recent channel text messages:\n{}\nLatest message:\n{}",
            formatted_context.trim(),
            latest_part
        );

        let body = serde_json::json!({
            "state": state,
            "questions": {
                "should_respond": {
                    "type": "noul",
                    "instructions": DECISION_INSTRUCTIONS
                }
            }
        });

        let mut req = self.http.post(&url).json(&body);
        if let Some(ref token) = self.api_token {
            req = req.bearer_auth(token);
        }

        let mut res = req
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(ref rt) = self.refresh_token {
                warn!("Clef received 401 Unauthorized, attempting OAuth token refresh...");
                if let Some(new_token) = crate::media::ocr::refresh_cloudflare_oauth_token(rt, &self.http).await {
                    res = self.http.post(&url)
                        .bearer_auth(&new_token)
                        .json(&body)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await?;
                }
            }
        }
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            return Err(format!("Cloudflare API error status {}: {}", status, text).into());
        }

        let json: serde_json::Value = res.json().await?;

        // 1. Check for Clef decision model response format: result.answers.should_respond
        if let Some(answers) = json.pointer("/result/answers/should_respond") {
            // Clef noul field represents the probability score [0.0 - 1.0]
            if let Some(noul) = answers.get("noul").and_then(|p| p.as_f64()) {
                debug!("Clef decision noul probability: {}", noul);
                return Ok(noul >= 0.5);
            }
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

    /// Evaluate using standard instruct / chat completion format (e.g. Llama-3.1, Gemma).
    pub async fn evaluate_with_instruct(
        &self,
        messages_context: &[(&str, &str)],
        latest_message: &str,
        media_summary: Option<&str>,
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

        let latest_part = match media_summary {
            Some(media) if !media.trim().is_empty() => {
                let m = media.trim();
                let media_tag = if m.starts_with('[') && m.ends_with(']') {
                    m.to_string()
                } else if m.to_lowercase().starts_with("user sent") {
                    format!("[{}]", m)
                } else {
                    format!("[user sent {}]", m)
                };

                let trimmed_msg = latest_message.trim();
                if trimmed_msg.is_empty() {
                    media_tag
                } else {
                    format!("{} {}", trimmed_msg, media_tag)
                }
            }
            _ => latest_message.trim().to_string(),
        };

        let user_prompt = format!(
            "Recent channel text messages:\n{}\n\nLatest message: \"{}\"\nShould the AI bot respond? (YES/NO):",
            formatted_context.trim(),
            latest_part
        );

        let body = serde_json::json!({
            "messages": [
                { "role": "system", "content": DECISION_INSTRUCTIONS },
                { "role": "user", "content": user_prompt }
            ],
            "max_tokens": 10
        });

        let mut req = self.http.post(&url).json(&body);
        if let Some(ref token) = self.api_token {
            req = req.bearer_auth(token);
        }

        let mut res = req
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(ref rt) = self.refresh_token {
                warn!("Clef received 401 Unauthorized, attempting OAuth token refresh...");
                if let Some(new_token) = crate::media::ocr::refresh_cloudflare_oauth_token(rt, &self.http).await {
                    res = self.http.post(&url)
                        .bearer_auth(&new_token)
                        .json(&body)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await?;
                }
            }
        }
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
    /// Distinguishes between multi-user conversation and messages intended for the bot.
    pub fn fallback_heuristic(
        messages_context: &[(&str, &str)],
        latest_message: &str,
        bot_name: Option<&str>,
    ) -> bool {
        Self::fallback_heuristic_with_media(messages_context, latest_message, bot_name, None)
    }

    /// Smart fallback heuristic with media context support.
    /// If `media_summary` is non-empty and user text is empty or brief, biases decision to true
    /// (the user sent media into the channel).
    pub fn fallback_heuristic_with_media(
        messages_context: &[(&str, &str)],
        latest_message: &str,
        bot_name: Option<&str>,
        media_summary: Option<&str>,
    ) -> bool {
        let text = latest_message.trim();

        // 0. Media context bias: if media was uploaded and text is empty or brief,
        // bias decision to true (user sent media to the channel for the bot).
        if let Some(media) = media_summary {
            let media_trim = media.trim();
            if !media_trim.is_empty() {
                let word_count = text.split_whitespace().count();
                if text.is_empty() || text.len() <= 60 || word_count <= 8 {
                    return true;
                }
            }
        }

        if text.is_empty() {
            return false;
        }

        let lower = text.to_lowercase();
        let dynamic_bot_name = bot_name.map(|n| n.to_lowercase());

        // 1. Direct address to the bot by name or alias (as words)
        let generic_bot_names = ["zeron", "bot", "assistant"];
        for word in lower.split(|c: char| !c.is_alphanumeric() && c != '@') {
            if generic_bot_names.contains(&word) {
                return true;
            }
            if let Some(ref name) = dynamic_bot_name {
                if !name.is_empty() && (word == name || lower.contains(name)) {
                    return true;
                }
            }
        }

        // 2. Direct task imperatives or coding assistance requests
        let task_cues = [
            "fix this",
            "fix the bug",
            "debug this",
            "explain this error",
            "write a function",
            "write a script",
            "write code",
            "generate a",
            "refactor this",
            "review this code",
            "how do i fix",
            "how to solve",
            "can you code",
            "implement a",
            "create a function",
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
            let is_prev_bot = prev_author_lower.contains("zeron")
                || prev_author_lower.contains("bot")
                || dynamic_bot_name
                    .as_deref()
                    .is_some_and(|name| !name.is_empty() && prev_author_lower.contains(name));

            if is_prev_bot {
                let follow_up_starters = [
                    "what about",
                    "how about",
                    "can you",
                    "could you",
                    "why does",
                    "and what",
                    "also",
                    "does that",
                    "is there",
                    "why",
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
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "zeron can you help me with this?",
            None
        ));
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "mybot please fix this error",
            Some("mybot")
        ));
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "hey bot, how do I solve this?",
            None
        ));
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "fix the bug in the auth service",
            None
        ));
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "write a function to parse json",
            None
        ));
        assert!(DecisionClient::fallback_heuristic(
            &[],
            "explain this error: NullPointerException",
            None
        ));
    }

    #[test]
    fn test_fallback_heuristic_casual_chatter() {
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "hey guys are we having lunch today?",
            Some("mybot")
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "what time is the meeting?",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "did you push the commit?",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "where are you going?",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "how was your weekend?",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "yeah sounds good to me",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "brb in 5 mins",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(
            &[],
            "good morning team",
            None
        ));
        assert!(!DecisionClient::fallback_heuristic(&[], "haha lol", None));
    }

    #[test]
    fn test_fallback_heuristic_conversation_continuation() {
        let bot_context = [
            ("Alice", "zeron how do I run tests?"),
            ("custombot", "Use cargo test in your terminal."),
        ];

        // Follow-up question directed to bot after bot replied
        assert!(DecisionClient::fallback_heuristic(
            &bot_context,
            "and what about release mode?",
            Some("custombot")
        ));
        assert!(DecisionClient::fallback_heuristic(
            &bot_context,
            "can you give an example?",
            Some("custombot")
        ));

        // Peer chatter after bot replied
        let peer_context = [
            ("Alice", "zeron how do I run tests?"),
            ("custombot", "Use cargo test in your terminal."),
            ("Bob", "hey Alice are you joining the call?"),
        ];
        assert!(!DecisionClient::fallback_heuristic(
            &peer_context,
            "what did you say?",
            Some("custombot")
        ));
    }

    #[test]
    fn test_fallback_heuristic_with_media() {
        // Empty text with media -> biased to true
        assert!(DecisionClient::fallback_heuristic_with_media(
            &[],
            "",
            None,
            Some("1 image")
        ));
        assert!(DecisionClient::fallback_heuristic_with_media(
            &[],
            "   ",
            None,
            Some("2 images, 1 voice note")
        ));

        // Brief text with media -> biased to true
        assert!(DecisionClient::fallback_heuristic_with_media(
            &[],
            "what is this?",
            None,
            Some("1 image")
        ));
        assert!(DecisionClient::fallback_heuristic_with_media(
            &[],
            "check this out",
            None,
            Some("1 image")
        ));

        // Empty text with NO media -> false
        assert!(!DecisionClient::fallback_heuristic_with_media(
            &[],
            "",
            None,
            None
        ));
        assert!(!DecisionClient::fallback_heuristic_with_media(
            &[],
            "",
            None,
            Some("")
        ));

        // Casual long chatter with NO media -> false
        assert!(!DecisionClient::fallback_heuristic_with_media(
            &[],
            "hey guys are we having lunch together at the cafeteria today?",
            None,
            None
        ));
    }
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Zeron MCP error: {0}")]
    ZeronMcp(String),

    #[error("Zeron process failed: {0}")]
    ZeronProcess(String),

    #[error("Discord Serenity error: {0}")]
    Discord(Box<serenity::Error>),

    #[error("Session error: {0}")]
    Session(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Timeout waiting for Zeron response")]
    Timeout,

    #[error("Other error: {0}")]
    Other(String),
}

impl From<serenity::Error> for BotError {
    fn from(err: serenity::Error) -> Self {
        BotError::Discord(Box::new(err))
    }
}

pub type BotResult<T> = Result<T, BotError>;

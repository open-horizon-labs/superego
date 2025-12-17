use serde::{Deserialize, Serialize};

/// A single entry in the transcript JSONL file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TranscriptEntry {
    /// Conversation summary
    Summary {
        summary: String,
        #[serde(rename = "leafUuid")]
        leaf_uuid: Option<String>,
    },
    /// File history snapshot
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot {
        #[serde(rename = "messageId")]
        message_id: Option<String>,
    },
    /// User message
    User {
        uuid: String,
        #[serde(rename = "parentUuid")]
        parent_uuid: Option<String>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        timestamp: Option<String>,
        message: UserMessage,
    },
    /// Assistant message
    Assistant {
        uuid: String,
        #[serde(rename = "parentUuid")]
        parent_uuid: Option<String>,
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        timestamp: Option<String>,
        message: AssistantMessage,
    },
    /// Catch-all for unknown types
    #[serde(other)]
    Unknown,
}

/// User message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub role: String,
    pub content: UserContent,
}

/// User content can be string or array of blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<UserContentBlock>),
}

/// User content block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    // Tool result fields
    #[serde(rename = "tool_use_id")]
    pub tool_use_id: Option<String>,
    /// Tool result content can be string or structured
    pub content: Option<serde_json::Value>,
}

/// Assistant message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<AssistantContentBlock>,
    pub model: Option<String>,
}

/// Assistant content block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub thinking: Option<String>,
    // Tool use fields
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

impl TranscriptEntry {
    /// Get the session ID if available
    pub fn session_id(&self) -> Option<&str> {
        match self {
            TranscriptEntry::User { session_id, .. } => session_id.as_deref(),
            TranscriptEntry::Assistant { session_id, .. } => session_id.as_deref(),
            _ => None,
        }
    }

    /// Get the timestamp if available
    pub fn timestamp(&self) -> Option<&str> {
        match self {
            TranscriptEntry::User { timestamp, .. } => timestamp.as_deref(),
            TranscriptEntry::Assistant { timestamp, .. } => timestamp.as_deref(),
            _ => None,
        }
    }

    /// Check if this is a user message
    pub fn is_user(&self) -> bool {
        matches!(self, TranscriptEntry::User { .. })
    }

    /// Check if this is an assistant message
    pub fn is_assistant(&self) -> bool {
        matches!(self, TranscriptEntry::Assistant { .. })
    }

    /// Check if this is a conversation message (user or assistant)
    pub fn is_message(&self) -> bool {
        self.is_user() || self.is_assistant()
    }

    /// Check if this is a summary
    pub fn is_summary(&self) -> bool {
        matches!(self, TranscriptEntry::Summary { .. })
    }

    /// Extract summary text
    pub fn summary_text(&self) -> Option<&str> {
        match self {
            TranscriptEntry::Summary { summary, .. } => Some(summary.as_str()),
            _ => None,
        }
    }

    /// Extract thinking content from assistant message
    pub fn assistant_thinking(&self) -> Option<String> {
        match self {
            TranscriptEntry::Assistant { message, .. } => {
                let thoughts: Vec<&str> = message
                    .content
                    .iter()
                    .filter(|b| b.block_type == "thinking")
                    .filter_map(|b| b.thinking.as_deref())
                    .collect();
                if thoughts.is_empty() {
                    None
                } else {
                    Some(thoughts.join("\n"))
                }
            }
            _ => None,
        }
    }

    /// Extract text content from user message
    pub fn user_text(&self) -> Option<String> {
        match self {
            TranscriptEntry::User { message, .. } => match &message.content {
                UserContent::Text(text) => Some(text.clone()),
                UserContent::Blocks(blocks) => {
                    let texts: Vec<&str> = blocks
                        .iter()
                        .filter(|b| b.block_type == "text")
                        .filter_map(|b| b.text.as_deref())
                        .collect();
                    if texts.is_empty() {
                        None
                    } else {
                        Some(texts.join("\n"))
                    }
                }
            },
            _ => None,
        }
    }

    /// Extract tool results from user message (tool outputs returned to Claude)
    pub fn tool_results(&self) -> Vec<(Option<&str>, String)> {
        match self {
            TranscriptEntry::User { message, .. } => match &message.content {
                UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter(|b| b.block_type == "tool_result")
                    .filter_map(|b| {
                        let content = b.content.as_ref().map(|c| match c {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })?;
                        Some((b.tool_use_id.as_deref(), content))
                    })
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Extract text content from assistant message (excludes thinking)
    pub fn assistant_text(&self) -> Option<String> {
        match self {
            TranscriptEntry::Assistant { message, .. } => {
                let texts: Vec<&str> = message
                    .content
                    .iter()
                    .filter(|b| b.block_type == "text")
                    .filter_map(|b| b.text.as_deref())
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
            _ => None,
        }
    }

    /// Extract tool uses from assistant message as (name, input_json)
    pub fn tool_uses(&self) -> Vec<(&str, Option<&serde_json::Value>)> {
        match self {
            TranscriptEntry::Assistant { message, .. } => message
                .content
                .iter()
                .filter(|b| b.block_type == "tool_use")
                .filter_map(|b| Some((b.name.as_deref()?, b.input.as_ref())))
                .collect(),
            _ => Vec::new(),
        }
    }
}

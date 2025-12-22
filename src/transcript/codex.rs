//! Codex transcript parser
//!
//! Parses OpenAI Codex CLI session files (JSONL format).
//! Session files are stored in ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
//!
//! Format validated against actual session files (codex-cli 0.77.0)

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::reader::TranscriptError;

/// Top-level entry in a Codex session JSONL file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexEntry {
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl CodexEntry {
    /// Check if this is a user message (response_item with role=user)
    pub fn is_user_message(&self) -> bool {
        if self.entry_type == "response_item" {
            if let Some(role) = self.payload.get("role") {
                return role == "user";
            }
            if let Some(ptype) = self.payload.get("type") {
                if ptype == "message" {
                    if let Some(role) = self.payload.get("role") {
                        return role == "user";
                    }
                }
            }
        }
        // Also check event_msg with user_message type
        if self.entry_type == "event_msg" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "user_message";
            }
        }
        false
    }

    /// Check if this is agent reasoning
    pub fn is_reasoning(&self) -> bool {
        if self.entry_type == "event_msg" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "agent_reasoning";
            }
        }
        if self.entry_type == "response_item" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "reasoning";
            }
        }
        false
    }

    /// Check if this is a function/tool call
    pub fn is_function_call(&self) -> bool {
        if self.entry_type == "response_item" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "function_call";
            }
        }
        false
    }

    /// Check if this is a function/tool output
    pub fn is_function_output(&self) -> bool {
        if self.entry_type == "response_item" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "function_call_output";
            }
        }
        false
    }

    /// Check if this is an agent message (text response)
    pub fn is_agent_message(&self) -> bool {
        if self.entry_type == "response_item" {
            if let Some(ptype) = self.payload.get("type") {
                return ptype == "message"
                    && self
                        .payload
                        .get("role")
                        .map(|r| r == "assistant")
                        .unwrap_or(false);
            }
        }
        false
    }

    /// Extract agent message text
    pub fn agent_text(&self) -> Option<String> {
        if !self.is_agent_message() {
            return None;
        }
        // Agent messages have content array with text blocks
        if let Some(content) = self.payload.get("content") {
            if let Some(arr) = content.as_array() {
                let texts: Vec<&str> = arr
                    .iter()
                    .filter_map(|block| {
                        let btype = block.get("type")?.as_str()?;
                        if btype == "output_text" || btype == "text" {
                            block.get("text")?.as_str()
                        } else {
                            None
                        }
                    })
                    .collect();
                if !texts.is_empty() {
                    return Some(texts.join("\n"));
                }
            }
        }
        None
    }

    /// Extract user message text
    pub fn user_text(&self) -> Option<String> {
        // event_msg with user_message type
        if self.entry_type == "event_msg" {
            if let Some(msg) = self.payload.get("message") {
                return msg.as_str().map(|s| s.to_string());
            }
        }
        // response_item with content array
        if self.entry_type == "response_item" {
            if let Some(content) = self.payload.get("content") {
                if let Some(arr) = content.as_array() {
                    let texts: Vec<&str> = arr
                        .iter()
                        .filter_map(|block| {
                            let btype = block.get("type")?.as_str()?;
                            if btype == "input_text" || btype == "text" {
                                block.get("text")?.as_str()
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !texts.is_empty() {
                        return Some(texts.join("\n"));
                    }
                }
            }
        }
        None
    }

    /// Extract reasoning text
    pub fn reasoning_text(&self) -> Option<String> {
        if self.entry_type == "event_msg" {
            if let Some(text) = self.payload.get("text") {
                return text.as_str().map(|s| s.to_string());
            }
        }
        if self.entry_type == "response_item" {
            // reasoning has summary array
            if let Some(summary) = self.payload.get("summary") {
                if let Some(arr) = summary.as_array() {
                    let texts: Vec<&str> = arr
                        .iter()
                        .filter_map(|item| item.get("text")?.as_str())
                        .collect();
                    if !texts.is_empty() {
                        return Some(texts.join("\n"));
                    }
                }
            }
        }
        None
    }

    /// Extract function call info (name, arguments)
    pub fn function_call(&self) -> Option<(String, String)> {
        if !self.is_function_call() {
            return None;
        }
        let name = self.payload.get("name")?.as_str()?.to_string();
        let args = self
            .payload
            .get("arguments")
            .map(|a| a.as_str().unwrap_or("").to_string())
            .unwrap_or_default();
        Some((name, args))
    }

    /// Extract function output
    pub fn function_output(&self) -> Option<String> {
        if !self.is_function_output() {
            return None;
        }
        self.payload.get("output").map(|o| {
            // Output can be string or JSON
            if let Some(s) = o.as_str() {
                s.to_string()
            } else {
                o.to_string()
            }
        })
    }
}

/// Read and parse a Codex session JSONL file
pub fn read_codex_transcript(path: &Path) -> Result<Vec<CodexEntry>, TranscriptError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<CodexEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!(
                    "Warning: skipping malformed Codex line {} in transcript: {}",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(entries)
}

/// Format Codex entries for evaluation context
pub fn format_codex_context(entries: &[CodexEntry]) -> String {
    let mut output = String::new();
    let mut seen_user_msg: Option<String> = None;

    for entry in entries {
        // User message (prefer event_msg version to avoid duplicates)
        if entry.entry_type == "event_msg" && entry.is_user_message() {
            if let Some(text) = entry.user_text() {
                // Skip duplicate if same as recent response_item
                if seen_user_msg.as_ref() != Some(&text) {
                    output.push_str("USER: ");
                    // Truncate very long messages
                    let truncated = if text.len() > 2000 {
                        format!("{}... [truncated]", &text[..2000])
                    } else {
                        text.clone()
                    };
                    output.push_str(&truncated);
                    output.push_str("\n\n");
                }
            }
        } else if entry.entry_type == "response_item" && entry.is_user_message() {
            if let Some(text) = entry.user_text() {
                seen_user_msg = Some(text); // Track for dedup
            }
        }

        // Reasoning
        if entry.is_reasoning() {
            if let Some(text) = entry.reasoning_text() {
                output.push_str("THINKING: ");
                output.push_str(&text);
                output.push_str("\n\n");
            }
        }

        // Function calls
        if let Some((name, args)) = entry.function_call() {
            output.push_str("TOOL: ");
            output.push_str(&name);
            // Parse args to extract command if shell
            if name == "shell" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&args) {
                    if let Some(cmd) = parsed.get("command") {
                        output.push(' ');
                        output.push_str(&cmd.to_string());
                    }
                }
            }
            output.push('\n');
        }

        // Function outputs (truncated)
        if let Some(out) = entry.function_output() {
            let truncated = if out.len() > 500 {
                format!("{}... [truncated]", &out[..500])
            } else {
                out
            };
            output.push_str("OUTPUT: ");
            output.push_str(&truncated);
            output.push_str("\n\n");
        }

        // Agent text responses
        if let Some(text) = entry.agent_text() {
            output.push_str("ASSISTANT: ");
            let truncated = if text.len() > 2000 {
                format!("{}... [truncated]", &text[..2000])
            } else {
                text
            };
            output.push_str(&truncated);
            output.push_str("\n\n");
        }
    }

    output
}

/// Detect if a file is a Codex transcript (vs Claude Code)
pub fn is_codex_format(path: &Path) -> bool {
    // Check by path pattern first
    let path_str = path.to_string_lossy();
    if path_str.contains(".codex/sessions/") || path_str.contains("rollout-") {
        return true;
    }

    // Check by content
    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        for line in reader.lines().take(5).flatten() {
            // Codex-specific markers
            if line.contains("\"session_meta\"")
                || line.contains("\"response_item\"")
                || line.contains("\"event_msg\"")
                || line.contains("\"turn_context\"")
            {
                return true;
            }
            // Claude Code markers
            if line.contains("\"parentUuid\"") || line.contains("\"sessionId\"") {
                return false;
            }
        }
    }

    false
}

/// Check if a session file is user-initiated (not a sub-agent codex_exec session)
fn is_user_initiated_session(path: &Path) -> bool {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);

    // Check first few lines for session_meta with originator
    for line in reader.lines().take(5).flatten() {
        if let Ok(entry) = serde_json::from_str::<CodexEntry>(&line) {
            if entry.entry_type == "session_meta" {
                // Check originator field - "codex_exec" means sub-agent, skip it
                if let Some(originator) = entry.payload.get("originator") {
                    if originator == "codex_exec" {
                        return false;
                    }
                }
                // Found session_meta without codex_exec originator, it's user-initiated
                return true;
            }
        }
    }
    // No session_meta found, assume it's ok
    true
}

/// Find the most recent user-initiated Codex session file
/// Filters out sub-agent sessions (originator: "codex_exec")
pub fn find_latest_codex_session() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let sessions_dir = Path::new(&home).join(".codex/sessions");

    if !sessions_dir.exists() {
        return None;
    }

    // Find all .jsonl files and get the most recent USER-INITIATED session
    let mut latest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;

    fn visit_dir(dir: &Path, latest: &mut Option<(std::time::SystemTime, std::path::PathBuf)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, latest);
                } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    // Skip sub-agent sessions (codex_exec)
                    if !is_user_initiated_session(&path) {
                        continue;
                    }
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            match latest {
                                Some((ref t, _)) if modified > *t => {
                                    *latest = Some((modified, path));
                                }
                                None => {
                                    *latest = Some((modified, path));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    visit_dir(&sessions_dir, &mut latest);
    latest.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_meta() {
        let json = r#"{"timestamp":"2025-11-04T00:16:00.093Z","type":"session_meta","payload":{"id":"test-id","cwd":"/test"}}"#;
        let entry: CodexEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "session_meta");
    }

    #[test]
    fn test_parse_user_message_event() {
        let json = r#"{"timestamp":"2025-11-04T00:16:00.102Z","type":"event_msg","payload":{"type":"user_message","message":"Hello, help me debug this","images":[]}}"#;
        let entry: CodexEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_user_message());
        assert_eq!(
            entry.user_text(),
            Some("Hello, help me debug this".to_string())
        );
    }

    #[test]
    fn test_parse_agent_reasoning() {
        let json = r#"{"timestamp":"2025-11-04T00:16:08.855Z","type":"event_msg","payload":{"type":"agent_reasoning","text":"**Investigating the issue**"}}"#;
        let entry: CodexEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_reasoning());
        assert_eq!(
            entry.reasoning_text(),
            Some("**Investigating the issue**".to_string())
        );
    }

    #[test]
    fn test_parse_function_call() {
        let json = r#"{"timestamp":"2025-11-04T00:16:11.856Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"zsh\",\"-lc\",\"ls\"]}","call_id":"call_123"}}"#;
        let entry: CodexEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_function_call());
        let (name, _args) = entry.function_call().unwrap();
        assert_eq!(name, "shell");
    }

    #[test]
    fn test_parse_function_output() {
        let json = r#"{"timestamp":"2025-11-04T00:16:11.856Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_123","output":"file1.txt\nfile2.txt"}}"#;
        let entry: CodexEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_function_output());
        assert_eq!(
            entry.function_output(),
            Some("file1.txt\nfile2.txt".to_string())
        );
    }
}

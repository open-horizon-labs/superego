use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::transcript::types::TranscriptEntry;

/// Error type for transcript reading
#[derive(Debug)]
pub enum TranscriptError {
    IoError(std::io::Error),
    ParseError { line: usize, error: String },
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptError::IoError(e) => write!(f, "IO error: {}", e),
            TranscriptError::ParseError { line, error } => {
                write!(f, "Parse error at line {}: {}", line, error)
            }
        }
    }
}

impl std::error::Error for TranscriptError {}

impl From<std::io::Error> for TranscriptError {
    fn from(e: std::io::Error) -> Self {
        TranscriptError::IoError(e)
    }
}

/// Read and parse a transcript JSONL file
///
/// Skips malformed lines rather than failing entirely
pub fn read_transcript(path: &Path) -> Result<Vec<TranscriptEntry>, TranscriptError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<TranscriptEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                // Log warning but continue - don't fail on malformed lines
                eprintln!(
                    "Warning: skipping malformed line {} in transcript: {}",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(entries)
}

/// Get recent conversation messages (user and assistant only)
pub fn get_recent_messages(entries: &[TranscriptEntry], limit: usize) -> Vec<&TranscriptEntry> {
    entries
        .iter()
        .filter(|e| e.is_message())
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Extract session ID from transcript entries
pub fn extract_session_id(entries: &[TranscriptEntry]) -> Option<String> {
    entries
        .iter()
        .find_map(|e| e.session_id().map(|s| s.to_string()))
}

/// Extract only user messages
pub fn get_user_messages(entries: &[TranscriptEntry]) -> Vec<&TranscriptEntry> {
    entries.iter().filter(|e| e.is_user()).collect()
}

/// Extract only assistant messages
pub fn get_assistant_messages(entries: &[TranscriptEntry]) -> Vec<&TranscriptEntry> {
    entries.iter().filter(|e| e.is_assistant()).collect()
}

/// Get the most recent user message
pub fn get_last_user_message(entries: &[TranscriptEntry]) -> Option<&TranscriptEntry> {
    entries.iter().rev().find(|e| e.is_user())
}

/// Get the most recent assistant message
pub fn get_last_assistant_message(entries: &[TranscriptEntry]) -> Option<&TranscriptEntry> {
    entries.iter().rev().find(|e| e.is_assistant())
}

/// Format recent messages for context (for sending to superego LLM)
pub fn format_recent_context(entries: &[TranscriptEntry], limit: usize) -> String {
    let recent = get_recent_messages(entries, limit);
    let mut output = String::new();

    for entry in recent {
        match entry {
            TranscriptEntry::User { .. } => {
                if let Some(text) = entry.user_text() {
                    output.push_str("USER: ");
                    output.push_str(&text);
                    output.push_str("\n\n");
                }
            }
            TranscriptEntry::Assistant { .. } => {
                if let Some(text) = entry.assistant_text() {
                    output.push_str("ASSISTANT: ");
                    // Truncate long assistant responses
                    if text.len() > 500 {
                        output.push_str(&text[..500]);
                        output.push_str("...[truncated]");
                    } else {
                        output.push_str(&text);
                    }
                    output.push_str("\n\n");
                }
            }
            _ => {}
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_entry() {
        let json = r#"{"type":"user","uuid":"abc","parentUuid":null,"sessionId":"sess-1","timestamp":"2025-01-15T10:00:00Z","message":{"role":"user","content":"hello"}}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_user());
        assert_eq!(entry.session_id(), Some("sess-1"));
        assert_eq!(entry.user_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_parse_assistant_entry() {
        let json = r#"{"type":"assistant","uuid":"def","parentUuid":"abc","sessionId":"sess-1","timestamp":"2025-01-15T10:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"hi there"}]}}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_assistant());
        assert_eq!(entry.assistant_text(), Some("hi there".to_string()));
    }

    #[test]
    fn test_parse_unknown_type() {
        let json = r#"{"type":"some-new-type","data":"whatever"}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, TranscriptEntry::Unknown));
    }
}

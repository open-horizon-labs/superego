use chrono::{DateTime, Utc};
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::transcript::types::TranscriptEntry;

/// Error type for transcript reading
#[derive(Debug)]
pub enum TranscriptError {
    IoError(std::io::Error),
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptError::IoError(e) => write!(f, "IO error: {}", e),
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

/// Get messages since a given timestamp, optionally filtered by session
/// AIDEV-NOTE: This is the primary context selection method. We evaluate
/// everything new since the last evaluation, not an arbitrary window.
/// When session_id is provided, only messages from that session are included
/// to prevent cross-session context bleed.
pub fn get_messages_since<'a>(
    entries: &'a [TranscriptEntry],
    since: Option<DateTime<Utc>>,
    session_id: Option<&str>,
) -> Vec<&'a TranscriptEntry> {
    let session_filter = |e: &&TranscriptEntry| -> bool {
        match session_id {
            Some(sid) => e.session_id() == Some(sid),
            None => true, // No session filter - include all (backward compat)
        }
    };

    // Include messages AND summaries (summaries provide context after compaction)
    let content_filter = |e: &&TranscriptEntry| e.is_message() || e.is_summary();

    match since {
        Some(cutoff) => {
            entries
                .iter()
                .filter(content_filter)
                .filter(session_filter)
                .filter(|e| {
                    // Include if timestamp is after cutoff (or if no timestamp)
                    // Summaries don't have timestamps, so they pass through
                    e.timestamp()
                        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                        .map(|ts| ts > cutoff)
                        .unwrap_or(true)
                })
                .collect()
        }
        None => {
            // No previous evaluation - include all messages + summaries (for this session)
            entries.iter().filter(content_filter).filter(session_filter).collect()
        }
    }
}

/// Strip <system-reminder>...</system-reminder> blocks from text
/// AIDEV-NOTE: System reminders are injected by Claude Code for workflow/context.
/// Superego should evaluate technical decisions, not enforce workflow instructions.
fn strip_system_reminders(text: &str) -> String {
    let re = Regex::new(r"(?s)<system-reminder>.*?</system-reminder>").unwrap();
    re.replace_all(text, "").trim().to_string()
}

/// Extract key identifier from tool input (file path, command, pattern)
fn tool_summary(name: &str, input: Option<&serde_json::Value>) -> String {
    let input = match input {
        Some(v) => v,
        None => return String::new(),
    };
    match name {
        "Edit" | "Write" | "Read" => {
            input.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        "Bash" => {
            input.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        "Glob" | "Grep" => {
            input.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        _ => String::new(),
    }
}

/// Format messages for context (for sending to superego LLM)
pub fn format_context(messages: &[&TranscriptEntry]) -> String {
    let mut output = String::new();

    for entry in messages {
        match entry {
            TranscriptEntry::Summary { .. } => {
                if let Some(text) = entry.summary_text() {
                    output.push_str("SUMMARY: ");
                    output.push_str(text);
                    output.push_str("\n\n");
                }
            }
            TranscriptEntry::User { .. } => {
                // Include tool results (what Claude read/executed)
                let tool_results = entry.tool_results();
                if !tool_results.is_empty() {
                    for (_id, content) in &tool_results {
                        // Truncate large tool results to avoid token bloat
                        let truncated = if content.len() > 500 {
                            format!("{}...[truncated]", &content[..500])
                        } else {
                            content.clone()
                        };
                        output.push_str("TOOL_RESULT: ");
                        output.push_str(&truncated);
                        output.push_str("\n\n");
                    }
                }

                if let Some(text) = entry.user_text() {
                    let cleaned = strip_system_reminders(&text);
                    if !cleaned.is_empty() {
                        output.push_str("USER: ");
                        output.push_str(&cleaned);
                        output.push_str("\n\n");
                    }
                }
            }
            TranscriptEntry::Assistant { .. } => {
                let tool_uses = entry.tool_uses();

                // Include thinking if present (shows Claude's reasoning)
                if let Some(thinking) = entry.assistant_thinking() {
                    output.push_str("THINKING: ");
                    output.push_str(&thinking);
                    output.push_str("\n\n");
                }

                if !tool_uses.is_empty() {
                    output.push_str("TOOLS: ");
                    for (name, input) in &tool_uses {
                        output.push_str(name);
                        let summary = tool_summary(name, *input);
                        if !summary.is_empty() {
                            output.push_str("(");
                            output.push_str(&summary);
                            output.push_str(")");
                        }
                        output.push_str(" ");
                    }
                    output.push_str("\n");
                }

                if let Some(text) = entry.assistant_text() {
                    output.push_str("ASSISTANT: ");
                    output.push_str(&text);
                    output.push_str("\n\n");
                } else if !tool_uses.is_empty() {
                    output.push_str("\n");
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

    #[test]
    fn test_strip_system_reminders_single() {
        let text = "Hello <system-reminder>workflow stuff</system-reminder> world";
        assert_eq!(strip_system_reminders(text), "Hello  world");
    }

    #[test]
    fn test_strip_system_reminders_multiple() {
        let text = "<system-reminder>first</system-reminder>content<system-reminder>second</system-reminder>";
        assert_eq!(strip_system_reminders(text), "content");
    }

    #[test]
    fn test_strip_system_reminders_multiline() {
        let text = "Question here\n<system-reminder>\nMultiple\nlines\n</system-reminder>\nMore text";
        assert_eq!(strip_system_reminders(text), "Question here\n\nMore text");
    }

    #[test]
    fn test_strip_system_reminders_none() {
        let text = "Just normal text";
        assert_eq!(strip_system_reminders(text), "Just normal text");
    }

    #[test]
    fn test_strip_system_reminders_only_reminders() {
        let text = "<system-reminder>only this</system-reminder>";
        assert_eq!(strip_system_reminders(text), "");
    }
}

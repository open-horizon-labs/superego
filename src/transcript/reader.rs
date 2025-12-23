use chrono::{DateTime, Utc};
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
            entries
                .iter()
                .filter(content_filter)
                .filter(session_filter)
                .collect()
        }
    }
}

/// Keep only the last <system-reminder>...</system-reminder> block, strip others
/// AIDEV-NOTE: System reminders are injected by Claude Code for workflow/context.
/// We keep the last one as a signal to superego that guidance exists, but dedupe
/// to avoid context bloat from repeated reminders.
fn dedupe_system_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";

    // Find all reminder blocks
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut search_start = 0;

    while let Some(open_offset) = text[search_start..].find(OPEN) {
        let open_pos = search_start + open_offset;
        let after_open = open_pos + OPEN.len();
        if let Some(close_offset) = text[after_open..].find(CLOSE) {
            let close_end = after_open + close_offset + CLOSE.len();
            blocks.push((open_pos, close_end));
            search_start = close_end;
        } else {
            break;
        }
    }

    if blocks.len() <= 1 {
        // Zero or one reminder - nothing to dedupe
        return text.trim().to_string();
    }

    // Keep last block, remove all others
    blocks.pop(); // Remove last from removal list (it stays in output)
    let mut result = String::with_capacity(text.len());
    let mut prev_end = 0;

    for (start, end) in blocks {
        result.push_str(&text[prev_end..start]);
        prev_end = end;
    }
    result.push_str(&text[prev_end..]);
    result.trim().to_string()
}

/// Extract key identifier from tool input (file path, command, pattern)
fn tool_summary(name: &str, input: Option<&serde_json::Value>) -> String {
    let input = match input {
        Some(v) => v,
        None => return String::new(),
    };
    match name {
        "Edit" | "Write" | "Read" => input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Glob" | "Grep" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
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
                        output.push_str("TOOL_RESULT: ");
                        output.push_str(content);
                        output.push_str("\n\n");
                    }
                }

                if let Some(text) = entry.user_text() {
                    let cleaned = dedupe_system_reminders(&text);
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
                            output.push('(');
                            output.push_str(&summary);
                            output.push(')');
                        }
                        output.push(' ');
                    }
                    output.push('\n');
                }

                if let Some(text) = entry.assistant_text() {
                    output.push_str("ASSISTANT: ");
                    output.push_str(&text);
                    output.push_str("\n\n");
                } else if !tool_uses.is_empty() {
                    output.push('\n');
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
    fn test_dedupe_system_reminders_single() {
        // Single reminder is kept
        let text = "Hello <system-reminder>workflow stuff</system-reminder> world";
        assert_eq!(
            dedupe_system_reminders(text),
            "Hello <system-reminder>workflow stuff</system-reminder> world"
        );
    }

    #[test]
    fn test_dedupe_system_reminders_multiple() {
        // Multiple reminders: keep last, strip others
        let text = "<system-reminder>first</system-reminder>content<system-reminder>second</system-reminder>";
        assert_eq!(
            dedupe_system_reminders(text),
            "content<system-reminder>second</system-reminder>"
        );
    }

    #[test]
    fn test_dedupe_system_reminders_multiline() {
        // Single multiline reminder is kept
        let text =
            "Question here\n<system-reminder>\nMultiple\nlines\n</system-reminder>\nMore text";
        assert_eq!(
            dedupe_system_reminders(text),
            "Question here\n<system-reminder>\nMultiple\nlines\n</system-reminder>\nMore text"
        );
    }

    #[test]
    fn test_dedupe_system_reminders_none() {
        let text = "Just normal text";
        assert_eq!(dedupe_system_reminders(text), "Just normal text");
    }

    #[test]
    fn test_dedupe_system_reminders_only_reminders() {
        // Single reminder is kept (even if it's the only content)
        let text = "<system-reminder>only this</system-reminder>";
        assert_eq!(
            dedupe_system_reminders(text),
            "<system-reminder>only this</system-reminder>"
        );
    }

    #[test]
    fn test_dedupe_system_reminders_three() {
        // Three reminders: keep only the last
        let text = "<system-reminder>1</system-reminder>A<system-reminder>2</system-reminder>B<system-reminder>3</system-reminder>";
        assert_eq!(
            dedupe_system_reminders(text),
            "AB<system-reminder>3</system-reminder>"
        );
    }

    #[test]
    fn test_get_messages_since_race_condition_scenario() {
        // AIDEV-NOTE: This tests the race condition fix scenario.
        //
        // Timeline:
        //   T1 (10:00:00) - Message A written
        //   T2 (10:00:05) - transcript_read_at captured (evaluation starts)
        //   T3 (10:00:10) - Message B written (during LLM eval)
        //   T4 (10:00:35) - LLM eval finishes
        //
        // OLD BUG: last_evaluated = T4 → Message B skipped forever!
        // FIX: last_evaluated = T2 → Message B included in next eval
        use chrono::TimeZone;

        let msg_a = r#"{"type":"user","uuid":"a","sessionId":"s1","timestamp":"2025-01-15T10:00:00Z","message":{"role":"user","content":"Message A"}}"#;
        let msg_b = r#"{"type":"user","uuid":"b","sessionId":"s1","timestamp":"2025-01-15T10:00:10Z","message":{"role":"user","content":"Message B (during eval)"}}"#;

        let entries: Vec<TranscriptEntry> = vec![
            serde_json::from_str(msg_a).unwrap(),
            serde_json::from_str(msg_b).unwrap(),
        ];

        // Simulate: transcript_read_at was captured at 10:00:05
        let transcript_read_at = chrono::Utc.with_ymd_and_hms(2025, 1, 15, 10, 0, 5).unwrap();

        // First eval: cutoff is None (first run), should get both messages
        let first_eval = get_messages_since(&entries, None, Some("s1"));
        assert_eq!(first_eval.len(), 2, "First eval should get all messages");

        // Second eval: cutoff is transcript_read_at (10:00:05)
        // Should include Message B (10:00:10 > 10:00:05) but not A
        let second_eval = get_messages_since(&entries, Some(transcript_read_at), Some("s1"));
        assert_eq!(
            second_eval.len(),
            1,
            "Second eval should only get Message B (written after cutoff)"
        );
        assert_eq!(
            second_eval[0].user_text(),
            Some("Message B (during eval)".to_string())
        );

        // Simulate the OLD bug: cutoff is completion time (10:00:35)
        // This would SKIP Message B - demonstrating the bug
        let buggy_cutoff = chrono::Utc
            .with_ymd_and_hms(2025, 1, 15, 10, 0, 35)
            .unwrap();
        let buggy_eval = get_messages_since(&entries, Some(buggy_cutoff), Some("s1"));
        assert_eq!(
            buggy_eval.len(),
            0,
            "Bug scenario: using completion time would skip Message B"
        );
    }
}

/// Evaluation for superego
///
/// LLM-based evaluation with natural language feedback.

use std::fs;
use std::path::Path;

use crate::bd;
use crate::claude::{self, ClaudeOptions};
use crate::decision::{Decision, Journal};
use crate::feedback::{Feedback, FeedbackQueue};
use crate::state::StateManager;
use crate::transcript;

/// Error type for evaluation
#[derive(Debug)]
pub enum EvaluateError {
    TranscriptError(transcript::TranscriptError),
    ClaudeError(claude::ClaudeError),
    IoError(std::io::Error),
}

impl std::fmt::Display for EvaluateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluateError::TranscriptError(e) => write!(f, "Transcript error: {}", e),
            EvaluateError::ClaudeError(e) => write!(f, "Claude error: {}", e),
            EvaluateError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for EvaluateError {}

impl From<transcript::TranscriptError> for EvaluateError {
    fn from(e: transcript::TranscriptError) -> Self {
        EvaluateError::TranscriptError(e)
    }
}

impl From<claude::ClaudeError> for EvaluateError {
    fn from(e: claude::ClaudeError) -> Self {
        EvaluateError::ClaudeError(e)
    }
}

impl From<std::io::Error> for EvaluateError {
    fn from(e: std::io::Error) -> Self {
        EvaluateError::IoError(e)
    }
}

/// Result of LLM-based evaluation
#[derive(Debug)]
pub struct LlmEvaluationResult {
    /// The feedback text (or "No concerns." if none)
    pub feedback: String,
    /// Whether there were concerns
    pub has_concerns: bool,
    /// Cost of the LLM call
    pub cost_usd: f64,
    /// Session ID for continuation
    pub session_id: String,
}

/// Parse the structured decision response from the LLM
///
/// Expected format:
/// ```
/// DECISION: ALLOW|BLOCK
///
/// <feedback text>
/// ```
///
/// Returns (has_concerns, feedback_text)
/// AIDEV-NOTE: If parsing fails, defaults to BLOCK to be safe.
fn parse_decision_response(response: &str) -> (bool, String) {
    let lines: Vec<&str> = response.lines().collect();

    if lines.is_empty() {
        return (true, response.to_string()); // Default to block if empty
    }

    let first_line = lines[0].trim();

    // Check for DECISION: prefix
    if let Some(decision_part) = first_line.strip_prefix("DECISION:") {
        let decision = decision_part.trim().to_uppercase();

        // Extract feedback (everything after the first line, skipping blank lines)
        let feedback: String = lines[1..]
            .iter()
            .skip_while(|l| l.trim().is_empty())
            .cloned()
            .collect::<Vec<&str>>()
            .join("\n")
            .trim()
            .to_string();

        match decision.as_str() {
            "ALLOW" => (false, feedback),
            "BLOCK" => (true, feedback),
            _ => {
                // Unknown decision, default to block
                eprintln!("Warning: Unknown decision '{}', defaulting to BLOCK", decision);
                (true, feedback)
            }
        }
    } else {
        // No DECISION prefix found - legacy format or malformed
        // Fall back to old behavior: check for "No concerns"
        let has_concerns = !response.eq_ignore_ascii_case("no concerns.")
            && !response.eq_ignore_ascii_case("no concerns");
        (has_concerns, response.to_string())
    }
}

/// Evaluate conversation using LLM with natural language feedback
///
/// AIDEV-NOTE: This calls Claude with the superego prompt and gets
/// rich natural language feedback that Claude can reason about.
/// Context is everything since last_evaluated - not an arbitrary window.
pub fn evaluate_llm(
    transcript_path: &Path,
    superego_dir: &Path,
) -> Result<LlmEvaluationResult, EvaluateError> {
    // Load state to get last_evaluated timestamp
    let state_mgr = StateManager::new(superego_dir);
    let state = state_mgr.load().unwrap_or_default();

    // Load transcript
    let entries = transcript::read_transcript(transcript_path)?;

    // Get messages since last evaluation
    let messages = transcript::get_messages_since(&entries, state.last_evaluated);

    // Skip if nothing new to evaluate
    if messages.is_empty() {
        return Ok(LlmEvaluationResult {
            feedback: "No concerns.".to_string(),
            has_concerns: false,
            cost_usd: 0.0,
            session_id: String::new(),
        });
    }

    // Load system prompt
    let prompt_path = superego_dir.join("prompt.md");
    let system_prompt = if prompt_path.exists() {
        fs::read_to_string(&prompt_path)?
    } else {
        include_str!("../default_prompt.md").to_string()
    };

    // Format conversation context
    let context = transcript::format_context(&messages);

    // Get bd task context
    let bd_context = match bd::evaluate() {
        Ok(eval) => {
            if let Some(task) = eval.current_task {
                format!("CURRENT TASK: {} - {}\n\n", task.id, task.title)
            } else if eval.read_only {
                "CURRENT TASK: None (no task claimed)\n\n".to_string()
            } else {
                String::new()
            }
        }
        Err(_) => String::new(), // bd not initialized, skip context
    };

    // Check for pending change context (from PreToolUse hook)
    let pending_change_path = superego_dir.join("pending_change.txt");
    let pending_change = if pending_change_path.exists() {
        fs::read_to_string(&pending_change_path).unwrap_or_default()
    } else {
        String::new()
    };

    let pending_context = if !pending_change.is_empty() {
        format!("\n--- PENDING CHANGE (evaluate this!) ---\n{}\n--- END PENDING CHANGE ---\n", pending_change)
    } else {
        String::new()
    };

    // Build message for superego - include bd context and pending change
    let message = format!(
        "Review the following Claude Code conversation and provide feedback.\n\n\
        {}--- CONVERSATION ---\n\
        {}\n\
        --- END CONVERSATION ---{}",
        bd_context,
        context,
        pending_context
    );

    // Load superego session ID if available
    let session_path = superego_dir.join("session").join("session_id");
    let superego_session_id = fs::read_to_string(&session_path).ok();

    // Call Claude
    let options = ClaudeOptions {
        model: Some("sonnet".to_string()),
        session_id: superego_session_id.clone(),
        no_session_persistence: false,
    };

    let response = claude::invoke(&system_prompt, &message, options)?;

    // Save session ID for next time
    if response.session_id != superego_session_id.unwrap_or_default() {
        let session_dir = superego_dir.join("session");
        fs::create_dir_all(&session_dir)?;
        fs::write(session_path, &response.session_id)?;
    }

    // Update last_evaluated timestamp (reuse state_mgr from top)
    if let Err(e) = state_mgr.update(|s| s.mark_evaluated()) {
        eprintln!("Warning: failed to update state: {}", e);
    }

    // Parse the structured response: "DECISION: ALLOW|BLOCK\n\n<feedback>"
    let response_text = response.result.trim();
    let (has_concerns, feedback) = parse_decision_response(response_text);

    // Write to feedback queue and decision journal if there are concerns
    if has_concerns {
        let queue = FeedbackQueue::new(superego_dir);
        let fb = Feedback::warning(&feedback);
        if let Err(e) = queue.write(&fb) {
            eprintln!("Warning: failed to write feedback: {}", e);
        }
        // Record to decision journal for audit trail
        let journal = Journal::new(superego_dir);
        let decision = Decision::feedback_delivered(Some(response.session_id.clone()), feedback.clone());
        if let Err(e) = journal.write(&decision) {
            eprintln!("Warning: failed to write decision journal: {}", e);
        }
    }

    Ok(LlmEvaluationResult {
        feedback,
        has_concerns,
        cost_usd: response.total_cost_usd,
        session_id: response.session_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decision_allow() {
        let response = "DECISION: ALLOW\n\nGreat work! The code follows good patterns.";
        let (has_concerns, feedback) = parse_decision_response(response);
        assert!(!has_concerns);
        assert_eq!(feedback, "Great work! The code follows good patterns.");
    }

    #[test]
    fn test_parse_decision_block() {
        let response = "DECISION: BLOCK\n\nThis may be a local maximum. Have alternatives been considered?";
        let (has_concerns, feedback) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(feedback, "This may be a local maximum. Have alternatives been considered?");
    }

    #[test]
    fn test_parse_decision_case_insensitive() {
        let response = "DECISION: allow\n\nLooks good.";
        let (has_concerns, _) = parse_decision_response(response);
        assert!(!has_concerns);

        let response = "DECISION: Block\n\nConcern here.";
        let (has_concerns, _) = parse_decision_response(response);
        assert!(has_concerns);
    }

    #[test]
    fn test_parse_decision_multiline_feedback() {
        let response = "DECISION: BLOCK\n\nFirst concern.\n\nSecond concern.\n\n- Bullet point";
        let (has_concerns, feedback) = parse_decision_response(response);
        assert!(has_concerns);
        assert!(feedback.contains("First concern."));
        assert!(feedback.contains("Second concern."));
        assert!(feedback.contains("- Bullet point"));
    }

    #[test]
    fn test_parse_decision_legacy_no_concerns() {
        // Legacy format should still work
        let response = "No concerns.";
        let (has_concerns, feedback) = parse_decision_response(response);
        assert!(!has_concerns);
        assert_eq!(feedback, "No concerns.");
    }

    #[test]
    fn test_parse_decision_legacy_with_concerns() {
        // Legacy format - any other text means concerns
        let response = "The code has a bug.";
        let (has_concerns, feedback) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(feedback, "The code has a bug.");
    }

    #[test]
    fn test_parse_decision_unknown_defaults_to_block() {
        let response = "DECISION: MAYBE\n\nNot sure about this.";
        let (has_concerns, _) = parse_decision_response(response);
        assert!(has_concerns); // Unknown decision defaults to block
    }
}

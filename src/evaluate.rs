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

    // The result is natural language feedback
    let feedback = response.result.trim().to_string();
    let has_concerns = !feedback.eq_ignore_ascii_case("no concerns.")
        && !feedback.eq_ignore_ascii_case("no concerns");

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

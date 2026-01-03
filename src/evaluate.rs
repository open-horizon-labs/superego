//! Evaluation for superego
//!
//! LLM-based evaluation with natural language feedback.

use std::fs;
use std::path::Path;

use chrono::Duration;

use crate::ba;
use crate::claude::{self, ClaudeOptions};
use crate::config::Config;
use crate::decision::{Decision, DecisionType, Journal};
use crate::feedback::{Feedback, FeedbackQueue};
use crate::oh::OhIntegration;
use crate::state::StateManager;
use crate::transcript;

/// Error type for evaluation
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
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

/// Confidence level from superego evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::High => write!(f, "HIGH"),
            Confidence::Medium => write!(f, "MEDIUM"),
            Confidence::Low => write!(f, "LOW"),
        }
    }
}

/// Result of LLM-based evaluation
#[derive(Debug)]
pub struct LlmEvaluationResult {
    /// The feedback text (or "No concerns." if none)
    pub feedback: String,
    /// Whether there were concerns
    pub has_concerns: bool,
    /// Confidence level of the evaluation (included in feedback, exposed for callers)
    #[allow(dead_code)]
    pub confidence: Option<Confidence>,
    /// Cost of the LLM call
    pub cost_usd: f64,
}

/// Strip common markdown formatting from a line
/// Handles: # headings, > blockquotes, * bold/italic
fn strip_markdown_prefix(line: &str) -> &str {
    line.trim().trim_start_matches(['#', '>', '*']).trim()
}

/// Parse the structured decision response from the LLM
///
/// Expected format:
/// ```
/// DECISION: ALLOW|BLOCK
/// CONFIDENCE: HIGH|MEDIUM|LOW (optional)
///
/// <feedback text>
/// ```
///
/// Returns (has_concerns, feedback_text, confidence)
/// AIDEV-NOTE: If parsing fails, defaults to BLOCK to be safe.
/// AIDEV-NOTE: Handles markdown variations like "## DECISION:" or "**DECISION:**"
fn parse_decision_response(response: &str) -> (bool, String, Option<Confidence>) {
    let lines: Vec<&str> = response.lines().collect();

    if lines.is_empty() {
        return (true, response.to_string(), None);
    }

    // Search for DECISION: line anywhere in response (handles code fences, extra whitespace, etc.)
    for (idx, line) in lines.iter().enumerate() {
        // Strip markdown formatting (## headings, ** bold, > blockquotes)
        let stripped = strip_markdown_prefix(line);
        if let Some(decision_part) = stripped.strip_prefix("DECISION:") {
            // Also strip trailing markdown (e.g., "DECISION:** ALLOW" → "ALLOW")
            let decision = decision_part.trim_start_matches('*').trim().to_uppercase();

            // Search for optional CONFIDENCE: in next few lines (allows blank lines between)
            let mut confidence: Option<Confidence> = None;
            let mut confidence_line_idx: Option<usize> = None;
            for offset in 1..=3 {
                if let Some(l) = lines.get(idx + offset) {
                    let trimmed = l.trim();
                    if trimmed.is_empty() {
                        continue; // Skip blank lines
                    }
                    // First non-empty line: either CONFIDENCE or start of feedback
                    if let Some(c) = trimmed.strip_prefix("CONFIDENCE:") {
                        confidence = match c.trim().to_uppercase().as_str() {
                            "HIGH" => Some(Confidence::High),
                            "MEDIUM" => Some(Confidence::Medium),
                            "LOW" => Some(Confidence::Low),
                            _ => None,
                        };
                        if confidence.is_some() {
                            confidence_line_idx = Some(idx + offset);
                        }
                    }
                    break; // Stop at first non-empty line
                }
            }

            // Extract feedback (skip past CONFIDENCE line if found)
            let start = confidence_line_idx.map_or(idx + 1, |ci| ci + 1);
            let feedback: String = lines[start..]
                .iter()
                .skip_while(|l| l.trim().is_empty())
                .cloned()
                .collect::<Vec<&str>>()
                .join("\n")
                .trim()
                .trim_end_matches("```")
                .trim()
                .to_string();

            match decision.as_str() {
                "ALLOW" => return (false, feedback, confidence),
                "BLOCK" => return (true, feedback, confidence),
                _ => {
                    eprintln!(
                        "Warning: Unknown decision '{}', defaulting to BLOCK",
                        decision
                    );
                    return (true, feedback, confidence);
                }
            }
        }
    }

    // No DECISION prefix found - legacy format or malformed
    // Fall back to old behavior: check for "No concerns"
    let has_concerns = !response.eq_ignore_ascii_case("no concerns.")
        && !response.eq_ignore_ascii_case("no concerns");
    (has_concerns, response.to_string(), None)
}

/// Evaluate conversation using LLM with natural language feedback
///
/// AIDEV-NOTE: This calls Claude with the superego prompt and gets
/// rich natural language feedback that Claude can reason about.
/// Context is everything since last_evaluated - not an arbitrary window.
/// When session_id is provided, uses session-namespaced paths for state isolation.
pub fn evaluate_llm(
    transcript_path: &Path,
    superego_dir: &Path,
    session_id: Option<&str>,
) -> Result<LlmEvaluationResult, EvaluateError> {
    // Use session-namespaced directory for state if session_id provided
    let session_dir = if let Some(sid) = session_id {
        superego_dir.join("sessions").join(sid)
    } else {
        superego_dir.to_path_buf()
    };

    // Ensure session directory exists
    if session_id.is_some() {
        fs::create_dir_all(&session_dir)?;
    }

    // Load state to get last_evaluated timestamp (from session dir)
    let state_mgr = StateManager::new(&session_dir);
    let state = state_mgr.load().unwrap_or_default();

    // AIDEV-NOTE: Capture read timestamp NOW, before reading transcript.
    // This creates a barrier: "we've evaluated everything as of this moment".
    // Messages written during LLM eval will be caught by next evaluation.
    // Using Utc::now() at read time (not finish time) prevents race conditions.
    let transcript_read_at = chrono::Utc::now();

    // Auto-detect transcript format and load appropriately
    // AIDEV-NOTE: transcript_entries is kept around for carryover context (avoids double read)
    let (context, transcript_entries) = if transcript::codex::is_codex_format(transcript_path) {
        // Codex format
        let entries = transcript::codex::read_codex_transcript(transcript_path)?;
        if entries.is_empty() {
            return Ok(LlmEvaluationResult {
                feedback: "No concerns.".to_string(),
                has_concerns: false,
                confidence: None,
                cost_usd: 0.0,
            });
        }
        (
            transcript::codex::format_codex_context(&entries),
            Vec::new(),
        )
    } else {
        // Claude Code format
        let entries = transcript::read_transcript(transcript_path)?;

        // Get messages since last evaluation, filtered by session_id to prevent cross-session bleed
        let messages = transcript::get_messages_since(&entries, state.last_evaluated, session_id);

        // Skip if nothing new to evaluate
        if messages.is_empty() {
            return Ok(LlmEvaluationResult {
                feedback: "No concerns.".to_string(),
                has_concerns: false,
                confidence: None,
                cost_usd: 0.0,
            });
        }

        (transcript::format_context(&messages), entries)
    };

    // Load config for carryover settings
    let config = Config::load(superego_dir);

    // Build carryover context for continuity (replaces session resumption)
    // AIDEV-NOTE: Instead of resuming Claude sessions (which accumulates unbounded context),
    // we provide explicit carryover: recent decisions + recent messages before
    // the current evaluation window. Counts configurable in config.yaml.
    let carryover_context = {
        let mut parts = Vec::new();

        // Get recent decisions from journal (sorted oldest first, so reverse and take N)
        let journal = Journal::new(&session_dir);
        if let Ok(decisions) = journal.read_all() {
            let recent: Vec<_> = decisions
                .iter()
                .rev()
                .filter(|d| d.decision_type == DecisionType::FeedbackDelivered)
                .take(config.carryover_decision_count)
                .collect();

            if !recent.is_empty() {
                parts.push("Recent superego decisions:".to_string());
                for d in recent.iter().rev() {
                    let feedback = d.context.as_deref().unwrap_or("(no context)");
                    parts.push(format!(
                        "- [{}]: {}",
                        d.timestamp.format("%H:%M:%S"),
                        feedback
                    ));
                }
                parts.push(String::new()); // blank line
            }
        }

        // Get messages from N minutes before last_evaluated (if we have a cutoff)
        // Uses transcript_entries loaded earlier (avoids double read)
        if let Some(cutoff) = state.last_evaluated {
            let window_start = cutoff - Duration::minutes(config.carryover_window_minutes);
            let recent_messages = transcript::get_messages_in_window(
                &transcript_entries,
                window_start,
                cutoff,
                session_id,
            );

            if !recent_messages.is_empty() {
                parts.push("Recent activity (before current evaluation window):".to_string());
                parts.push(transcript::format_context(&recent_messages));
            }
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!(
                "--- PREVIOUS CONTEXT ---\n{}\n--- END PREVIOUS CONTEXT ---\n\n",
                parts.join("\n")
            )
        }
    };

    // Load system prompt
    let prompt_path = superego_dir.join("prompt.md");
    let system_prompt = if prompt_path.exists() {
        fs::read_to_string(&prompt_path)?
    } else {
        include_str!("../default_prompt.md").to_string()
    };

    // Get ba task context (only include if there IS a task - for drift detection)
    let ba_context = match ba::evaluate() {
        Ok(eval) => {
            if let Some(task) = eval.current_task {
                format!("CURRENT TASK: {} - {}\n\n", task.id, task.title)
            } else {
                String::new() // No task = no context (don't prime workflow concerns)
            }
        }
        Err(_) => String::new(),
    };

    // Get OH endeavor context (optional - graceful degradation if unavailable)
    let oh_context = OhIntegration::new(superego_dir)
        .map(|oh| oh.get_endeavor_context())
        .unwrap_or_default();

    // Check for pending change context (from PreToolUse hook) - session-namespaced
    let pending_change_path = session_dir.join("pending_change.txt");
    let pending_change = if pending_change_path.exists() {
        fs::read_to_string(&pending_change_path).unwrap_or_default()
    } else {
        String::new()
    };

    let pending_context = if !pending_change.is_empty() {
        format!(
            "\n--- PENDING CHANGE (evaluate this!) ---\n{}\n--- END PENDING CHANGE ---\n",
            pending_change
        )
    } else {
        String::new()
    };

    // Build message for superego - include carryover, ba context, OH context, and pending change
    // AIDEV-NOTE: carryover_context provides continuity without session resumption
    let message = format!(
        "Review the following Claude Code conversation and provide feedback.\n\n\
        {}{}{}--- CONVERSATION ---\n\
        {}\n\
        --- END CONVERSATION ---{}",
        carryover_context, ba_context, oh_context, context, pending_context
    );

    // Call Claude - each evaluation is isolated (no session resumption)
    // AIDEV-NOTE: Session resumption was removed because it accumulates context unboundedly,
    // eventually causing "Prompt is too long" errors. Carryover context provides continuity instead.
    let options = ClaudeOptions {
        model: None,
        session_id: None, // No resumption - isolated evaluations
        no_session_persistence: true,
        timeout_ms: None,
    };

    let response = claude::invoke(&system_prompt, &message, options)?;

    // Update last_evaluated to transcript read time (not completion time!)
    // This ensures messages written during LLM eval are caught next time.
    if let Err(e) = state_mgr.update(|s| s.mark_evaluated_at(transcript_read_at)) {
        eprintln!("Warning: failed to update state: {}", e);
    }

    // Parse the structured response: "DECISION: ALLOW|BLOCK\nCONFIDENCE: ...\n\n<feedback>"
    let response_text = response.result.trim();
    let (has_concerns, feedback, confidence) = parse_decision_response(response_text);

    // Write to feedback queue (session-namespaced) and decision journal if there are concerns
    if has_concerns {
        let queue = FeedbackQueue::new(&session_dir);
        // Include confidence in feedback so agent sees it
        let feedback_with_confidence = if let Some(conf) = confidence {
            format!("CONFIDENCE: {}\n\n{}", conf, feedback)
        } else {
            feedback.clone()
        };
        let fb = Feedback::warning(&feedback_with_confidence);
        if let Err(e) = queue.write(&fb) {
            eprintln!("ERROR: failed to write feedback file: {}", e);
            eprintln!("FEEDBACK CONTENT (fallback):\n{}", feedback_with_confidence);
        }
        // Record to decision journal for audit trail (session-namespaced per user requirement)
        let journal = Journal::new(&session_dir);
        let decision =
            Decision::feedback_delivered(Some(response.session_id.clone()), feedback.clone());
        if let Err(e) = journal.write(&decision) {
            eprintln!("Warning: failed to write decision journal: {}", e);
        }

        // Log to Open Horizons if configured (optional integration)
        if let Some(oh) = OhIntegration::new(superego_dir) {
            if let Err(e) = oh.log_feedback(&feedback) {
                eprintln!("Warning: failed to log to Open Horizons: {}", e);
            }
        }
    }

    Ok(LlmEvaluationResult {
        feedback,
        has_concerns,
        confidence,
        cost_usd: response.total_cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decision_allow() {
        let response = "DECISION: ALLOW\n\nGreat work! The code follows good patterns.";
        let (has_concerns, feedback, confidence) = parse_decision_response(response);
        assert!(!has_concerns);
        assert_eq!(feedback, "Great work! The code follows good patterns.");
        assert_eq!(confidence, None);
    }

    #[test]
    fn test_parse_decision_block() {
        let response =
            "DECISION: BLOCK\n\nThis may be a local maximum. Have alternatives been considered?";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(
            feedback,
            "This may be a local maximum. Have alternatives been considered?"
        );
    }

    #[test]
    fn test_parse_decision_with_confidence() {
        let response = "DECISION: BLOCK\nCONFIDENCE: HIGH\n\nThis is over-engineered.";
        let (has_concerns, feedback, confidence) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(feedback, "This is over-engineered.");
        assert_eq!(confidence, Some(Confidence::High));

        let response = "DECISION: ALLOW\nCONFIDENCE: LOW\n\nLooks okay but uncertain.";
        let (has_concerns, feedback, confidence) = parse_decision_response(response);
        assert!(!has_concerns);
        assert_eq!(feedback, "Looks okay but uncertain.");
        assert_eq!(confidence, Some(Confidence::Low));
    }

    #[test]
    fn test_parse_decision_case_insensitive() {
        let response = "DECISION: allow\n\nLooks good.";
        let (has_concerns, _, _) = parse_decision_response(response);
        assert!(!has_concerns);

        let response = "DECISION: Block\n\nConcern here.";
        let (has_concerns, _, _) = parse_decision_response(response);
        assert!(has_concerns);
    }

    #[test]
    fn test_parse_decision_multiline_feedback() {
        let response = "DECISION: BLOCK\n\nFirst concern.\n\nSecond concern.\n\n- Bullet point";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(has_concerns);
        assert!(feedback.contains("First concern."));
        assert!(feedback.contains("Second concern."));
        assert!(feedback.contains("- Bullet point"));
    }

    #[test]
    fn test_parse_decision_legacy_no_concerns() {
        // Legacy format should still work
        let response = "No concerns.";
        let (has_concerns, feedback, confidence) = parse_decision_response(response);
        assert!(!has_concerns);
        assert_eq!(feedback, "No concerns.");
        assert_eq!(confidence, None);
    }

    #[test]
    fn test_parse_decision_legacy_with_concerns() {
        // Legacy format - any other text means concerns
        let response = "The code has a bug.";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(feedback, "The code has a bug.");
    }

    #[test]
    fn test_parse_decision_unknown_defaults_to_block() {
        let response = "DECISION: MAYBE\n\nNot sure about this.";
        let (has_concerns, _, _) = parse_decision_response(response);
        assert!(has_concerns); // Unknown decision defaults to block
    }

    #[test]
    fn test_parse_decision_markdown_heading() {
        // LLMs often output "## DECISION: ALLOW" as a markdown heading
        let response = "## DECISION: ALLOW\n\nExcellent work on this implementation.";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(!has_concerns, "Should parse ALLOW despite ## prefix");
        assert_eq!(feedback, "Excellent work on this implementation.");

        let response = "## DECISION: BLOCK\n\nThis needs review.";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(has_concerns, "Should parse BLOCK despite ## prefix");
        assert_eq!(feedback, "This needs review.");
    }

    #[test]
    fn test_parse_decision_markdown_bold() {
        // Handle **DECISION:** format
        let response = "**DECISION:** ALLOW\n\nLooks good.";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(!has_concerns, "Should parse ALLOW despite ** prefix");
        assert_eq!(feedback, "Looks good.");
    }

    #[test]
    fn test_parse_decision_markdown_blockquote() {
        // Handle > DECISION: format
        let response = "> DECISION: ALLOW\n\nApproved.";
        let (has_concerns, feedback, _) = parse_decision_response(response);
        assert!(!has_concerns, "Should parse ALLOW despite > prefix");
        assert_eq!(feedback, "Approved.");
    }

    #[test]
    fn test_parse_decision_confidence_with_blank_line() {
        // Allow blank lines between DECISION and CONFIDENCE
        let response = "DECISION: BLOCK\n\nCONFIDENCE: MEDIUM\n\nNeeds review.";
        let (has_concerns, feedback, confidence) = parse_decision_response(response);
        assert!(has_concerns);
        assert_eq!(feedback, "Needs review.");
        assert_eq!(confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_strip_markdown_prefix() {
        assert_eq!(strip_markdown_prefix("## DECISION:"), "DECISION:");
        assert_eq!(strip_markdown_prefix("### DECISION:"), "DECISION:");
        assert_eq!(strip_markdown_prefix("**DECISION:**"), "DECISION:**");
        assert_eq!(strip_markdown_prefix("> DECISION:"), "DECISION:");
        assert_eq!(strip_markdown_prefix("  ## DECISION:"), "DECISION:");
        assert_eq!(strip_markdown_prefix("DECISION:"), "DECISION:");
    }
}

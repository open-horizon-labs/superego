/// Phase evaluation for superego
///
/// Evaluates conversation context and infers phase using Claude CLI.

use std::fs;
use std::path::Path;

use crate::claude::{self, ClaudeOptions, SuperegoEvaluation};
use crate::decision::{Decision, Journal, Phase};
use crate::state::{State, StateManager};
use crate::transcript::{self, TranscriptEntry};

/// Error type for evaluation
#[derive(Debug)]
pub enum EvaluateError {
    TranscriptError(transcript::TranscriptError),
    ClaudeError(claude::ClaudeError),
    IoError(std::io::Error),
    StateError(crate::state::StateError),
    JournalError(crate::decision::JournalError),
}

impl std::fmt::Display for EvaluateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluateError::TranscriptError(e) => write!(f, "Transcript error: {}", e),
            EvaluateError::ClaudeError(e) => write!(f, "Claude error: {}", e),
            EvaluateError::IoError(e) => write!(f, "IO error: {}", e),
            EvaluateError::StateError(e) => write!(f, "State error: {}", e),
            EvaluateError::JournalError(e) => write!(f, "Journal error: {}", e),
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

impl From<crate::state::StateError> for EvaluateError {
    fn from(e: crate::state::StateError) -> Self {
        EvaluateError::StateError(e)
    }
}

impl From<crate::decision::JournalError> for EvaluateError {
    fn from(e: crate::decision::JournalError) -> Self {
        EvaluateError::JournalError(e)
    }
}

/// Result of phase evaluation
#[derive(Debug)]
pub struct EvaluationResult {
    pub phase: Phase,
    pub previous_phase: Phase,
    pub approved_scope: Option<String>,
    pub concerns: Vec<String>,
    pub cost_usd: f64,
    pub changed: bool,
}

/// Run phase evaluation
pub fn evaluate(
    transcript_path: &Path,
    superego_dir: &Path,
    context_limit: usize,
) -> Result<EvaluationResult, EvaluateError> {
    // Load transcript
    let entries = transcript::read_transcript(transcript_path)?;
    let main_session_id = transcript::extract_session_id(&entries);

    // Load current state (or default to EXPLORING)
    let state_mgr = StateManager::new(superego_dir);
    let current_state = state_mgr.load().unwrap_or_default();
    let previous_phase = current_state.phase;

    // Load system prompt
    let prompt_path = superego_dir.join("prompt.md");
    let system_prompt = if prompt_path.exists() {
        fs::read_to_string(&prompt_path)?
    } else {
        include_str!("../default_prompt.md").to_string()
    };

    // Format conversation context
    let context = transcript::format_recent_context(&entries, context_limit);

    // Build message for superego
    let message = format!(
        "Evaluate the following conversation and determine the current phase.\n\n\
        Current phase: {}\n\n\
        --- CONVERSATION ---\n\
        {}\n\
        --- END CONVERSATION ---\n\n\
        Respond with JSON only.",
        previous_phase, context
    );

    // Load superego session ID if available
    let session_path = superego_dir.join("session").join("session_id");
    let superego_session_id = fs::read_to_string(&session_path).ok();

    // Call Claude for evaluation
    let options = ClaudeOptions {
        model: Some("sonnet".to_string()),
        session_id: superego_session_id.clone(),
        timeout_secs: Some(30),
        no_session_persistence: false,
    };

    let response = claude::invoke(&system_prompt, &message, options)?;

    // Save superego session ID for next time
    if response.session_id != superego_session_id.unwrap_or_default() {
        let session_dir = superego_dir.join("session");
        fs::create_dir_all(&session_dir)?;
        fs::write(session_path, &response.session_id)?;
    }

    // Parse evaluation
    let eval = claude::parse_evaluation(&response.result)?;

    // Convert phase string to enum
    let new_phase = match eval.phase.to_lowercase().as_str() {
        "ready" => Phase::Ready,
        "discussing" => Phase::Discussing,
        _ => Phase::Exploring,
    };

    // Collect concerns
    let concerns: Vec<String> = eval
        .concerns
        .unwrap_or_default()
        .into_iter()
        .map(|c| format!("{}: {}", c.concern_type, c.description))
        .collect();

    // Update state if phase changed
    let changed = new_phase != previous_phase;
    if changed {
        let mut new_state = current_state.clone();
        new_state.transition_to(new_phase, eval.approved_scope.clone());
        state_mgr.save(&new_state)?;

        // Record in decision journal
        let journal = Journal::new(superego_dir);
        let decision = Decision::phase_transition(
            main_session_id,
            previous_phase,
            new_phase,
            eval.reason.unwrap_or_else(|| "LLM evaluation".to_string()),
            eval.approved_scope.clone(),
        );
        journal.write(&decision)?;
    } else {
        // Update last_evaluated timestamp
        let mut new_state = current_state;
        new_state.last_evaluated = Some(chrono::Utc::now());
        state_mgr.save(&new_state)?;
    }

    Ok(EvaluationResult {
        phase: new_phase,
        previous_phase,
        approved_scope: eval.approved_scope,
        concerns,
        cost_usd: response.total_cost_usd,
        changed,
    })
}

/// Evaluate with fallback to EXPLORING on error
pub fn evaluate_with_fallback(
    transcript_path: &Path,
    superego_dir: &Path,
    context_limit: usize,
) -> EvaluationResult {
    match evaluate(transcript_path, superego_dir, context_limit) {
        Ok(result) => result,
        Err(e) => {
            // AIDEV-NOTE: On error, fall back to EXPLORING (safe default)
            eprintln!("Warning: evaluation failed, falling back to EXPLORING: {}", e);

            let state_mgr = StateManager::new(superego_dir);
            let current_phase = state_mgr
                .load()
                .map(|s| s.phase)
                .unwrap_or(Phase::Exploring);

            EvaluationResult {
                phase: Phase::Exploring,
                previous_phase: current_phase,
                approved_scope: None,
                concerns: vec![format!("Evaluation error: {}", e)],
                cost_usd: 0.0,
                changed: current_phase != Phase::Exploring,
            }
        }
    }
}

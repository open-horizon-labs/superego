//! Ba integration for task state
//!
//! Task state comes from ba, not LLM conversation analysis.
//! AIDEV-NOTE: Simplified - removed unused functions.

use serde::Deserialize;
use std::process::Command;

/// Issue from ba --json list
#[derive(Debug, Clone, Deserialize)]
pub struct BaIssue {
    pub id: String,
    pub title: String,
}

/// Error type for ba operations
#[derive(Debug)]
pub enum BaError {
    CommandFailed(String),
    ParseError(String),
    NotInitialized,
}

impl std::fmt::Display for BaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaError::CommandFailed(msg) => write!(f, "ba command failed: {}", msg),
            BaError::ParseError(msg) => write!(f, "Failed to parse ba output: {}", msg),
            BaError::NotInitialized => write!(f, "ba not initialized in this project"),
        }
    }
}

impl std::error::Error for BaError {}

/// Get issues in progress
fn get_in_progress() -> Result<Vec<BaIssue>, BaError> {
    let output = Command::new("ba")
        .args(["--json", "list", "--status", "in_progress"])
        .output()
        .map_err(|e| BaError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not initialized") || stderr.contains("No database") {
            return Err(BaError::NotInitialized);
        }
        return Err(BaError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Handle empty output
    if stdout.trim().is_empty() || stdout.trim() == "[]" {
        return Ok(Vec::new());
    }

    serde_json::from_str(&stdout).map_err(|e| BaError::ParseError(format!("{}: {}", e, stdout)))
}

/// Check if ba is initialized
fn is_initialized() -> bool {
    Command::new("ba")
        .args(["list"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Evaluation result based on ba state
#[derive(Debug)]
pub struct BaEvaluation {
    /// Current task if any (for drift detection)
    pub current_task: Option<BaIssue>,
}

/// Evaluate current state based on ba
pub fn evaluate() -> Result<BaEvaluation, BaError> {
    if !is_initialized() {
        return Ok(BaEvaluation { current_task: None });
    }

    let tasks = get_in_progress()?;

    // Return first in-progress task (if any) for drift detection
    Ok(BaEvaluation {
        current_task: tasks.into_iter().next(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_initialized() {
        // This will depend on whether ba is installed and initialized
        // Just verify the function doesn't panic
        let _ = is_initialized();
    }
}

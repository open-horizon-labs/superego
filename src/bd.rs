//! Beads (bd) integration for task state
//!
//! Task state comes from bd, not LLM conversation analysis.
//! AIDEV-NOTE: Simplified - removed unused functions.

use serde::Deserialize;
use std::process::Command;

/// Issue from bd list --json
#[derive(Debug, Clone, Deserialize)]
pub struct BdIssue {
    pub id: String,
    pub title: String,
}

/// Error type for bd operations
#[derive(Debug)]
pub enum BdError {
    CommandFailed(String),
    ParseError(String),
    NotInitialized,
}

impl std::fmt::Display for BdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BdError::CommandFailed(msg) => write!(f, "bd command failed: {}", msg),
            BdError::ParseError(msg) => write!(f, "Failed to parse bd output: {}", msg),
            BdError::NotInitialized => write!(f, "bd not initialized in this project"),
        }
    }
}

impl std::error::Error for BdError {}

/// Get issues in progress
fn get_in_progress() -> Result<Vec<BdIssue>, BdError> {
    let output = Command::new("bd")
        .args(["list", "--status", "in_progress", "--json"])
        .output()
        .map_err(|e| BdError::CommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not initialized") || stderr.contains("No database") {
            return Err(BdError::NotInitialized);
        }
        return Err(BdError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Handle empty output
    if stdout.trim().is_empty() || stdout.trim() == "[]" {
        return Ok(Vec::new());
    }

    serde_json::from_str(&stdout).map_err(|e| BdError::ParseError(format!("{}: {}", e, stdout)))
}

/// Check if bd is initialized
fn is_initialized() -> bool {
    Command::new("bd")
        .args(["stats"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Evaluation result based on bd state
#[derive(Debug)]
pub struct BdEvaluation {
    /// Current task if any (for drift detection)
    pub current_task: Option<BdIssue>,
}

/// Evaluate current state based on bd
pub fn evaluate() -> Result<BdEvaluation, BdError> {
    if !is_initialized() {
        return Ok(BdEvaluation { current_task: None });
    }

    let tasks = get_in_progress()?;

    // Return first in-progress task (if any) for drift detection
    Ok(BdEvaluation {
        current_task: tasks.into_iter().next(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_initialized() {
        // This will depend on whether bd is installed and initialized
        // Just verify the function doesn't panic
        let _ = is_initialized();
    }
}

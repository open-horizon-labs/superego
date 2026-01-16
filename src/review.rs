//! On-demand review functionality
//!
//! Allows users to proactively request superego review of changes.

use std::path::Path;
use std::process::{Command, Output};

use crate::claude;
use crate::codex_llm;
use crate::prompts;

/// Run a git command and check for errors
fn run_git(args: &[&str]) -> Result<Output, ReviewError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| ReviewError::GitError(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            return Err(ReviewError::GitError(stderr.trim().to_string()));
        }
    }

    Ok(output)
}

/// Review target type
#[derive(Debug)]
pub enum ReviewTarget {
    /// Staged changes (git diff --cached)
    Staged,
    /// PR diff vs base branch
    Pr,
    /// Specific file
    File(String),
}

impl ReviewTarget {
    /// Parse target from string argument
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            None => ReviewTarget::Staged,
            Some("staged") => ReviewTarget::Staged,
            Some("pr") => ReviewTarget::Pr,
            Some(path) => ReviewTarget::File(path.to_string()),
        }
    }
}

/// Result of a review
#[derive(Debug)]
pub struct ReviewResult {
    pub feedback: String,
    pub target_description: String,
}

/// Error type for review operations
#[derive(Debug)]
pub enum ReviewError {
    NoDiff(String),
    GitError(String),
    LlmError(String),
    NotInitialized,
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewError::NoDiff(msg) => write!(f, "Nothing to review: {}", msg),
            ReviewError::GitError(msg) => write!(f, "Git error: {}", msg),
            ReviewError::LlmError(msg) => write!(f, "LLM error: {}", msg),
            ReviewError::NotInitialized => write!(f, ".superego/ not initialized"),
        }
    }
}

impl std::error::Error for ReviewError {}

/// Get diff content based on target
fn get_diff(target: &ReviewTarget) -> Result<(String, String), ReviewError> {
    let (diff, description) = match target {
        ReviewTarget::Staged => {
            let output = run_git(&["diff", "--cached"])?;
            let diff = String::from_utf8_lossy(&output.stdout).to_string();

            // If nothing staged, fall back to uncommitted
            if diff.trim().is_empty() {
                let output = run_git(&["diff", "HEAD"])?;
                let diff = String::from_utf8_lossy(&output.stdout).to_string();
                if diff.trim().is_empty() {
                    return Err(ReviewError::NoDiff(
                        "no staged or uncommitted changes".to_string(),
                    ));
                }
                (diff, "uncommitted changes (nothing staged)".to_string())
            } else {
                (diff, "staged changes".to_string())
            }
        }
        ReviewTarget::Pr => {
            // Get the base branch (usually main or master)
            let base = get_base_branch()?;
            let diff_ref = format!("{}...HEAD", base);

            let output = run_git(&["diff", &diff_ref])?;
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if diff.trim().is_empty() {
                return Err(ReviewError::NoDiff(format!(
                    "no changes vs {} branch",
                    base
                )));
            }
            (diff, format!("PR changes vs {}", base))
        }
        ReviewTarget::File(path) => {
            // Try staged first, then unstaged
            let output = run_git(&["diff", "--cached", "--", path])?;
            let diff = String::from_utf8_lossy(&output.stdout).to_string();

            if !diff.trim().is_empty() {
                (diff, format!("staged changes in {}", path))
            } else {
                let output = run_git(&["diff", "HEAD", "--", path])?;
                let diff = String::from_utf8_lossy(&output.stdout).to_string();
                if diff.trim().is_empty() {
                    return Err(ReviewError::NoDiff(format!("no changes in {}", path)));
                }
                (diff, format!("changes in {}", path))
            }
        }
    };

    Ok((diff, description))
}

/// Get the base branch for PR comparison
fn get_base_branch() -> Result<String, ReviewError> {
    // Try to get the default branch from git
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace("refs/remotes/origin/", "");
            if !branch.is_empty() {
                return Ok(branch);
            }
        }
    }

    // Fall back to checking for main or master
    let output = Command::new("git")
        .args(["branch", "-l", "main", "master"])
        .output()
        .map_err(|e| ReviewError::GitError(e.to_string()))?;

    let branches = String::from_utf8_lossy(&output.stdout);
    if branches.contains("main") {
        Ok("main".to_string())
    } else if branches.contains("master") {
        Ok("master".to_string())
    } else {
        Err(ReviewError::GitError(
            "could not determine base branch".to_string(),
        ))
    }
}

/// Run a review
pub fn review(superego_dir: &Path, target: ReviewTarget) -> Result<ReviewResult, ReviewError> {
    if !superego_dir.exists() {
        return Err(ReviewError::NotInitialized);
    }

    // Get the diff
    let (diff, description) = get_diff(&target)?;

    // Load the current prompt
    let prompt_path = superego_dir.join("prompt.md");
    let system_prompt = if prompt_path.exists() {
        std::fs::read_to_string(&prompt_path)
            .unwrap_or_else(|_| prompts::PromptType::Code.content().to_string())
    } else {
        prompts::PromptType::Code.content().to_string()
    };

    // Prepare the message
    let message = format!(
        "Review the following changes and provide feedback.\n\n\
        This is an on-demand review requested by the user (not a hook evaluation).\n\
        Provide constructive feedback - no DECISION/BLOCK format needed, just helpful observations.\n\n\
        --- CHANGES ({}) ---\n{}\n--- END CHANGES ---",
        description, diff
    );

    // Call the LLM
    let response = claude::invoke(&system_prompt, &message, claude::ClaudeOptions::default())
        .map_err(|e| ReviewError::LlmError(e.to_string()))?;

    Ok(ReviewResult {
        feedback: response.result,
        target_description: description,
    })
}

/// Run a review using Codex LLM (for Codex skill)
pub fn review_codex(
    superego_dir: &Path,
    target: ReviewTarget,
) -> Result<ReviewResult, ReviewError> {
    if !superego_dir.exists() {
        return Err(ReviewError::NotInitialized);
    }

    // Get the diff
    let (diff, description) = get_diff(&target)?;

    // Load the current prompt
    let prompt_path = superego_dir.join("prompt.md");
    let system_prompt = if prompt_path.exists() {
        std::fs::read_to_string(&prompt_path)
            .unwrap_or_else(|_| prompts::PromptType::Code.content().to_string())
    } else {
        prompts::PromptType::Code.content().to_string()
    };

    // Prepare the message
    let message = format!(
        "Review the following changes and provide feedback.\n\n\
        This is an on-demand review requested by the user (not a hook evaluation).\n\
        Provide constructive feedback - no DECISION/BLOCK format needed, just helpful observations.\n\n\
        --- CHANGES ({}) ---\n{}\n--- END CHANGES ---",
        description, diff
    );

    // Call Codex LLM
    let response = codex_llm::invoke(&system_prompt, &message, None)
        .map_err(|e| ReviewError::LlmError(e.to_string()))?;

    Ok(ReviewResult {
        feedback: response.result,
        target_description: description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_target_from_arg() {
        assert!(matches!(ReviewTarget::from_arg(None), ReviewTarget::Staged));
        assert!(matches!(
            ReviewTarget::from_arg(Some("staged")),
            ReviewTarget::Staged
        ));
        assert!(matches!(
            ReviewTarget::from_arg(Some("pr")),
            ReviewTarget::Pr
        ));
        assert!(matches!(
            ReviewTarget::from_arg(Some("foo.rs")),
            ReviewTarget::File(_)
        ));
    }
}

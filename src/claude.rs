//! Claude CLI invocation
//!
//! Calls the Claude Code CLI for superego evaluation.
//! AIDEV-NOTE: Simplified - removed phase-based JSON parsing.
//! Now just returns raw result text for natural language feedback.

use serde::Deserialize;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Response from Claude CLI in JSON format
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeResponse {
    pub result: String,
    pub session_id: String,
    pub total_cost_usd: f64,
}

/// Error type for Claude invocation
#[derive(Debug)]
pub enum ClaudeError {
    CommandFailed(String),
    ParseError(serde_json::Error),
    IoError(std::io::Error),
    Timeout(Duration),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeError::CommandFailed(msg) => write!(f, "Claude command failed: {}", msg),
            ClaudeError::ParseError(e) => write!(f, "Failed to parse Claude response: {}", e),
            ClaudeError::IoError(e) => write!(f, "IO error: {}", e),
            ClaudeError::Timeout(d) => write!(f, "Claude timed out after {:?}", d),
        }
    }
}

impl std::error::Error for ClaudeError {}

impl From<std::io::Error> for ClaudeError {
    fn from(e: std::io::Error) -> Self {
        ClaudeError::IoError(e)
    }
}

impl From<serde_json::Error> for ClaudeError {
    fn from(e: serde_json::Error) -> Self {
        ClaudeError::ParseError(e)
    }
}

/// Default timeout: 5 minutes
const DEFAULT_TIMEOUT_MS: u64 = 300_000;

/// Options for Claude invocation
#[derive(Debug, Clone, Default)]
pub struct ClaudeOptions {
    /// Model to use (default: sonnet)
    pub model: Option<String>,
    /// Session ID for continuation
    pub session_id: Option<String>,
    /// Don't persist session to disk
    pub no_session_persistence: bool,
    /// Timeout in milliseconds (default: 5 minutes)
    pub timeout_ms: Option<u64>,
}

/// Invoke Claude CLI with a system prompt and user message
///
/// # Arguments
/// * `system_prompt` - System prompt for Claude
/// * `message` - User message / context
/// * `options` - Invocation options
///
/// # Returns
/// * `Ok(ClaudeResponse)` - Successful response
/// * `Err(ClaudeError)` - Error during invocation
pub fn invoke(
    system_prompt: &str,
    message: &str,
    options: ClaudeOptions,
) -> Result<ClaudeResponse, ClaudeError> {
    let mut cmd = Command::new("claude");

    // Non-interactive mode with JSON output
    cmd.arg("-p").arg("--output-format").arg("json");

    // Enable tools for superego to inspect the codebase
    cmd.arg("--tools").arg("Bash,Read,Glob,Grep");

    // System prompt
    cmd.arg("--system-prompt").arg(system_prompt);

    // Model (default to sonnet for cost efficiency)
    let model = options.model.unwrap_or_else(|| "sonnet".to_string());
    cmd.arg("--model").arg(&model);

    // Session handling
    if let Some(session_id) = &options.session_id {
        cmd.arg("--resume").arg(session_id);
    }

    // Don't persist session by default for superego
    if options.no_session_persistence {
        cmd.arg("--no-session-persistence");
    }

    // The message is passed as the prompt argument
    cmd.arg(message);

    // AIDEV-NOTE: Recursion prevention - superego's Claude calls must not
    // trigger hooks that call superego again. Hooks check this env var.
    cmd.env("SUPEREGO_DISABLED", "1");

    // AIDEV-NOTE: Must pipe stdout/stderr to capture output, otherwise
    // wait_with_output() returns empty and JSON parsing fails with EOF.
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());

    // Execute with timeout (default 5 minutes)
    let timeout = Duration::from_millis(options.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let mut child = cmd.spawn()?;
    let start = Instant::now();

    // Poll for completion with timeout
    loop {
        match child.try_wait()? {
            Some(status) => {
                // Process exited - collect output
                let output = child.wait_with_output()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                
                if !status.success() {
                    // Claude CLI returns errors in JSON stdout with is_error: true
                    // Try to parse stdout to get a more helpful error message
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                            return Err(ClaudeError::CommandFailed(result.to_string()));
                        }
                    }
                    // Fall back to stderr if we can't parse stdout
                    let error_msg = if stderr.is_empty() { 
                        stdout.to_string() 
                    } else { 
                        stderr.to_string() 
                    };
                    return Err(ClaudeError::CommandFailed(error_msg));
                }
                let response: ClaudeResponse = serde_json::from_str(&stdout)?;
                return Ok(response);
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the process
                    return Err(ClaudeError::Timeout(timeout));
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that stdout is properly captured when piped.
    /// This verifies the fix for the EOF parsing bug where stdout wasn't piped.
    #[test]
    fn test_stdout_capture_with_piped_stdio() {
        let mut cmd = Command::new("echo");
        cmd.arg(r#"{"result":"test","session_id":"abc","total_cost_usd":0.01}"#);

        // This is what we're testing - stdout must be piped to capture output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().expect("Failed to spawn echo");
        let output = child.wait_with_output().expect("Failed to wait");

        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty(), "stdout should not be empty when piped");

        // Verify we can parse the JSON
        let response: ClaudeResponse =
            serde_json::from_str(stdout.trim()).expect("Should parse JSON");
        assert_eq!(response.result, "test");
        assert_eq!(response.session_id, "abc");
    }
}

//! Claude CLI invocation
//!
//! Calls the Claude Code CLI for superego evaluation.
//! AIDEV-NOTE: Simplified - removed phase-based JSON parsing.
//! Now just returns raw result text for natural language feedback.

use serde::Deserialize;
use serde_json::Value;
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

/// AIDEV-NOTE: Claude CLI can return either:
/// 1. A single JSON object (expected format): `{"result":"...","session_id":"...","total_cost_usd":0.1}`
/// 2. An array of objects (when hooks are present): `[{"type":"system",...},{"type":"result","result":"..."}]`
///
/// This function handles both cases robustly.
/// If parsing fails unexpectedly, run `claude -p --output-format json "test"` to see current format.
fn parse_claude_response(stdout: &str) -> Result<ClaudeResponse, ClaudeError> {
    // First, try parsing as a single ClaudeResponse object (common case)
    // Capture error for later if array parsing also fails
    let single_obj_err = match serde_json::from_str::<ClaudeResponse>(stdout) {
        Ok(response) => return Ok(response),
        Err(e) => e,
    };

    // If that fails, try parsing as an array and find the "type": "result" entry
    if let Ok(array) = serde_json::from_str::<Vec<Value>>(stdout) {
        // Find the result entry (usually the last one)
        for entry in array.iter().rev() {
            if entry.get("type").and_then(|t| t.as_str()) == Some("result") {
                // Extract result field - must be present and non-empty string
                let result = match entry.get("result").and_then(|r| r.as_str()) {
                    Some(s) if !s.is_empty() => s.to_string(),
                    Some(_) => continue, // Empty string, try next entry
                    None => continue,    // Missing or wrong type, try next entry
                };

                // session_id and total_cost_usd are optional, default to empty/0
                let session_id = entry
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let total_cost_usd = entry
                    .get("total_cost_usd")
                    .and_then(|c| c.as_f64())
                    .unwrap_or(0.0);

                return Ok(ClaudeResponse {
                    result,
                    session_id,
                    total_cost_usd,
                });
            }
        }

        // Array found but no valid result entry
        return Err(ClaudeError::CommandFailed(
            "Claude response array contains no valid 'result' entry (missing or empty result field)".to_string(),
        ));
    }

    // Neither format worked - return the original parse error
    Err(ClaudeError::ParseError(single_obj_err))
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

    if let Some(model) = options.model {
        cmd.arg("--model").arg(model);
    }

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
                return parse_claude_response(&stdout);
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

    /// Test parsing single object format (standard case)
    #[test]
    fn test_parse_single_object_response() {
        let json = r#"{"result":"Hello!","session_id":"abc-123","total_cost_usd":0.05}"#;
        let response = parse_claude_response(json).expect("Should parse single object");
        assert_eq!(response.result, "Hello!");
        assert_eq!(response.session_id, "abc-123");
        assert!((response.total_cost_usd - 0.05).abs() < 0.001);
    }

    /// Test parsing array format (when hooks add entries before the result)
    /// This is the format reported in GitHub issue #20
    #[test]
    fn test_parse_array_response_with_hooks() {
        let json = r#"[
            {"type":"system","subtype":"hook_response","session_id":"test-session"},
            {"type":"system","subtype":"init","session_id":"test-session"},
            {"type":"assistant","message":{"content":[{"type":"text","text":"Hi!"}]}},
            {"type":"result","subtype":"success","result":"Hi! How can I help?","session_id":"test-session","total_cost_usd":0.12}
        ]"#;

        let response = parse_claude_response(json).expect("Should parse array format");
        assert_eq!(response.result, "Hi! How can I help?");
        assert_eq!(response.session_id, "test-session");
        assert!((response.total_cost_usd - 0.12).abs() < 0.001);
    }

    /// Test that array without result entry gives helpful error
    #[test]
    fn test_parse_array_without_result_entry() {
        let json = r#"[
            {"type":"system","subtype":"hook_response"},
            {"type":"assistant","message":{}}
        ]"#;

        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention missing result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that empty array gives helpful error
    #[test]
    fn test_parse_empty_array() {
        let json = "[]";
        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention missing result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that result entry with wrong type for result field is skipped
    #[test]
    fn test_parse_array_result_wrong_type() {
        // result is a number instead of string - should fail
        let json = r#"[
            {"type":"result","result":123,"session_id":"test","total_cost_usd":0.1}
        ]"#;

        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention invalid result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that result entry with empty result string is skipped
    #[test]
    fn test_parse_array_empty_result_string() {
        // result is empty string - should fail
        let json = r#"[
            {"type":"result","result":"","session_id":"test","total_cost_usd":0.1}
        ]"#;

        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention invalid result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that result entry missing result field is skipped
    #[test]
    fn test_parse_array_missing_result_field() {
        // type is "result" but no result field
        let json = r#"[
            {"type":"result","session_id":"test","total_cost_usd":0.1}
        ]"#;

        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention missing result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that result entry with null result value is skipped
    #[test]
    fn test_parse_array_result_null() {
        let json = r#"[
            {"type":"result","result":null,"session_id":"test","total_cost_usd":0.1}
        ]"#;

        let err = parse_claude_response(json).unwrap_err();
        match err {
            ClaudeError::CommandFailed(msg) => {
                assert!(
                    msg.contains("no valid 'result' entry"),
                    "Error should mention invalid result: {}",
                    msg
                );
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }
    }

    /// Test that invalid result entries are skipped to find valid ones
    /// This verifies the `continue` logic works when multiple result entries exist
    #[test]
    fn test_parse_array_skips_invalid_finds_valid() {
        // Array has two result entries: first is valid, second (last) is invalid
        // Since we iterate in reverse, we skip the invalid one and find the valid one
        let json = r#"[
            {"type":"result","result":"Valid result","session_id":"s1","total_cost_usd":0.05},
            {"type":"result","result":"","session_id":"s2","total_cost_usd":0.10}
        ]"#;

        let response = parse_claude_response(json).expect("Should find valid entry");
        assert_eq!(response.result, "Valid result");
        assert_eq!(response.session_id, "s1");
    }

    /// Test that invalid JSON returns parse error
    #[test]
    fn test_parse_invalid_json() {
        let json = "not valid json at all";
        let err = parse_claude_response(json).unwrap_err();
        assert!(matches!(err, ClaudeError::ParseError(_)));
    }
}

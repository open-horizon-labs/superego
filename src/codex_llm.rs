//! Codex CLI invocation for LLM evaluation
//!
//! Uses `codex exec --json` to run superego evaluation using Codex's own LLM.
//! This allows Codex users to run superego without needing Claude CLI installed.

use serde::Deserialize;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Response from Codex exec
#[derive(Debug, Clone)]
pub struct CodexLlmResponse {
    pub result: String,
    #[allow(dead_code)]
    pub session_id: String,
    pub total_tokens: u64,
}

/// Error type for Codex invocation
#[derive(Debug)]
pub enum CodexLlmError {
    CommandFailed(String),
    ParseError(String),
    IoError(std::io::Error),
    Timeout(Duration),
    NotInstalled,
    RateLimited { resets_in_seconds: Option<u64> },
}

impl std::fmt::Display for CodexLlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodexLlmError::CommandFailed(msg) => write!(f, "Codex command failed: {}", msg),
            CodexLlmError::ParseError(msg) => write!(f, "Failed to parse Codex response: {}", msg),
            CodexLlmError::IoError(e) => write!(f, "IO error: {}", e),
            CodexLlmError::Timeout(d) => write!(f, "Codex timed out after {:?}", d),
            CodexLlmError::NotInstalled => write!(f, "Codex CLI not installed"),
            CodexLlmError::RateLimited { resets_in_seconds } => {
                if let Some(secs) = resets_in_seconds {
                    write!(f, "Rate limited (resets in {} minutes)", secs / 60)
                } else {
                    write!(f, "Rate limited")
                }
            }
        }
    }
}

impl std::error::Error for CodexLlmError {}

impl From<std::io::Error> for CodexLlmError {
    fn from(e: std::io::Error) -> Self {
        CodexLlmError::IoError(e)
    }
}

/// JSONL event from codex exec --json
#[derive(Debug, Deserialize)]
struct CodexEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    item: Option<CodexItem>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    usage: Option<CodexUsage>,
}

#[derive(Debug, Deserialize)]
struct CodexItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// Default timeout: 3 minutes
const DEFAULT_TIMEOUT_MS: u64 = 180_000;

/// Check if Codex CLI is available
pub fn is_available() -> bool {
    Command::new("codex")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Invoke Codex exec with a prompt for evaluation
pub fn invoke(
    system_prompt: &str,
    message: &str,
    timeout_ms: Option<u64>,
) -> Result<CodexLlmResponse, CodexLlmError> {
    if !is_available() {
        return Err(CodexLlmError::NotInstalled);
    }

    let mut cmd = Command::new("codex");

    // Non-interactive exec mode with JSONL output
    // Skip git repo check since we're running as a meta-evaluator
    // Use "-" to read prompt from stdin (avoids CLI arg length limits)
    cmd.arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("-");

    // Combine system prompt and message
    let full_prompt = format!(
        "{}\n\n---\n\n{}\n\n---\n\nRespond with DECISION: ALLOW or DECISION: BLOCK followed by your feedback.",
        system_prompt, message
    );

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());

    // Recursion prevention - superego's Codex calls must not trigger
    // hooks/skills that call superego again.
    cmd.env("SUPEREGO_DISABLED", "1");

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let mut child = cmd.spawn()?;

    // Write prompt to stdin before waiting
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(full_prompt.as_bytes())?;
        drop(stdin); // Explicitly close stdin to signal EOF
    }

    let start = Instant::now();

    loop {
        match child.try_wait()? {
            Some(status) => {
                let output = child.wait_with_output()?;

                if !status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    // Check for rate limiting (429)
                    if stderr.contains("429") || stderr.contains("usage_limit_reached") {
                        // Try to extract resets_in_seconds from the error
                        let resets_in = stderr.find("resets_in_seconds\":").and_then(|i| {
                            let start = i + 19; // length of "resets_in_seconds\":"
                            let rest = &stderr[start..];
                            rest.split(|c: char| !c.is_ascii_digit())
                                .next()
                                .and_then(|s| s.parse::<u64>().ok())
                        });
                        return Err(CodexLlmError::RateLimited {
                            resets_in_seconds: resets_in,
                        });
                    }

                    return Err(CodexLlmError::CommandFailed(stderr.to_string()));
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                return parse_codex_output(&stdout);
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CodexLlmError::Timeout(timeout));
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Parse JSONL output from codex exec --json
fn parse_codex_output(output: &str) -> Result<CodexLlmResponse, CodexLlmError> {
    let mut result_text = String::new();
    let mut thread_id = String::new();
    let mut total_tokens: u64 = 0;

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(event) = serde_json::from_str::<CodexEvent>(line) {
            if let Some(tid) = event.thread_id {
                thread_id = tid;
            }

            if event.event_type == "item.completed" {
                if let Some(item) = event.item {
                    if item.item_type == "agent_message" {
                        if let Some(text) = item.text {
                            result_text = text;
                        }
                    }
                }
            }

            if let Some(usage) = event.usage {
                total_tokens = usage.input_tokens + usage.output_tokens;
            }
        }
    }

    if result_text.is_empty() {
        return Err(CodexLlmError::ParseError(
            "No agent_message found in output".to_string(),
        ));
    }

    Ok(CodexLlmResponse {
        result: result_text,
        session_id: thread_id,
        total_tokens,
    })
}

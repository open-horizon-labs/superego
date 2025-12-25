//! Open Horizons integration for superego
//!
//! Optional integration that logs superego decisions to OH endeavors.
//! Enabled when OH_API_URL and OH_API_KEY environment variables are set.
//!
//! AIDEV-NOTE: This is completely optional - if OH is not configured,
//! superego works exactly as before. The integration enables higher-level
//! coordination by connecting metacognitive feedback to strategic context.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

/// OH API configuration from environment
#[derive(Debug, Clone)]
pub struct OhConfig {
    pub api_url: String,
    pub api_key: String,
}

impl OhConfig {
    /// Try to load configuration from environment variables
    /// Returns None if OH_API_KEY is not set (OH_API_URL has default)
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("OH_API_KEY").ok()?;
        let api_url =
            env::var("OH_API_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());
        Some(OhConfig { api_url, api_key })
    }

    /// Try to load configuration from .superego/config.yaml
    /// Priority: env vars override config file values
    pub fn from_config(superego_dir: &Path) -> Option<Self> {
        let config_path = superego_dir.join("config.yaml");
        let content = fs::read_to_string(&config_path).ok()?;

        // Parse oh_api_key and oh_api_url from config (env vars override)
        let api_key = env::var("OH_API_KEY")
            .ok()
            .or_else(|| parse_config_value(&content, "oh_api_key"))?;

        let api_url = env::var("OH_API_URL")
            .ok()
            .or_else(|| parse_config_value(&content, "oh_api_url"))
            .unwrap_or_else(|| "http://localhost:3001".to_string());

        Some(OhConfig { api_url, api_key })
    }
}

/// Parse a string value from config file content
fn parse_config_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix(key).and_then(|s| s.strip_prefix(':')) {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Error type for OH operations
#[derive(Debug)]
pub enum OhError {
    /// HTTP request failed
    RequestFailed(String),
    /// Failed to parse response
    ParseError(String),
    /// OH not configured (not an error, just skip)
    NotConfigured,
    /// API returned an error
    ApiError(u16, String),
}

impl std::fmt::Display for OhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OhError::RequestFailed(msg) => write!(f, "OH request failed: {}", msg),
            OhError::ParseError(msg) => write!(f, "Failed to parse OH response: {}", msg),
            OhError::NotConfigured => write!(f, "OH not configured"),
            OhError::ApiError(status, msg) => write!(f, "OH API error ({}): {}", status, msg),
        }
    }
}

impl std::error::Error for OhError {}

/// Full endeavor details from GET /api/endeavors/:id
#[derive(Debug, Clone, Deserialize)]
pub struct OhEndeavorFull {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GetEndeavorResponse {
    endeavor: OhEndeavorFull,
}

/// Log entry from GET /api/logs
#[derive(Debug, Clone, Deserialize)]
pub struct OhLogEntry {
    pub content: String,
    pub log_date: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GetLogsResponse {
    logs: Vec<OhLogEntry>,
}

/// Guardrail from GET /api/endeavors/:id/extensions
#[derive(Debug, Clone, Deserialize)]
pub struct OhGuardrail {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub severity: String, // "hard", "soft", "advisory"
    #[serde(default)]
    pub enforcement: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub inherited_from: Option<String>,
    #[serde(default)]
    pub depth: i32,
}

/// Metis entry from GET /api/endeavors/:id/extensions
#[derive(Debug, Clone, Deserialize)]
pub struct OhMetis {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub freshness: String, // "recent", "stale", "historical"
    #[serde(default)]
    pub source: Option<String>,
}

/// Extensions response from GET /api/endeavors/:id/extensions
#[derive(Debug, Clone, Deserialize)]
pub struct OhExtensions {
    pub endeavor_id: String,
    pub guardrails: Vec<OhGuardrail>,
    pub metis: Vec<OhMetis>,
}

/// Response from creating a log entry
#[derive(Debug, Clone, Deserialize)]
pub struct LogResponse {
    pub log: Option<LogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub id: String,
}

/// OH API client
#[derive(Debug, Clone)]
pub struct OhClient {
    config: OhConfig,
}

impl OhClient {
    /// Create a new OH client if configuration is available (env vars only)
    pub fn new() -> Result<Self, OhError> {
        let config = OhConfig::from_env().ok_or(OhError::NotConfigured)?;
        Ok(OhClient { config })
    }

    /// Create a new OH client from config file (with env var override)
    pub fn from_config(superego_dir: &Path) -> Result<Self, OhError> {
        let config = OhConfig::from_config(superego_dir).ok_or(OhError::NotConfigured)?;
        Ok(OhClient { config })
    }

    /// Log a decision to an endeavor
    pub fn log_decision(
        &self,
        endeavor_id: &str,
        content: &str,
        log_date: Option<&str>,
    ) -> Result<String, OhError> {
        let url = format!("{}/api/logs", self.config.api_url);

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let date = log_date.unwrap_or(&today);

        #[derive(Serialize)]
        struct LogRequest<'a> {
            entity_type: &'a str,
            entity_id: &'a str,
            content: &'a str,
            content_type: &'a str,
            log_date: &'a str,
        }

        let request = LogRequest {
            entity_type: "endeavor",
            entity_id: endeavor_id,
            content,
            content_type: "markdown",
            log_date: date,
        };

        let response = attohttpc::post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .json(&request)
            .map_err(|e| OhError::RequestFailed(e.to_string()))?
            .send()
            .map_err(|e| OhError::RequestFailed(e.to_string()))?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(OhError::ApiError(status, body));
        }

        let body = response
            .text()
            .map_err(|e| OhError::ParseError(e.to_string()))?;
        let log_response: LogResponse = serde_json::from_str(&body)
            .map_err(|e| OhError::ParseError(format!("{}: {}", e, body)))?;

        Ok(log_response
            .log
            .map(|l| l.id)
            .unwrap_or_else(|| "unknown".to_string()))
    }

    /// Get a single endeavor by ID
    pub fn get_endeavor(&self, endeavor_id: &str) -> Result<OhEndeavorFull, OhError> {
        let url = format!(
            "{}/api/endeavors/{}",
            self.config.api_url,
            urlencoding::encode(endeavor_id)
        );

        let response = attohttpc::get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| OhError::RequestFailed(e.to_string()))?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(OhError::ApiError(status, body));
        }

        let body = response
            .text()
            .map_err(|e| OhError::ParseError(e.to_string()))?;
        let wrapper: GetEndeavorResponse = serde_json::from_str(&body)
            .map_err(|e| OhError::ParseError(format!("{}: {}", e, body)))?;

        Ok(wrapper.endeavor)
    }

    /// Log a retrospective to an endeavor with full metadata
    ///
    /// Uses the metadata JSONB field to store structured retrospective data
    /// that OH can visualize independently.
    pub fn log_retrospective(
        &self,
        payload: &crate::retro::RetrospectivePayload,
    ) -> Result<String, OhError> {
        let url = format!("{}/api/logs", self.config.api_url);

        let response = attohttpc::post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(10))
            .json(payload)
            .map_err(|e| OhError::RequestFailed(e.to_string()))?
            .send()
            .map_err(|e| OhError::RequestFailed(e.to_string()))?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(OhError::ApiError(status, body));
        }

        let body = response
            .text()
            .map_err(|e| OhError::ParseError(e.to_string()))?;
        let log_response: LogResponse = serde_json::from_str(&body)
            .map_err(|e| OhError::ParseError(format!("{}: {}", e, body)))?;

        Ok(log_response
            .log
            .map(|l| l.id)
            .unwrap_or_else(|| "unknown".to_string()))
    }

    /// Get recent logs for an endeavor
    pub fn get_logs(&self, endeavor_id: &str, days: u32) -> Result<Vec<OhLogEntry>, OhError> {
        let end_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let start_date = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();

        let url = format!(
            "{}/api/logs?entity_type=endeavor&entity_id={}&start_date={}&end_date={}&limit=10",
            self.config.api_url,
            urlencoding::encode(endeavor_id),
            urlencoding::encode(&start_date),
            urlencoding::encode(&end_date)
        );

        let response = attohttpc::get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| OhError::RequestFailed(e.to_string()))?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(OhError::ApiError(status, body));
        }

        let body = response
            .text()
            .map_err(|e| OhError::ParseError(e.to_string()))?;
        let wrapper: GetLogsResponse = serde_json::from_str(&body)
            .map_err(|e| OhError::ParseError(format!("{}: {}", e, body)))?;

        Ok(wrapper.logs)
    }

    /// Get extensions (guardrails + metis) for an endeavor
    /// Returns inherited guardrails from ancestors and relevant metis
    pub fn get_extensions(&self, endeavor_id: &str) -> Result<OhExtensions, OhError> {
        let url = format!(
            "{}/api/endeavors/{}/extensions",
            self.config.api_url,
            urlencoding::encode(endeavor_id)
        );

        let response = attohttpc::get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| OhError::RequestFailed(e.to_string()))?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(OhError::ApiError(status, body));
        }

        let body = response
            .text()
            .map_err(|e| OhError::ParseError(e.to_string()))?;
        let extensions: OhExtensions = serde_json::from_str(&body)
            .map_err(|e| OhError::ParseError(format!("{}: {}", e, body)))?;

        Ok(extensions)
    }
}

/// Parse oh_endeavor_id from config file content
/// Extracted for testability (avoids env var interference in tests)
fn parse_config_for_endeavor_id(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("oh_endeavor_id:") {
            if let Some(value) = line.strip_prefix("oh_endeavor_id:") {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Get the configured OH endeavor ID from environment or config file
///
/// Priority:
/// 1. OH_ENDEAVOR_ID environment variable (for overrides)
/// 2. oh_endeavor_id in .superego/config.yaml
///
/// Returns None if not configured (OH integration will be skipped)
pub fn get_endeavor_id(superego_dir: &Path) -> Option<String> {
    // First check env var (allows override)
    if let Ok(id) = env::var("OH_ENDEAVOR_ID") {
        if !id.is_empty() {
            return Some(id);
        }
    }

    // Then check config.yaml
    let config_path = superego_dir.join("config.yaml");
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            return parse_config_for_endeavor_id(&content);
        }
    }

    None
}

/// Full OH integration configuration
/// Combines API config with endeavor targeting
#[derive(Debug, Clone)]
pub struct OhIntegration {
    pub client: OhClient,
    pub endeavor_id: String,
}

impl OhIntegration {
    /// Try to create a fully configured OH integration
    /// Returns None if either API is not configured or endeavor ID is not set
    pub fn new(superego_dir: &Path) -> Option<Self> {
        let client = OhClient::new().ok()?;
        let endeavor_id = get_endeavor_id(superego_dir)?;
        Some(OhIntegration {
            client,
            endeavor_id,
        })
    }

    /// Log superego feedback to the configured endeavor
    pub fn log_feedback(&self, feedback: &str) -> Result<String, OhError> {
        let content = format!("## Superego Feedback\n\n{}", feedback);
        self.client.log_decision(&self.endeavor_id, &content, None)
    }

    /// Get formatted endeavor context for evaluation
    /// Returns empty string if fetching fails (graceful degradation)
    pub fn get_endeavor_context(&self) -> String {
        // Fetch endeavor details
        let endeavor = match self.client.get_endeavor(&self.endeavor_id) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Warning: failed to fetch OH endeavor: {}", e);
                return String::new();
            }
        };

        // Fetch extensions (guardrails + metis)
        let extensions = match self.client.get_extensions(&self.endeavor_id) {
            Ok(ext) => Some(ext),
            Err(e) => {
                eprintln!("Warning: failed to fetch OH extensions: {}", e);
                None
            }
        };

        // Fetch recent logs (last 7 days)
        let logs = match self.client.get_logs(&self.endeavor_id, 7) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Warning: failed to fetch OH logs: {}", e);
                Vec::new() // Continue with endeavor info even if logs fail
            }
        };

        // Format context
        let mut context = String::new();
        context.push_str("--- OH ENDEAVOR CONTEXT ---\n");
        context.push_str(&format!("ENDEAVOR: {} - {}\n", endeavor.id, endeavor.title));

        if let Some(desc) = &endeavor.description {
            if !desc.is_empty() {
                context.push_str(&format!("DESCRIPTION: {}\n", desc));
            }
        }

        if let Some(status) = &endeavor.status {
            context.push_str(&format!("STATUS: {}\n", status));
        }

        // Include guardrails (enforce these!)
        if let Some(ref ext) = extensions {
            if !ext.guardrails.is_empty() {
                context.push_str("\n--- ACTIVE GUARDRAILS (enforce these!) ---\n");

                // Group by severity
                let hard: Vec<_> = ext.guardrails.iter().filter(|g| g.severity == "hard").collect();
                let soft: Vec<_> = ext.guardrails.iter().filter(|g| g.severity == "soft").collect();
                let advisory: Vec<_> = ext.guardrails.iter().filter(|g| g.severity == "advisory").collect();

                if !hard.is_empty() {
                    context.push_str("\nHARD (BLOCK if violated - no override):\n");
                    for g in hard {
                        context.push_str(&format!("• {}\n", g.title));
                    }
                }

                if !soft.is_empty() {
                    context.push_str("\nSOFT (BLOCK unless override rationale provided):\n");
                    for g in soft {
                        context.push_str(&format!("• {}\n", g.title));
                    }
                }

                if !advisory.is_empty() {
                    context.push_str("\nADVISORY (WARN in feedback):\n");
                    for g in advisory {
                        context.push_str(&format!("• {}\n", g.title));
                    }
                }

                context.push_str("--- END GUARDRAILS ---\n");
            }

            // Include metis (situational wisdom)
            if !ext.metis.is_empty() {
                context.push_str("\n--- METIS (situational wisdom) ---\n");
                for m in ext.metis.iter().take(5) {
                    let freshness_indicator = match m.freshness.as_str() {
                        "recent" => "🟢",
                        "stale" => "🟡",
                        _ => "⚪",
                    };
                    // Truncate long content
                    let content = if m.content.chars().count() > 150 {
                        format!("{}...", m.content.chars().take(150).collect::<String>())
                    } else {
                        m.content.clone()
                    };
                    context.push_str(&format!("{} {}: {}\n", freshness_indicator, m.title, content));
                }
                context.push_str("--- END METIS ---\n");
            }
        }

        if !logs.is_empty() {
            context.push_str("\nRECENT LOGS:\n");
            for log in logs.iter().take(5) {
                // Truncate long content (use chars() to avoid UTF-8 panic on multi-byte)
                let content = if log.content.chars().count() > 200 {
                    format!("{}...", log.content.chars().take(200).collect::<String>())
                } else {
                    log.content.clone()
                };
                context.push_str(&format!("- [{}] {}\n", log.log_date, content));
            }
        }

        context.push_str("--- END OH CONTEXT ---\n\n");
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_missing() {
        // Clear env vars for test
        env::remove_var("OH_API_KEY");
        env::remove_var("OH_API_URL");

        assert!(OhConfig::from_env().is_none());
    }

    #[test]
    fn test_client_new_fails_when_not_configured() {
        env::remove_var("OH_API_KEY");
        env::remove_var("OH_API_URL");

        let result = OhClient::new();
        assert!(matches!(result, Err(OhError::NotConfigured)));
    }

    // Tests for parse_config_for_endeavor_id (no env var interference)

    #[test]
    fn test_parse_config_extracts_endeavor_id() {
        let content = "# Config\neval_interval_minutes: 5\noh_endeavor_id: my-endeavor-123\n";
        let result = parse_config_for_endeavor_id(content);
        assert_eq!(result, Some("my-endeavor-123".to_string()));
    }

    #[test]
    fn test_parse_config_strips_double_quotes() {
        let content = "oh_endeavor_id: \"quoted-value\"";
        let result = parse_config_for_endeavor_id(content);
        assert_eq!(result, Some("quoted-value".to_string()));
    }

    #[test]
    fn test_parse_config_strips_single_quotes() {
        let content = "oh_endeavor_id: 'single-quoted'";
        let result = parse_config_for_endeavor_id(content);
        assert_eq!(result, Some("single-quoted".to_string()));
    }

    #[test]
    fn test_parse_config_returns_none_when_missing() {
        let content = "eval_interval_minutes: 5\nmodel: opus";
        let result = parse_config_for_endeavor_id(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_config_returns_none_for_empty_value() {
        let content = "oh_endeavor_id: ";
        let result = parse_config_for_endeavor_id(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_config_handles_whitespace() {
        let content = "  oh_endeavor_id:   spaced-value  \n";
        let result = parse_config_for_endeavor_id(content);
        assert_eq!(result, Some("spaced-value".to_string()));
    }

    #[test]
    fn test_parse_endeavor_response() {
        let json = r#"{"endeavor":{"id":"test-123","title":"Test Endeavor","description":"A description","status":"active"}}"#;
        let response: GetEndeavorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.endeavor.id, "test-123");
        assert_eq!(response.endeavor.title, "Test Endeavor");
        assert_eq!(
            response.endeavor.description,
            Some("A description".to_string())
        );
        assert_eq!(response.endeavor.status, Some("active".to_string()));
    }

    #[test]
    fn test_parse_endeavor_response_minimal() {
        let json = r#"{"endeavor":{"id":"min-id","title":"Minimal"}}"#;
        let response: GetEndeavorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.endeavor.id, "min-id");
        assert_eq!(response.endeavor.title, "Minimal");
        assert!(response.endeavor.description.is_none());
        assert!(response.endeavor.status.is_none());
    }

    #[test]
    fn test_parse_logs_response() {
        let json = r#"{"logs":[{"content":"First log","log_date":"2025-12-20"},{"content":"Second log","log_date":"2025-12-19"}]}"#;
        let response: GetLogsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.logs.len(), 2);
        assert_eq!(response.logs[0].content, "First log");
        assert_eq!(response.logs[0].log_date, "2025-12-20");
        assert_eq!(response.logs[1].log_date, "2025-12-19");
    }

    #[test]
    fn test_parse_logs_response_empty() {
        let json = r#"{"logs":[]}"#;
        let response: GetLogsResponse = serde_json::from_str(json).unwrap();
        assert!(response.logs.is_empty());
    }
}

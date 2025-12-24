//! Retrospective visualization for superego sessions
//!
//! Generates HTML timeline visualizations of superego decisions.
//! Two modes:
//! - Default: Show all decisions with keyword-based severity/tags
//! - Curated: LLM picks key moments with generated summaries

use crate::claude::{self, ClaudeOptions};
use crate::decision::{Decision, DecisionType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Severity levels for timeline events
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Success,
    Info,
}

impl Severity {
    fn css_class(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Success => "success",
            Severity::Info => "info",
        }
    }
}

/// A moment in the timeline
#[derive(Debug, Clone, Serialize)]
pub struct Moment {
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub detail: String,
    pub severity: Severity,
    pub tag: String,
    /// Whether Claude accepted the feedback (curated mode only)
    pub accepted: Option<bool>,
    /// Claude's reaction/reasoning (curated mode only)
    pub reaction: Option<String>,
}

/// Session metadata for the report header
struct SessionMeta {
    session_id: String,
    date: String,
    decision_count: usize,
    /// Executive summary from LLM curation (empty for default mode)
    executive_summary: Option<String>,
}

/// Error type for retro operations
#[derive(Debug)]
pub enum RetroError {
    NoSessions,
    SessionNotFound(String),
    IoError(std::io::Error),
    DecisionError(String),
}

impl std::fmt::Display for RetroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetroError::NoSessions => write!(f, "No sessions found in .superego/sessions/"),
            RetroError::SessionNotFound(id) => write!(f, "Session not found: {}", id),
            RetroError::IoError(e) => write!(f, "IO error: {}", e),
            RetroError::DecisionError(e) => write!(f, "Decision error: {}", e),
        }
    }
}

impl From<std::io::Error> for RetroError {
    fn from(e: std::io::Error) -> Self {
        RetroError::IoError(e)
    }
}

/// Find the most recent session in .superego/sessions/
fn find_latest_session(superego_dir: &Path) -> Result<String, RetroError> {
    let sessions_dir = superego_dir.join("sessions");
    if !sessions_dir.exists() {
        return Err(RetroError::NoSessions);
    }

    let mut sessions: Vec<_> = fs::read_dir(&sessions_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    if sessions.is_empty() {
        return Err(RetroError::NoSessions);
    }

    // Sort by modification time (most recent first)
    sessions.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    sessions.reverse();

    sessions[0]
        .file_name()
        .to_str()
        .map(|s| s.to_string())
        .ok_or(RetroError::NoSessions)
}

/// Load decisions from a session directory
fn load_decisions(session_dir: &Path) -> Result<Vec<Decision>, RetroError> {
    let decisions_dir = session_dir.join("decisions");
    if !decisions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut decisions = Vec::new();

    for entry in fs::read_dir(&decisions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "json") {
            let content = fs::read_to_string(&path)?;
            match serde_json::from_str::<Decision>(&content) {
                Ok(decision) => decisions.push(decision),
                Err(e) => {
                    eprintln!("Warning: skipping {:?}: {}", path, e);
                }
            }
        }
    }

    decisions.sort_by_key(|d| d.timestamp);
    Ok(decisions)
}

/// Infer severity from decision context using keywords
fn infer_severity(context: &str) -> Severity {
    let lower = context.to_lowercase();

    if lower.contains("error")
        || lower.contains("critical")
        || lower.contains("violation")
        || lower.contains("must not")
        || lower.contains("block")
    {
        Severity::Error
    } else if lower.contains("warning")
        || lower.contains("concern")
        || lower.contains("should")
        || lower.contains("consider")
    {
        Severity::Warning
    } else if lower.contains("correct")
        || lower.contains("validated")
        || lower.contains("good")
        || lower.contains("allow")
    {
        Severity::Success
    } else {
        Severity::Info
    }
}

/// Infer tag from decision context using keywords
fn infer_tag(context: &str) -> String {
    let lower = context.to_lowercase();

    if lower.contains("protocol") || lower.contains("session close") {
        "Protocol".to_string()
    } else if lower.contains("intent") || lower.contains("x-y problem") || lower.contains("why") {
        "Intent Check".to_string()
    } else if lower.contains("scope")
        || lower.contains("over-engineer")
        || lower.contains("complexity")
    {
        "Scope Alert".to_string()
    } else if lower.contains("plan mode") || lower.contains("exitplanmode") {
        "Plan Mode".to_string()
    } else if lower.contains("pattern") || lower.contains("repeating") {
        "Pattern".to_string()
    } else if lower.contains("workflow") || lower.contains("todowrite") {
        "Workflow".to_string()
    } else if lower.contains("compilation") || lower.contains("error") {
        "Technical".to_string()
    } else {
        "Feedback".to_string()
    }
}

/// Extract title from context (first sentence, truncated)
fn extract_title(context: &str) -> String {
    // Get first line or sentence
    let first_line = context
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(context);

    // Take first sentence or truncate
    let title = if let Some(idx) = first_line.find(". ") {
        &first_line[..idx]
    } else if let Some(idx) = first_line.find(".\n") {
        &first_line[..idx]
    } else {
        first_line
    };

    // Clean up markdown
    let title = title
        .trim_start_matches('#')
        .trim_start_matches('*')
        .trim_start_matches(' ')
        .trim();

    // Truncate if too long
    if title.len() > 60 {
        format!("{}...", &title[..57])
    } else {
        title.to_string()
    }
}

/// Extract summary from context (second sentence or line)
fn extract_summary(context: &str) -> String {
    let lines: Vec<&str> = context.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.len() > 1 {
        let summary = lines[1]
            .trim_start_matches('#')
            .trim_start_matches('*')
            .trim_start_matches('-')
            .trim();

        if summary.len() > 100 {
            format!("{}...", &summary[..97])
        } else {
            summary.to_string()
        }
    } else {
        "Click to expand details".to_string()
    }
}

/// Convert decisions to moments (default mode - no LLM)
fn decisions_to_moments(decisions: Vec<Decision>) -> Vec<Moment> {
    decisions
        .into_iter()
        .filter(|d| d.decision_type == DecisionType::FeedbackDelivered)
        .filter_map(|d| {
            let context = d.context.as_ref()?;

            Some(Moment {
                timestamp: d.timestamp,
                title: extract_title(context),
                summary: extract_summary(context),
                detail: context.clone(),
                severity: infer_severity(context),
                tag: infer_tag(context),
                accepted: None, // Not available in default mode
                reaction: None,
            })
        })
        .collect()
}

/// Extract JSON object from text that might have surrounding content
fn extract_json(text: &str) -> Option<&str> {
    // Find first { and last }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start < end {
        Some(&text[start..=end])
    } else {
        None
    }
}

/// LLM response format for curated moments
#[derive(Debug, Deserialize)]
struct CuratedResponse {
    /// Short narrative theme of the session (e.g., "LP Speedtest Implementation")
    executive_summary: String,
    moments: Vec<CuratedMoment>,
}

#[derive(Debug, Deserialize)]
struct CuratedMoment {
    timestamp: String,
    title: String,
    summary: String,
    severity: String,
    tag: String,
    /// Whether Claude accepted/incorporated the feedback (null if unclear)
    #[serde(default)]
    accepted: Option<bool>,
    /// Brief description of Claude's reaction/reasoning
    #[serde(default)]
    reaction: Option<String>,
}

/// Result of LLM curation including executive summary
pub struct CurationResult {
    pub executive_summary: String,
    pub moments: Vec<Moment>,
}

// === OH Integration Payload ===

/// Statistics for the retrospective
#[derive(Debug, Serialize)]
pub struct RetrospectiveStats {
    /// Total number of decisions in session
    pub total_decisions: usize,
    /// Number of curated moments shown
    pub curated_count: usize,
    /// Number of moments where Claude accepted feedback
    pub accepted_count: usize,
    /// Number of moments where Claude dismissed feedback
    pub dismissed_count: usize,
}

/// Metadata payload for OH log entry
#[derive(Debug, Serialize)]
pub struct RetrospectiveMetadata {
    #[serde(rename = "type")]
    pub payload_type: String,
    pub version: u8,
    pub session_id: String,
    pub executive_summary: String,
    pub stats: RetrospectiveStats,
    pub moments: Vec<Moment>,
}

/// Full OH log payload
#[derive(Debug, Serialize)]
pub struct RetrospectivePayload {
    pub entity_type: String,
    pub entity_id: String,
    pub content: String,
    pub content_type: String,
    pub log_date: String,
    pub metadata: RetrospectiveMetadata,
}

/// Format retrospective data as OH log payload
pub fn format_oh_payload(
    session_id: &str,
    endeavor_id: &str,
    total_decisions: usize,
    result: &CurationResult,
) -> RetrospectivePayload {
    // Count acceptance stats
    let accepted_count = result
        .moments
        .iter()
        .filter(|m| m.accepted == Some(true))
        .count();
    let dismissed_count = result
        .moments
        .iter()
        .filter(|m| m.accepted == Some(false))
        .count();

    // Generate markdown content for the log
    let content = format!(
        "## Superego Retrospective\n\n**Theme:** {}\n\n**Key Moments:** {} (from {} total decisions)\n\n{}",
        result.executive_summary,
        result.moments.len(),
        total_decisions,
        result
            .moments
            .iter()
            .take(5) // Just summaries in markdown
            .map(|m| format!("- **{}**: {}", m.title, m.summary))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    RetrospectivePayload {
        entity_type: "endeavor".to_string(),
        entity_id: endeavor_id.to_string(),
        content,
        content_type: "markdown".to_string(),
        log_date: today,
        metadata: RetrospectiveMetadata {
            payload_type: "superego_retrospective".to_string(),
            version: 1,
            session_id: session_id.to_string(),
            executive_summary: result.executive_summary.clone(),
            stats: RetrospectiveStats {
                total_decisions,
                curated_count: result.moments.len(),
                accepted_count,
                dismissed_count,
            },
            moments: result.moments.clone(),
        },
    }
}

/// Curate moments using LLM (picks key moments, generates summaries)
fn curate_moments(decisions: Vec<Decision>) -> Result<CurationResult, RetroError> {
    // Filter to feedback decisions and format for LLM
    let feedback_decisions: Vec<_> = decisions
        .iter()
        .filter(|d| d.decision_type == DecisionType::FeedbackDelivered)
        .collect();

    if feedback_decisions.is_empty() {
        return Ok(CurationResult {
            executive_summary: String::new(),
            moments: Vec::new(),
        });
    }

    // Format decisions as context
    let mut context = String::new();
    for d in &feedback_decisions {
        if let Some(ctx) = &d.context {
            context.push_str(&format!(
                "---\nTimestamp: {}\nContent:\n{}\n\n",
                d.timestamp.to_rfc3339(),
                ctx
            ));
        }
    }

    let system_prompt = r#"You are analyzing superego feedback decisions to create a retrospective timeline.

Your task: Select 5-20 of the MOST significant moments that tell a compelling narrative arc.

Output JSON in this exact format:
{
  "executive_summary": "Short theme description (3-8 words, e.g., 'LP Speedtest Implementation', 'Authentication Refactor Gone Wrong')",
  "moments": [
    {
      "timestamp": "2025-12-22T16:10:09Z",
      "title": "X-Y Problem Detected",
      "summary": "Claude searching for branches without establishing the actual need",
      "severity": "warning",
      "tag": "Intent Check",
      "accepted": true,
      "reaction": "Claude acknowledged the issue and asked clarifying questions before proceeding"
    }
  ]
}

Rules:
- Select ONLY 5-20 moments (never more than 20, never fewer than 5)
- Choose moments that tell a compelling narrative arc with clear progression
- executive_summary: 3-8 word theme capturing what the session was about
- severity must be: "error", "warning", "success", or "info"
- title: 3-6 words, action-oriented
- summary: 1 sentence, ~15 words max
- tag: short category like "Protocol", "Intent Check", "Scope Alert", "Pattern", "Technical"
- accepted: true if Claude incorporated the feedback, false if dismissed/ignored, null if unclear
- reaction: 1 sentence describing how Claude responded (e.g., "Stopped and asked for clarification", "Acknowledged but repeated the pattern", "Course-corrected immediately")
- Focus on: intent issues, protocol violations, scope creep, course corrections, key discoveries
- Skip routine/minor feedback, keep only pivotal moments
- Use exact timestamps from the input
- Output ONLY the JSON, no other text"#;

    let message = format!(
        "Analyze these superego decisions and select the key moments:\n\n{}",
        context
    );

    eprintln!("Calling LLM to curate moments...");

    let options = ClaudeOptions {
        model: Some("haiku".to_string()), // Fast and cheap for this task
        no_session_persistence: true,
        ..Default::default()
    };

    let response = claude::invoke(system_prompt, &message, options)
        .map_err(|e| RetroError::DecisionError(format!("LLM call failed: {}", e)))?;

    // Extract JSON from response (LLM might add text before/after)
    let json_str = extract_json(&response.result)
        .ok_or_else(|| RetroError::DecisionError("No JSON found in LLM response".to_string()))?;

    // Parse JSON from response
    let curated: CuratedResponse = serde_json::from_str(json_str)
        .map_err(|e| RetroError::DecisionError(format!("Failed to parse LLM response: {}", e)))?;

    // Convert to Moments, matching timestamps to original decisions for full context
    let moments: Vec<Moment> = curated
        .moments
        .into_iter()
        .map(|cm| {
            // Find matching decision by timestamp
            let matching_decision = feedback_decisions
                .iter()
                .find(|d| d.timestamp.to_rfc3339().starts_with(&cm.timestamp[..19]));

            let detail = matching_decision
                .and_then(|d| d.context.clone())
                .unwrap_or_else(|| cm.summary.clone());

            // Parse timestamp
            let timestamp = matching_decision
                .map(|d| d.timestamp)
                .unwrap_or_else(Utc::now);

            let severity = match cm.severity.to_lowercase().as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                "success" => Severity::Success,
                _ => Severity::Info,
            };

            Moment {
                timestamp,
                title: cm.title,
                summary: cm.summary,
                detail,
                severity,
                tag: cm.tag,
                accepted: cm.accepted,
                reaction: cm.reaction,
            }
        })
        .collect();

    eprintln!("LLM selected {} key moments", moments.len());
    Ok(CurationResult {
        executive_summary: curated.executive_summary,
        moments,
    })
}

/// HTML template with placeholders
const HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Superego Session Retrospective</title>
  <style>
    :root {
      --bg: #0d1117;
      --surface: #161b22;
      --border: #30363d;
      --text: #c9d1d9;
      --text-muted: #8b949e;
      --accent: #58a6ff;
      --warning: #d29922;
      --error: #f85149;
      --success: #3fb950;
    }
    * { box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: var(--bg);
      color: var(--text);
      margin: 0;
      padding: 2rem;
      line-height: 1.6;
    }
    .container { max-width: 1000px; margin: 0 auto; }
    header { text-align: center; margin-bottom: 3rem; }
    h1 { font-size: 2rem; font-weight: 600; margin-bottom: 0.5rem; }
    .subtitle { color: var(--text-muted); font-size: 1rem; }
    .stats { display: flex; justify-content: center; gap: 2rem; margin-top: 1.5rem; }
    .stat { text-align: center; }
    .stat-value { font-size: 1.5rem; font-weight: 600; color: var(--accent); }
    .stat-label { font-size: 0.8rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
    .timeline { position: relative; padding-left: 2rem; }
    .timeline::before {
      content: '';
      position: absolute;
      left: 0.5rem;
      top: 0;
      bottom: 0;
      width: 2px;
      background: var(--border);
    }
    .event {
      position: relative;
      margin-bottom: 2rem;
      padding: 1.25rem;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      cursor: pointer;
      transition: all 0.2s ease;
    }
    .event:hover {
      border-color: var(--accent);
      transform: translateX(4px);
    }
    .event::before {
      content: '';
      position: absolute;
      left: -1.75rem;
      top: 1.5rem;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--border);
      border: 2px solid var(--bg);
    }
    .event.warning::before { background: var(--warning); }
    .event.error::before { background: var(--error); }
    .event.success::before { background: var(--success); }
    .event.info::before { background: var(--accent); }
    .event-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      margin-bottom: 0.5rem;
    }
    .event-time { font-size: 0.75rem; color: var(--text-muted); font-family: monospace; }
    .event-tag {
      font-size: 0.7rem;
      padding: 0.2rem 0.5rem;
      border-radius: 4px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      font-weight: 600;
    }
    .event.warning .event-tag { background: rgba(210, 153, 34, 0.2); color: var(--warning); }
    .event.error .event-tag { background: rgba(248, 81, 73, 0.2); color: var(--error); }
    .event.success .event-tag { background: rgba(63, 185, 80, 0.2); color: var(--success); }
    .event.info .event-tag { background: rgba(88, 166, 255, 0.2); color: var(--accent); }
    .event-title { font-size: 1rem; font-weight: 600; margin-bottom: 0.5rem; }
    .event-summary { font-size: 0.9rem; color: var(--text-muted); }
    .event-detail {
      display: none;
      margin-top: 1rem;
      padding-top: 1rem;
      border-top: 1px solid var(--border);
      font-size: 0.85rem;
      white-space: pre-wrap;
    }
    .event.expanded .event-detail { display: block; }
    .event-detail h4 { color: var(--accent); margin: 1rem 0 0.5rem; font-size: 0.9rem; }
    .event-detail h4:first-child { margin-top: 0; }
    .event-detail code { background: var(--bg); padding: 0.2rem 0.4rem; border-radius: 3px; font-size: 0.8rem; }
    .event-detail pre { background: var(--bg); padding: 1rem; border-radius: 6px; overflow-x: auto; font-size: 0.8rem; }
    .reaction { margin: 0.75rem 0; padding: 0.5rem 0.75rem; border-radius: 4px; font-size: 0.85rem; font-style: italic; border-left: 3px solid var(--border); background: var(--surface); }
    .reaction.accepted { border-left-color: var(--success); color: var(--success); }
    .reaction.dismissed { border-left-color: var(--error); color: var(--error); }
    .reaction.unclear { border-left-color: var(--warning); color: var(--warning); }
    .reaction-icon { font-weight: bold; margin-right: 0.5rem; font-style: normal; }
    footer { margin-top: 3rem; text-align: center; color: var(--text-muted); font-size: 0.8rem; }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <h1>Superego Session Retrospective</h1>
      <p class="subtitle">{{SUBTITLE}}</p>
      <div class="stats">
        <div class="stat">
          <div class="stat-value">{{DECISION_COUNT}}</div>
          <div class="stat-label">Decisions</div>
        </div>
      </div>
    </header>
    <div class="timeline">
{{EVENTS}}
    </div>
    <footer>
      Generated by <code>sg retro</code> • Superego
    </footer>
  </div>
  <script>
    document.querySelectorAll('.event').forEach(el => {
      el.addEventListener('click', () => el.classList.toggle('expanded'));
    });
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        document.querySelectorAll('.event.expanded').forEach(el => el.classList.remove('expanded'));
      }
    });
  </script>
</body>
</html>"#;

/// Escape HTML special characters
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Generate HTML for a single event
fn generate_event_html(moment: &Moment) -> String {
    let time = moment.timestamp.format("%H:%M").to_string();
    let severity_class = moment.severity.css_class();

    // Generate reaction HTML if available (curated mode only)
    let reaction_html = moment
        .reaction
        .as_ref()
        .map(|r| {
            let (icon, status_class) = match moment.accepted {
                Some(true) => ("✓", "accepted"),
                Some(false) => ("✗", "dismissed"),
                None => ("?", "unclear"),
            };
            format!(
                r#"        <div class="reaction {}"><span class="reaction-icon">{}</span> {}</div>
"#,
                status_class,
                icon,
                escape_html(r)
            )
        })
        .unwrap_or_default();

    format!(
        r#"      <div class="event {}">
        <div class="event-header">
          <span class="event-time">{}</span>
          <span class="event-tag">{}</span>
        </div>
        <div class="event-title">{}</div>
        <div class="event-summary">{}</div>
{}        <div class="event-detail">{}</div>
      </div>
"#,
        severity_class,
        time,
        escape_html(&moment.tag),
        escape_html(&moment.title),
        escape_html(&moment.summary),
        reaction_html,
        escape_html(&moment.detail)
    )
}

/// Generate the full HTML report
fn generate_html(moments: Vec<Moment>, meta: SessionMeta) -> String {
    let events_html: String = moments.iter().map(generate_event_html).collect();

    // Include executive summary in subtitle if present
    let subtitle = match &meta.executive_summary {
        Some(summary) if !summary.is_empty() => {
            format!(
                "Session {} • {} • {}",
                &meta.session_id[..8],
                meta.date,
                summary
            )
        }
        _ => format!("Session {} • {}", &meta.session_id[..8], meta.date),
    };

    HTML_TEMPLATE
        .replace("{{SUBTITLE}}", &subtitle)
        .replace("{{DECISION_COUNT}}", &meta.decision_count.to_string())
        .replace("{{EVENTS}}", &events_html)
}

/// Open file in default browser
fn open_browser(path: &Path) -> Result<(), RetroError> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn()?;
    }
    Ok(())
}

/// Main entry point for the retro command
pub fn run(
    superego_dir: &Path,
    session_id: Option<&str>,
    curated: bool,
    output: &Path,
    open: bool,
    push_oh: bool,
) -> Result<(), RetroError> {
    // Find session
    let session_id = match session_id {
        Some(id) => id.to_string(),
        None => {
            let id = find_latest_session(superego_dir)?;
            eprintln!("Using latest session: {}", id);
            id
        }
    };

    let session_dir = superego_dir.join("sessions").join(&session_id);
    if !session_dir.exists() {
        return Err(RetroError::SessionNotFound(session_id));
    }

    // Load decisions
    let decisions = load_decisions(&session_dir)?;
    if decisions.is_empty() {
        eprintln!("No decisions found in session.");
        return Ok(());
    }

    let total_decisions = decisions.len();
    eprintln!("Found {} decisions", total_decisions);

    // Get date from first decision
    let date = decisions
        .first()
        .map(|d| d.timestamp.format("%b %d, %Y").to_string())
        .unwrap_or_default();

    // Determine processing mode - curate if either flag is set
    let need_curation = curated || push_oh;

    // Process decisions (moves ownership into one path, no cloning)
    let (moments, executive_summary, curation_for_oh) = if need_curation {
        let result = curate_moments(decisions)?;
        let summary = result.executive_summary.clone();
        let moments = result.moments.clone();
        (moments, Some(summary), Some(result))
    } else {
        (decisions_to_moments(decisions), None, None)
    };

    if moments.is_empty() {
        eprintln!("No feedback decisions to display.");
        return Ok(());
    }

    eprintln!("Generated {} timeline events", moments.len());

    let meta = SessionMeta {
        session_id: session_id.clone(),
        date,
        decision_count: moments.len(),
        executive_summary,
    };

    // Generate HTML
    let html = generate_html(moments, meta);

    // Write to file
    fs::write(output, &html)?;
    eprintln!("Written to: {}", output.display());

    // Open in browser if requested
    if open {
        open_browser(output)?;
    }

    // Push to Open Horizons if requested
    if push_oh {
        if let Some(ref result) = curation_for_oh {
            push_to_oh(superego_dir, &session_id, total_decisions, result)?;
        }
    }

    Ok(())
}

/// Push retrospective data to Open Horizons
fn push_to_oh(
    superego_dir: &Path,
    session_id: &str,
    total_decisions: usize,
    result: &CurationResult,
) -> Result<(), RetroError> {
    use crate::oh::{get_endeavor_id, OhClient};

    // Get OH configuration
    let endeavor_id = match get_endeavor_id(superego_dir) {
        Some(id) => id,
        None => {
            eprintln!("OH push skipped: no oh_endeavor_id configured in .superego/config.yaml");
            return Ok(());
        }
    };

    let client = match OhClient::from_config(superego_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "OH push skipped: {} (set oh_api_key in config.yaml or OH_API_KEY env var)",
                e
            );
            return Ok(());
        }
    };

    // Format payload
    let payload = format_oh_payload(session_id, &endeavor_id, total_decisions, result);

    // Push to OH
    eprintln!("Pushing retrospective to OH endeavor: {}", endeavor_id);
    match client.log_retrospective(&payload) {
        Ok(log_id) => {
            eprintln!("Successfully pushed to OH (log_id: {})", log_id);
        }
        Err(e) => {
            eprintln!("Failed to push to OH: {}", e);
            // Don't fail the command, just warn
        }
    }

    Ok(())
}

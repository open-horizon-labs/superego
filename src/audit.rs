//! Audit module for analyzing superego decision history
//!
//! Provides statistics and LLM-based analysis of decisions.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;

use crate::claude::{self, ClaudeError, ClaudeOptions};
use crate::decision::Decision;

/// Statistics about decisions
#[derive(Debug, Clone, Serialize)]
pub struct AuditStats {
    pub total: usize,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub session_count: usize,
}

/// Full audit result with stats and analysis
#[derive(Debug, Clone, Serialize)]
pub struct AuditResult {
    pub stats: AuditStats,
    pub analysis: String,
}

/// Calculate statistics from decisions
pub fn calculate_stats(decisions: &[Decision]) -> AuditStats {
    if decisions.is_empty() {
        return AuditStats {
            total: 0,
            start_date: None,
            end_date: None,
            session_count: 0,
        };
    }

    // Count unique sessions
    let sessions: HashSet<_> = decisions
        .iter()
        .filter_map(|d| d.session_id.as_ref())
        .collect();

    // Decisions are already sorted by timestamp
    AuditStats {
        total: decisions.len(),
        start_date: decisions.first().map(|d| d.timestamp),
        end_date: decisions.last().map(|d| d.timestamp),
        session_count: sessions.len(),
    }
}

/// Build the prompt for Claude to analyze decisions
fn build_audit_prompt(decisions: &[Decision]) -> String {
    let mut prompt = String::from(
        "You are analyzing superego's decision history for a project.\n\n\
         Superego is a metacognitive advisor that monitors Claude Code sessions \
         and provides feedback when it detects potential issues.\n\n\
         Below are all recorded decisions (feedback given to Claude Code):\n\n",
    );

    for (i, decision) in decisions.iter().enumerate() {
        prompt.push_str(&format!("--- Decision {} ---\n", i + 1));
        prompt.push_str(&format!(
            "Timestamp: {}\n",
            decision.timestamp.format("%Y-%m-%d %H:%M UTC")
        ));

        if let Some(session) = &decision.session_id {
            // Truncate session ID for readability
            let short_session = if session.len() > 8 {
                &session[..8]
            } else {
                session
            };
            prompt.push_str(&format!("Session: {}...\n", short_session));
        } else {
            prompt.push_str("Session: (unknown)\n");
        }

        if let Some(context) = &decision.context {
            prompt.push_str(&format!("Feedback: {}\n", context));
        }
        prompt.push('\n');
    }

    prompt.push_str(
        "---\n\n\
         Provide a concise analysis covering:\n\n\
         1. **Patterns & Themes**: What kinds of concerns came up repeatedly? \
         Any behavioral patterns you notice?\n\n\
         2. **Timeline**: Brief chronological narrative of significant events.\n\n\
         3. **Actionable Insights**: Based on this history, what should the \
         developer focus on improving?\n\n\
         Keep the analysis concise and actionable. Use markdown formatting.",
    );

    prompt
}

/// Analyze decisions using Claude LLM
pub fn analyze_decisions(decisions: &[Decision]) -> Result<String, ClaudeError> {
    if decisions.is_empty() {
        return Ok("No decisions to analyze.".to_string());
    }

    let prompt = build_audit_prompt(decisions);

    let options = ClaudeOptions {
        model: None,
        no_session_persistence: true,
        ..Default::default()
    };

    let system_prompt = "You are a code review analyst. Analyze the provided decision history \
                         and provide actionable insights. Be concise and direct.";

    let response = claude::invoke(system_prompt, &prompt, options)?;
    Ok(response.result)
}

/// Run full audit: calculate stats and analyze with LLM
pub fn run_audit(decisions: &[Decision]) -> Result<AuditResult, ClaudeError> {
    let stats = calculate_stats(decisions);
    let analysis = analyze_decisions(decisions)?;

    Ok(AuditResult { stats, analysis })
}

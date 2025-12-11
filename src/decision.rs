/// Decision journal for superego
///
/// Stores phase transitions, overrides, and acknowledgments as YAML files
/// in .superego/decisions/ for audit trail and context recovery.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Phase states for the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Exploring,
    Discussing,
    Ready,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::Exploring => write!(f, "exploring"),
            Phase::Discussing => write!(f, "discussing"),
            Phase::Ready => write!(f, "ready"),
        }
    }
}

/// Types of decisions that can be recorded
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    PhaseTransition,
    OverrideGranted,
    FeedbackAccepted,
    PrecompactSnapshot,
}

/// A decision record stored in the journal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub decision_type: DecisionType,
    pub from_state: Option<Phase>,
    pub to_state: Option<Phase>,
    pub context: Option<String>,
    pub trigger: Option<String>,
    pub approved_scope: Option<String>,
}

impl Decision {
    /// Create a new phase transition decision
    pub fn phase_transition(
        session_id: Option<String>,
        from: Phase,
        to: Phase,
        trigger: String,
        approved_scope: Option<String>,
    ) -> Self {
        Decision {
            timestamp: Utc::now(),
            session_id,
            decision_type: DecisionType::PhaseTransition,
            from_state: Some(from),
            to_state: Some(to),
            context: None,
            trigger: Some(trigger),
            approved_scope,
        }
    }

    /// Create an override granted decision
    pub fn override_granted(session_id: Option<String>, reason: String) -> Self {
        Decision {
            timestamp: Utc::now(),
            session_id,
            decision_type: DecisionType::OverrideGranted,
            from_state: None,
            to_state: None,
            context: Some(reason),
            trigger: None,
            approved_scope: None,
        }
    }

    /// Create a feedback accepted decision
    pub fn feedback_accepted(session_id: Option<String>) -> Self {
        Decision {
            timestamp: Utc::now(),
            session_id,
            decision_type: DecisionType::FeedbackAccepted,
            from_state: None,
            to_state: None,
            context: None,
            trigger: None,
            approved_scope: None,
        }
    }

    /// Create a precompact snapshot decision
    pub fn precompact_snapshot(session_id: Option<String>, context: String) -> Self {
        Decision {
            timestamp: Utc::now(),
            session_id,
            decision_type: DecisionType::PrecompactSnapshot,
            from_state: None,
            to_state: None,
            context: Some(context),
            trigger: None,
            approved_scope: None,
        }
    }
}

/// Error type for decision journal operations
#[derive(Debug)]
pub enum JournalError {
    IoError(std::io::Error),
    SerdeError(serde_yaml::Error),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::IoError(e) => write!(f, "IO error: {}", e),
            JournalError::SerdeError(e) => write!(f, "YAML error: {}", e),
        }
    }
}

impl std::error::Error for JournalError {}

impl From<std::io::Error> for JournalError {
    fn from(e: std::io::Error) -> Self {
        JournalError::IoError(e)
    }
}

impl From<serde_yaml::Error> for JournalError {
    fn from(e: serde_yaml::Error) -> Self {
        JournalError::SerdeError(e)
    }
}

/// Decision journal - manages reading and writing decision records
pub struct Journal {
    decisions_dir: PathBuf,
}

impl Journal {
    /// Create a new journal for the given .superego directory
    pub fn new(superego_dir: &Path) -> Self {
        Journal {
            decisions_dir: superego_dir.join("decisions"),
        }
    }

    /// Ensure the decisions directory exists
    pub fn ensure_dir(&self) -> Result<(), JournalError> {
        fs::create_dir_all(&self.decisions_dir)?;
        Ok(())
    }

    /// Write a decision to the journal
    pub fn write(&self, decision: &Decision) -> Result<PathBuf, JournalError> {
        self.ensure_dir()?;

        // Format timestamp for filename: 2024-01-15T10-30-00Z.yaml
        let filename = decision
            .timestamp
            .format("%Y-%m-%dT%H-%M-%SZ.yaml")
            .to_string();
        let path = self.decisions_dir.join(&filename);

        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);
        let yaml = serde_yaml::to_string(decision)?;
        writer.write_all(yaml.as_bytes())?;

        Ok(path)
    }

    /// Read all decisions from the journal, sorted by timestamp
    pub fn read_all(&self) -> Result<Vec<Decision>, JournalError> {
        if !self.decisions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut decisions = Vec::new();

        for entry in fs::read_dir(&self.decisions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "yaml") {
                let content = fs::read_to_string(&path)?;
                match serde_yaml::from_str::<Decision>(&content) {
                    Ok(decision) => decisions.push(decision),
                    Err(e) => {
                        // AIDEV-NOTE: Skip malformed files rather than failing
                        eprintln!("Warning: skipping malformed decision file {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by timestamp (oldest first)
        decisions.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(decisions)
    }

    /// Read the most recent N decisions
    pub fn read_recent(&self, limit: usize) -> Result<Vec<Decision>, JournalError> {
        let mut all = self.read_all()?;
        let start = all.len().saturating_sub(limit);
        Ok(all.drain(start..).collect())
    }

    /// Get the most recent phase transition
    pub fn last_phase_transition(&self) -> Result<Option<Decision>, JournalError> {
        let all = self.read_all()?;
        Ok(all
            .into_iter()
            .rev()
            .find(|d| d.decision_type == DecisionType::PhaseTransition))
    }

    /// Get the current phase based on the last transition (defaults to Exploring)
    pub fn current_phase(&self) -> Result<Phase, JournalError> {
        match self.last_phase_transition()? {
            Some(d) => Ok(d.to_state.unwrap_or(Phase::Exploring)),
            None => Ok(Phase::Exploring),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_read_decision() {
        let dir = tempdir().unwrap();
        let journal = Journal::new(dir.path());

        let decision = Decision::phase_transition(
            Some("sess-123".to_string()),
            Phase::Discussing,
            Phase::Ready,
            "user said go ahead".to_string(),
            Some("implement auth".to_string()),
        );

        journal.write(&decision).unwrap();

        let read_back = journal.read_all().unwrap();
        assert_eq!(read_back.len(), 1);
        assert_eq!(read_back[0].decision_type, DecisionType::PhaseTransition);
        assert_eq!(read_back[0].to_state, Some(Phase::Ready));
    }

    #[test]
    fn test_current_phase_default() {
        let dir = tempdir().unwrap();
        let journal = Journal::new(dir.path());

        // No decisions - should default to exploring
        assert_eq!(journal.current_phase().unwrap(), Phase::Exploring);
    }

    #[test]
    fn test_current_phase_after_transition() {
        let dir = tempdir().unwrap();
        let journal = Journal::new(dir.path());

        let decision = Decision::phase_transition(
            None,
            Phase::Exploring,
            Phase::Ready,
            "confirmed".to_string(),
            None,
        );
        journal.write(&decision).unwrap();

        assert_eq!(journal.current_phase().unwrap(), Phase::Ready);
    }

    #[test]
    fn test_override_decision() {
        let dir = tempdir().unwrap();
        let journal = Journal::new(dir.path());

        let decision = Decision::override_granted(Some("sess-1".to_string()), "user approved".to_string());
        journal.write(&decision).unwrap();

        let read_back = journal.read_all().unwrap();
        assert_eq!(read_back[0].decision_type, DecisionType::OverrideGranted);
        assert_eq!(read_back[0].context, Some("user approved".to_string()));
    }
}

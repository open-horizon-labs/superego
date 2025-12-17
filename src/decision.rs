//! Decision journal for superego
//!
//! Stores feedback and snapshots as JSON files in .superego/decisions/
//! for audit trail and context recovery.
//! AIDEV-NOTE: Simplified - constructor methods removed, just read existing files.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Types of decisions that can be recorded
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    OverrideGranted,
    FeedbackDelivered,
    PrecompactSnapshot,
}

/// A decision record stored in the journal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub decision_type: DecisionType,
    pub context: Option<String>,
    pub trigger: Option<String>,
}

impl Decision {
    /// Create a feedback delivered decision for audit trail
    pub fn feedback_delivered(session_id: Option<String>, feedback: String) -> Self {
        Decision {
            timestamp: Utc::now(),
            session_id,
            decision_type: DecisionType::FeedbackDelivered,
            context: Some(feedback),
            trigger: None,
        }
    }
}

/// Error type for decision journal operations
#[derive(Debug)]
pub enum JournalError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::IoError(e) => write!(f, "IO error: {}", e),
            JournalError::JsonError(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for JournalError {}

impl From<std::io::Error> for JournalError {
    fn from(e: std::io::Error) -> Self {
        JournalError::IoError(e)
    }
}

impl From<serde_json::Error> for JournalError {
    fn from(e: serde_json::Error) -> Self {
        JournalError::JsonError(e)
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
    fn ensure_dir(&self) -> Result<(), JournalError> {
        fs::create_dir_all(&self.decisions_dir)?;
        Ok(())
    }

    /// Write a decision to the journal
    pub fn write(&self, decision: &Decision) -> Result<PathBuf, JournalError> {
        self.ensure_dir()?;

        // Format timestamp for filename: 2024-01-15T10-30-00Z.json
        let filename = decision
            .timestamp
            .format("%Y-%m-%dT%H-%M-%SZ.json")
            .to_string();
        let path = self.decisions_dir.join(&filename);

        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string_pretty(decision)?;
        writer.write_all(json.as_bytes())?;

        Ok(path)
    }

    /// Read all decisions from the journal, sorted by timestamp
    /// AIDEV-NOTE: Only reads .json files. Old .yaml files from pre-0.4 are ignored
    /// (decision journal is audit data, not critical state).
    pub fn read_all(&self) -> Result<Vec<Decision>, JournalError> {
        if !self.decisions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut decisions = Vec::new();

        for entry in fs::read_dir(&self.decisions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)?;
                match serde_json::from_str::<Decision>(&content) {
                    Ok(decision) => decisions.push(decision),
                    Err(e) => {
                        // AIDEV-NOTE: Skip malformed files rather than failing
                        eprintln!(
                            "Warning: skipping malformed decision file {:?}: {}",
                            path, e
                        );
                    }
                }
            }
        }

        // Sort by timestamp (oldest first)
        decisions.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(decisions)
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

        let decision = Decision {
            timestamp: Utc::now(),
            session_id: Some("sess-123".to_string()),
            decision_type: DecisionType::FeedbackDelivered,
            context: Some("test feedback".to_string()),
            trigger: None,
        };

        journal.write(&decision).unwrap();

        let read_back = journal.read_all().unwrap();
        assert_eq!(read_back.len(), 1);
        assert_eq!(read_back[0].decision_type, DecisionType::FeedbackDelivered);
    }
}

//! State management for superego
//!
//! Maintains disabled flag in .superego/state.json
//! AIDEV-NOTE: Simplified - removed override mechanism.
//! Task state comes from bd, disabled flag is for user control.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Current superego state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub last_evaluated: Option<DateTime<Utc>>,
    #[serde(default)]
    pub disabled: bool,
}

impl State {
    /// Mark as evaluated up to a specific timestamp
    /// AIDEV-NOTE: Use the transcript read timestamp, NOT Utc::now() at
    /// completion time. This prevents skipping messages written during LLM eval.
    pub fn mark_evaluated_at(&mut self, timestamp: DateTime<Utc>) {
        self.last_evaluated = Some(timestamp);
    }
}

/// Error type for state operations
#[derive(Debug)]
pub enum StateError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::IoError(e) => write!(f, "IO error: {}", e),
            StateError::JsonError(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for StateError {}

impl From<std::io::Error> for StateError {
    fn from(e: std::io::Error) -> Self {
        StateError::IoError(e)
    }
}

impl From<serde_json::Error> for StateError {
    fn from(e: serde_json::Error) -> Self {
        StateError::JsonError(e)
    }
}

/// State manager - reads and writes .superego/state.json
pub struct StateManager {
    state_path: PathBuf,
}

impl StateManager {
    /// Create a new state manager for the given .superego directory
    pub fn new(superego_dir: &Path) -> Self {
        StateManager {
            state_path: superego_dir.join("state.json"),
        }
    }

    /// Load state from disk (returns default if file doesn't exist)
    pub fn load(&self) -> Result<State, StateError> {
        if !self.state_path.exists() {
            return Ok(State::default());
        }

        let file = File::open(&self.state_path)?;
        let reader = BufReader::new(file);
        let state = serde_json::from_reader(reader)?;
        Ok(state)
    }

    /// Save state to disk
    pub fn save(&self, state: &State) -> Result<(), StateError> {
        // Ensure parent directory exists
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = File::create(&self.state_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, state)?;
        Ok(())
    }

    /// Load, modify, and save state atomically
    pub fn update<F>(&self, f: F) -> Result<State, StateError>
    where
        F: FnOnce(&mut State),
    {
        let mut state = self.load()?;
        f(&mut state);
        self.save(&state)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_state() {
        let state = State::default();
        assert!(!state.disabled);
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path());

        let state = State {
            disabled: true,
            ..Default::default()
        };

        manager.save(&state).unwrap();

        let loaded = manager.load().unwrap();
        assert!(loaded.disabled);
    }

    #[test]
    fn test_load_missing_returns_default() {
        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path());

        let state = manager.load().unwrap();
        assert!(!state.disabled);
    }

    #[test]
    fn test_update() {
        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path());

        manager
            .update(|s| {
                s.disabled = true;
            })
            .unwrap();

        let loaded = manager.load().unwrap();
        assert!(loaded.disabled);
    }

    #[test]
    fn test_mark_evaluated_at_stores_exact_timestamp() {
        // AIDEV-NOTE: This tests the race condition fix.
        // We must store the transcript READ time, not completion time.
        // If we stored Utc::now() at completion, messages written during
        // the 30+ second LLM eval would be skipped.
        use chrono::TimeZone;

        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path());

        // Simulate: captured read time is 5 minutes in the past
        // (LLM eval took 5 minutes, but we want the READ time stored)
        let read_time = Utc.with_ymd_and_hms(2025, 1, 15, 10, 0, 0).unwrap();

        manager
            .update(|s| {
                s.mark_evaluated_at(read_time);
            })
            .unwrap();

        let loaded = manager.load().unwrap();

        // Critical: the stored timestamp must be the exact read_time,
        // NOT some later time like Utc::now()
        assert_eq!(loaded.last_evaluated, Some(read_time));
    }
}

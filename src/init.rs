/// Initialization for superego
///
/// Creates .superego/ directory structure and default files

use std::fs;
use std::path::Path;

use crate::decision::Phase;
use crate::state::{State, StateManager};

/// Default superego system prompt (embedded at compile time)
const DEFAULT_PROMPT: &str = include_str!("../default_prompt.md");

/// Error type for initialization
#[derive(Debug)]
pub enum InitError {
    IoError(std::io::Error),
    AlreadyExists,
    StateError(crate::state::StateError),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::IoError(e) => write!(f, "IO error: {}", e),
            InitError::AlreadyExists => write!(f, ".superego/ already exists"),
            InitError::StateError(e) => write!(f, "State error: {}", e),
        }
    }
}

impl std::error::Error for InitError {}

impl From<std::io::Error> for InitError {
    fn from(e: std::io::Error) -> Self {
        InitError::IoError(e)
    }
}

impl From<crate::state::StateError> for InitError {
    fn from(e: crate::state::StateError) -> Self {
        InitError::StateError(e)
    }
}

/// Initialize superego in the current directory
pub fn init(force: bool) -> Result<(), InitError> {
    init_at(Path::new("."), force)
}

/// Initialize superego at a specific path
pub fn init_at(base_dir: &Path, force: bool) -> Result<(), InitError> {
    let superego_dir = base_dir.join(".superego");

    // Check if already exists
    if superego_dir.exists() && !force {
        return Err(InitError::AlreadyExists);
    }

    // Create directory structure
    fs::create_dir_all(superego_dir.join("decisions"))?;
    fs::create_dir_all(superego_dir.join("session"))?;

    // Write default prompt
    fs::write(superego_dir.join("prompt.md"), DEFAULT_PROMPT)?;

    // Create initial state
    let state_mgr = StateManager::new(&superego_dir);
    let initial_state = State {
        phase: Phase::Exploring,
        ..State::default()
    };
    state_mgr.save(&initial_state)?;

    // Create empty config (placeholder for future settings)
    fs::write(
        superego_dir.join("config.yaml"),
        "# Superego configuration\n# model: claude-sonnet-4-20250514\n# timeout_ms: 30000\n",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_creates_structure() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();

        assert!(dir.path().join(".superego").exists());
        assert!(dir.path().join(".superego/decisions").exists());
        assert!(dir.path().join(".superego/session").exists());
        assert!(dir.path().join(".superego/prompt.md").exists());
        assert!(dir.path().join(".superego/state.json").exists());
        assert!(dir.path().join(".superego/config.yaml").exists());
    }

    #[test]
    fn test_init_fails_if_exists() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();
        let result = init_at(dir.path(), false);
        assert!(matches!(result, Err(InitError::AlreadyExists)));
    }

    #[test]
    fn test_init_force_overwrites() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();
        init_at(dir.path(), true).unwrap(); // Should succeed with force
    }
}

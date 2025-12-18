//! Initialization for superego
//!
//! Creates .superego/ directory structure and default files.
//! Hook setup is now handled by the Claude Code plugin.

use std::fs;
use std::path::Path;

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

    // Create .superego directory (subdirs created on-demand)
    fs::create_dir_all(&superego_dir)?;

    // Write default prompt
    fs::write(superego_dir.join("prompt.md"), DEFAULT_PROMPT)?;

    // Create initial state
    let state_mgr = StateManager::new(&superego_dir);
    state_mgr.save(&State::default())?;

    // Create config with defaults
    fs::write(
        superego_dir.join("config.yaml"),
        "# Superego configuration\neval_interval_minutes: 5  # Periodic eval to catch drift\n# model: claude-sonnet-4-20250514\n# timeout_ms: 30000\n",
    )?;

    // Update .gitignore
    update_gitignore(base_dir)?;

    Ok(())
}

/// Update .gitignore to exclude superego files
fn update_gitignore(base_dir: &Path) -> Result<(), InitError> {
    let gitignore_path = base_dir.join(".gitignore");
    let marker = "# Superego";
    let entries = ".superego/";

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)?;
        if content.contains(".superego/") {
            return Ok(());
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(&format!("\n{}\n{}\n", marker, entries));
        fs::write(&gitignore_path, new_content)?;
    } else {
        fs::write(&gitignore_path, format!("{}\n{}\n", marker, entries))?;
    }

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

    #[test]
    fn test_init_does_not_create_hooks() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();

        // Hook scripts should NOT exist (plugin provides them now)
        assert!(!dir.path().join(".claude/hooks/superego").exists());
        assert!(!dir.path().join(".claude/settings.json").exists());
    }

    #[test]
    fn test_gitignore_updated() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();

        let gitignore = dir.path().join(".gitignore");
        assert!(gitignore.exists());

        let content = fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains(".superego/"));
    }
}

//! Hook management for superego
//!
//! Handles checking and auto-updating hook scripts when they don't match
//! the embedded versions in the binary.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Embedded hook scripts (from plugin/scripts/ for legacy support)
const EVALUATE_HOOK: &str = include_str!("../plugin/scripts/evaluate.sh");
const SESSION_START_HOOK: &str = include_str!("../plugin/scripts/session-start.sh");
const PRE_TOOL_USE_HOOK: &str = include_str!("../plugin/scripts/pre-tool-use.sh");

/// Result of checking/updating hooks
#[derive(Debug, Default)]
pub struct UpdateResult {
    /// Names of hooks that were updated
    pub updated: Vec<String>,
    /// Names of hooks that were already current
    pub current: Vec<String>,
}

/// Compute a content hash for comparison
fn content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Write a hook file and set executable permissions
fn write_hook(path: &Path, content: &str) -> io::Result<()> {
    fs::write(path, content)?;

    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;

    Ok(())
}

/// Check if a deployed hook matches the embedded version, update if not
/// Returns true if the hook was updated, false if already current
fn check_and_update_hook(path: &Path, expected_content: &str) -> io::Result<bool> {
    // If hook doesn't exist, write it
    if !path.exists() {
        write_hook(path, expected_content)?;
        return Ok(true);
    }

    // Read current content and compare hashes
    let current_content = fs::read_to_string(path)?;
    let current_hash = content_hash(&current_content);
    let expected_hash = content_hash(expected_content);

    if current_hash != expected_hash {
        write_hook(path, expected_content)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Check and update all hooks in .claude/hooks/superego/
/// Creates the directory if it doesn't exist.
///
/// # Arguments
/// * `base_dir` - Project root directory (where .claude/ lives)
///
/// # Returns
/// * `Ok(UpdateResult)` - Which hooks were updated vs current
/// * `Err` - If hook directory can't be created or hooks can't be written
pub fn check_and_update_hooks(base_dir: &Path) -> io::Result<UpdateResult> {
    let hooks_dir = base_dir.join(".claude").join("hooks").join("superego");

    // Ensure directory exists
    fs::create_dir_all(&hooks_dir)?;

    let mut result = UpdateResult::default();

    // Check each hook
    let hooks = [
        ("evaluate.sh", EVALUATE_HOOK),
        ("session-start.sh", SESSION_START_HOOK),
        ("pre-tool-use.sh", PRE_TOOL_USE_HOOK),
    ];

    for (name, content) in hooks {
        let path = hooks_dir.join(name);
        let updated = check_and_update_hook(&path, content)?;

        if updated {
            result.updated.push(name.to_string());
        } else {
            result.current.push(name.to_string());
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_content_hash_same() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world!");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_check_creates_missing_hooks() {
        let dir = tempdir().unwrap();
        let result = check_and_update_hooks(dir.path()).unwrap();

        // All hooks should be created (updated)
        assert_eq!(result.updated.len(), 3);
        assert!(result.current.is_empty());

        // Verify files exist
        assert!(dir
            .path()
            .join(".claude/hooks/superego/evaluate.sh")
            .exists());
        assert!(dir
            .path()
            .join(".claude/hooks/superego/session-start.sh")
            .exists());
        assert!(dir
            .path()
            .join(".claude/hooks/superego/pre-tool-use.sh")
            .exists());
    }

    #[test]
    fn test_check_skips_current_hooks() {
        let dir = tempdir().unwrap();

        // First call creates hooks
        check_and_update_hooks(dir.path()).unwrap();

        // Second call should find them current
        let result = check_and_update_hooks(dir.path()).unwrap();
        assert!(result.updated.is_empty());
        assert_eq!(result.current.len(), 3);
    }

    #[test]
    fn test_check_updates_modified_hooks() {
        let dir = tempdir().unwrap();

        // Create hooks
        check_and_update_hooks(dir.path()).unwrap();

        // Modify one hook
        let hook_path = dir.path().join(".claude/hooks/superego/evaluate.sh");
        fs::write(&hook_path, "#!/bin/bash\necho modified").unwrap();

        // Check should update the modified hook
        let result = check_and_update_hooks(dir.path()).unwrap();
        assert_eq!(result.updated, vec!["evaluate.sh"]);
        assert_eq!(result.current.len(), 2);

        // Verify content was restored
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("Superego evaluation hook"));
    }
}

//! Migration from legacy hooks to plugin mode
//!
//! Removes .claude/hooks/superego/ directory and superego entries from settings.json

use std::path::Path;

/// Error type for migration
#[derive(Debug)]
pub enum MigrateError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    NoLegacyHooks,
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrateError::IoError(e) => write!(f, "IO error: {}", e),
            MigrateError::JsonError(e) => write!(f, "JSON error: {}", e),
            MigrateError::NoLegacyHooks => write!(f, "No legacy hooks found to migrate"),
        }
    }
}

impl std::error::Error for MigrateError {}

impl From<std::io::Error> for MigrateError {
    fn from(e: std::io::Error) -> Self {
        MigrateError::IoError(e)
    }
}

impl From<serde_json::Error> for MigrateError {
    fn from(e: serde_json::Error) -> Self {
        MigrateError::JsonError(e)
    }
}

/// Check if legacy hooks exist (pre-0.4.0 installations)
pub fn has_legacy_hooks(base_dir: &Path) -> bool {
    let hooks_dir = base_dir.join(".claude").join("hooks").join("superego");
    if hooks_dir.exists() {
        return true;
    }

    let settings_path = base_dir.join(".claude").join("settings.json");
    if settings_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&settings_path) {
            if content.contains("superego") {
                return true;
            }
        }
    }

    false
}

/// Migrate from legacy hooks to plugin mode
pub fn migrate(base_dir: &Path) -> Result<MigrateReport, MigrateError> {
    let mut report = MigrateReport::default();

    let hooks_dir = base_dir.join(".claude").join("hooks").join("superego");
    let settings_path = base_dir.join(".claude").join("settings.json");

    // Check if there's anything to migrate
    if !hooks_dir.exists() && !has_superego_in_settings(&settings_path) {
        return Err(MigrateError::NoLegacyHooks);
    }

    // Remove hook scripts directory
    if hooks_dir.exists() {
        std::fs::remove_dir_all(&hooks_dir)?;
        report.removed_hooks_dir = true;
    }

    // Remove superego entries from settings.json
    if settings_path.exists() && remove_superego_from_settings(&settings_path)? {
        report.updated_settings = true;
    }

    Ok(report)
}

/// Check if settings.json contains superego hooks
fn has_superego_in_settings(settings_path: &Path) -> bool {
    if !settings_path.exists() {
        return false;
    }

    let Ok(content) = std::fs::read_to_string(settings_path) else {
        return false;
    };

    content.contains("superego")
}

/// Remove superego hooks from settings.json
fn remove_superego_from_settings(settings_path: &Path) -> Result<bool, MigrateError> {
    let content = std::fs::read_to_string(settings_path)?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)?;

    let mut modified = false;

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (_name, hook_array) in hooks.iter_mut() {
            if let Some(arr) = hook_array.as_array_mut() {
                let original_len = arr.len();
                arr.retain(|h| {
                    !h.get("hooks")
                        .and_then(|hs| hs.as_array())
                        .and_then(|hs| hs.first())
                        .and_then(|h| h.get("command"))
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("superego"))
                        .unwrap_or(false)
                });
                if arr.len() != original_len {
                    modified = true;
                }
            }
        }
    }

    if modified {
        let formatted = serde_json::to_string_pretty(&settings)?;
        std::fs::write(settings_path, formatted)?;
    }

    Ok(modified)
}

/// Report of what was migrated
#[derive(Debug, Default)]
pub struct MigrateReport {
    pub removed_hooks_dir: bool,
    pub updated_settings: bool,
}

impl MigrateReport {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.removed_hooks_dir {
            parts.push("Removed .claude/hooks/superego/");
        }
        if self.updated_settings {
            parts.push("Removed superego hooks from .claude/settings.json");
        }

        if parts.is_empty() {
            "No changes made".to_string()
        } else {
            parts.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_migrate_removes_hooks_dir() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join(".claude").join("hooks").join("superego");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("evaluate.sh"), "#!/bin/bash").unwrap();

        let report = migrate(dir.path()).unwrap();

        assert!(report.removed_hooks_dir);
        assert!(!hooks_dir.exists());
    }

    #[test]
    fn test_migrate_updates_settings() {
        let dir = tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();

        let settings = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/superego/evaluate.sh"
                    }]
                }]
            }
        });
        std::fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let report = migrate(dir.path()).unwrap();

        assert!(report.updated_settings);

        let content = std::fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert!(!content.contains("superego"));
    }

    #[test]
    fn test_migrate_no_legacy_hooks() {
        let dir = tempdir().unwrap();
        let result = migrate(dir.path());
        assert!(matches!(result, Err(MigrateError::NoLegacyHooks)));
    }
}

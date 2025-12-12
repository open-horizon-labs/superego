/// Initialization for superego
///
/// Creates .superego/ directory structure, default files, and Claude Code hooks

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::{json, Value};

use crate::state::{State, StateManager};

/// Default superego system prompt (embedded at compile time)
const DEFAULT_PROMPT: &str = include_str!("../default_prompt.md");

/// Embedded hook scripts
const EVALUATE_HOOK: &str = include_str!("../hooks/evaluate.sh");
const SESSION_START_HOOK: &str = include_str!("../hooks/session-start.sh");
const PRE_TOOL_USE_HOOK: &str = include_str!("../hooks/pre-tool-use.sh");

/// Error type for initialization
#[derive(Debug)]
pub enum InitError {
    IoError(std::io::Error),
    AlreadyExists,
    StateError(crate::state::StateError),
    JsonError(serde_json::Error),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::IoError(e) => write!(f, "IO error: {}", e),
            InitError::AlreadyExists => write!(f, ".superego/ already exists"),
            InitError::StateError(e) => write!(f, "State error: {}", e),
            InitError::JsonError(e) => write!(f, "JSON error: {}", e),
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

impl From<serde_json::Error> for InitError {
    fn from(e: serde_json::Error) -> Self {
        InitError::JsonError(e)
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
    state_mgr.save(&State::default())?;

    // Create empty config (placeholder for future settings)
    fs::write(
        superego_dir.join("config.yaml"),
        "# Superego configuration\n# model: claude-sonnet-4-20250514\n# timeout_ms: 30000\n",
    )?;

    // Set up Claude Code hooks
    setup_hooks(base_dir)?;

    Ok(())
}

/// Write hook scripts and configure .claude/settings.json
fn setup_hooks(base_dir: &Path) -> Result<(), InitError> {
    let hooks_dir = base_dir.join(".claude").join("hooks").join("superego");
    fs::create_dir_all(&hooks_dir)?;

    // Write hook scripts
    let evaluate_path = hooks_dir.join("evaluate.sh");
    let session_start_path = hooks_dir.join("session-start.sh");
    let pre_tool_use_path = hooks_dir.join("pre-tool-use.sh");

    fs::write(&evaluate_path, EVALUATE_HOOK)?;
    fs::write(&session_start_path, SESSION_START_HOOK)?;
    fs::write(&pre_tool_use_path, PRE_TOOL_USE_HOOK)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        fs::set_permissions(&evaluate_path, fs::Permissions::from_mode(0o755))?;
        fs::set_permissions(&session_start_path, fs::Permissions::from_mode(0o755))?;
        fs::set_permissions(&pre_tool_use_path, fs::Permissions::from_mode(0o755))?;
    }

    // Update .claude/settings.json
    let settings_path = base_dir.join(".claude").join("settings.json");
    let mut settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content)?
    } else {
        json!({})
    };

    // Build hook config with absolute paths
    let evaluate_abs = fs::canonicalize(&evaluate_path)?;
    let session_start_abs = fs::canonicalize(&session_start_path)?;
    let pre_tool_use_abs = fs::canonicalize(&pre_tool_use_path)?;

    let superego_hook = |path: &Path| -> Value {
        json!({
            "hooks": [{
                "type": "command",
                "command": path.to_string_lossy()
            }]
        })
    };

    // Hook with matcher (for PermissionRequest filtering)
    let superego_hook_with_matcher = |path: &Path, matcher: &str| -> Value {
        json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": path.to_string_lossy()
            }]
        })
    };

    // Ensure hooks object exists
    if settings.get("hooks").is_none() {
        settings["hooks"] = json!({});
    }

    // Append superego hooks to existing hooks (don't overwrite)
    // Regular hooks (no matcher)
    for (hook_name, hook_path) in [
        ("Stop", &evaluate_abs),
        ("PreCompact", &evaluate_abs),
        ("SessionStart", &session_start_abs),
        ("PreToolUse", &pre_tool_use_abs),
    ] {
        let entry = superego_hook(hook_path);

        if let Some(existing) = settings["hooks"].get_mut(hook_name) {
            if let Some(arr) = existing.as_array_mut() {
                // Check if superego hook already exists (by command path)
                let already_exists = arr.iter().any(|h| {
                    h.get("hooks")
                        .and_then(|hs| hs.as_array())
                        .and_then(|hs| hs.first())
                        .and_then(|h| h.get("command"))
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("superego"))
                        .unwrap_or(false)
                });
                if !already_exists {
                    arr.push(entry);
                }
            }
        } else {
            settings["hooks"][hook_name] = json!([entry]);
        }
    }

    // Add PermissionRequest hook for ExitPlanMode (requires matcher)
    // AIDEV-NOTE: ExitPlanMode triggers a permission request, not a Stop event,
    // so we need this hook to evaluate before Claude exits plan mode.
    {
        let hook_name = "PermissionRequest";
        let entry = superego_hook_with_matcher(&evaluate_abs, "ExitPlanMode");

        if let Some(existing) = settings["hooks"].get_mut(hook_name) {
            if let Some(arr) = existing.as_array_mut() {
                let already_exists = arr.iter().any(|h| {
                    h.get("hooks")
                        .and_then(|hs| hs.as_array())
                        .and_then(|hs| hs.first())
                        .and_then(|h| h.get("command"))
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("superego"))
                        .unwrap_or(false)
                });
                if !already_exists {
                    arr.push(entry);
                }
            }
        } else {
            settings["hooks"][hook_name] = json!([entry]);
        }
    }

    // Write settings back
    let formatted = serde_json::to_string_pretty(&settings)?;
    fs::write(&settings_path, formatted)?;

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

    #[test]
    fn test_init_creates_hooks() {
        let dir = tempdir().unwrap();

        init_at(dir.path(), false).unwrap();

        // Check hook scripts exist
        assert!(dir.path().join(".claude/hooks/superego/evaluate.sh").exists());
        assert!(dir.path().join(".claude/hooks/superego/session-start.sh").exists());
        assert!(dir.path().join(".claude/hooks/superego/pre-tool-use.sh").exists());

        // Check settings.json exists and has hooks
        let settings_path = dir.path().join(".claude/settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        assert!(settings["hooks"]["Stop"].is_array());
        assert!(settings["hooks"]["PreCompact"].is_array());
        assert!(settings["hooks"]["SessionStart"].is_array());
        assert!(settings["hooks"]["PreToolUse"].is_array());
        assert!(settings["hooks"]["PermissionRequest"].is_array());

        // Check PermissionRequest has ExitPlanMode matcher
        let perm_hooks = settings["hooks"]["PermissionRequest"].as_array().unwrap();
        let has_exit_plan_mode = perm_hooks.iter().any(|h| {
            h.get("matcher")
                .and_then(|m| m.as_str())
                .map(|m| m == "ExitPlanMode")
                .unwrap_or(false)
        });
        assert!(has_exit_plan_mode, "PermissionRequest hook should have ExitPlanMode matcher");
    }

    #[test]
    fn test_init_preserves_existing_hooks() {
        let dir = tempdir().unwrap();

        // Create existing settings with a custom hook
        let claude_dir = dir.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let existing_settings = json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/my-custom-hook.sh"
                    }]
                }]
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        ).unwrap();

        // Run init
        init_at(dir.path(), false).unwrap();

        // Check that existing hook is preserved
        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 2, "Should have 2 hooks: original + superego");

        // Verify original hook still there
        let has_custom = stop_hooks.iter().any(|h| {
            h.get("hooks")
                .and_then(|hs| hs.as_array())
                .and_then(|hs| hs.first())
                .and_then(|h| h.get("command"))
                .and_then(|c| c.as_str())
                .map(|c| c.contains("my-custom-hook"))
                .unwrap_or(false)
        });
        assert!(has_custom, "Original custom hook should be preserved");
    }

    #[test]
    fn test_init_force_no_duplicate_hooks() {
        let dir = tempdir().unwrap();

        // Init twice with force
        init_at(dir.path(), false).unwrap();
        init_at(dir.path(), true).unwrap();

        // Check that hooks aren't duplicated
        let content = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 1, "Should have only 1 superego hook, not duplicated");
    }
}

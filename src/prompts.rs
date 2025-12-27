//! Prompt management for superego
//!
//! Handles multiple prompt templates (code, writing) with switching and backup.

use std::fs;
use std::path::Path;

/// Available prompt types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptType {
    Code,
    Writing,
}

impl PromptType {
    /// All available prompt types
    pub fn all() -> &'static [PromptType] {
        &[PromptType::Code, PromptType::Writing]
    }

    /// Get prompt type from string name
    pub fn from_name(name: &str) -> Option<PromptType> {
        match name.to_lowercase().as_str() {
            "code" => Some(PromptType::Code),
            "writing" => Some(PromptType::Writing),
            _ => None,
        }
    }

    /// Get the name of this prompt type
    pub fn name(&self) -> &'static str {
        match self {
            PromptType::Code => "code",
            PromptType::Writing => "writing",
        }
    }

    /// Get a short description of this prompt type
    pub fn description(&self) -> &'static str {
        match self {
            PromptType::Code => "Metacognitive advisor for coding agents",
            PromptType::Writing => "Co-author reviewer for writing and content creation",
        }
    }

    /// Get the embedded prompt content
    pub fn content(&self) -> &'static str {
        match self {
            PromptType::Code => include_str!("prompts/code.md"),
            PromptType::Writing => include_str!("prompts/writing.md"),
        }
    }
}

impl std::fmt::Display for PromptType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Error type for prompt operations
#[derive(Debug)]
pub enum PromptError {
    IoError(std::io::Error),
    NotInitialized,
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::IoError(e) => write!(f, "IO error: {}", e),
            PromptError::NotInitialized => write!(f, ".superego/ not initialized"),
        }
    }
}

impl std::error::Error for PromptError {}

impl From<std::io::Error> for PromptError {
    fn from(e: std::io::Error) -> Self {
        PromptError::IoError(e)
    }
}

/// Get the current base prompt from config
pub fn get_current_base(superego_dir: &Path) -> Option<PromptType> {
    let config_path = superego_dir.join("config.yaml");
    if !config_path.exists() {
        return Some(PromptType::Code); // Default
    }

    let content = fs::read_to_string(&config_path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("base_prompt:") {
            let value = line.strip_prefix("base_prompt:")?.trim();
            return PromptType::from_name(value);
        }
    }

    Some(PromptType::Code) // Default if not specified
}

/// Set the base prompt in config
fn set_base_prompt(superego_dir: &Path, prompt_type: PromptType) -> Result<(), PromptError> {
    let config_path = superego_dir.join("config.yaml");

    let content = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    // Check if base_prompt line exists
    let mut found = false;
    let mut new_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.trim().starts_with("base_prompt:") {
            new_lines.push(format!("base_prompt: {}", prompt_type.name()));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !found {
        // Add after the first comment block or at the start
        let insert_pos = new_lines
            .iter()
            .position(|l| !l.trim().starts_with('#') && !l.trim().is_empty())
            .unwrap_or(new_lines.len());

        // Insert blank line first (if not at start), then base_prompt after it
        if insert_pos > 0 {
            new_lines.insert(insert_pos, String::new()); // Blank line
            new_lines.insert(insert_pos + 1, format!("base_prompt: {}", prompt_type.name()));
        } else {
            new_lines.insert(insert_pos, format!("base_prompt: {}", prompt_type.name()));
        }
    }

    fs::write(&config_path, new_lines.join("\n") + "\n")?;
    Ok(())
}

/// Get backup path for a prompt type
fn backup_path(superego_dir: &Path, prompt_type: PromptType) -> std::path::PathBuf {
    superego_dir.join(format!("prompt.{}.md.bak", prompt_type.name()))
}

/// Check if the current prompt.md has local modifications vs the base template
pub fn has_local_modifications(superego_dir: &Path) -> bool {
    let prompt_path = superego_dir.join("prompt.md");
    let current_base = get_current_base(superego_dir).unwrap_or(PromptType::Code);

    if !prompt_path.exists() {
        return false;
    }

    match fs::read_to_string(&prompt_path) {
        Ok(current) => current.trim() != current_base.content().trim(),
        Err(_) => false,
    }
}

/// Switch to a different prompt type
pub fn switch(superego_dir: &Path, target: PromptType) -> Result<SwitchResult, PromptError> {
    if !superego_dir.exists() {
        return Err(PromptError::NotInitialized);
    }

    let prompt_path = superego_dir.join("prompt.md");
    let current_base = get_current_base(superego_dir).unwrap_or(PromptType::Code);

    let mut result = SwitchResult {
        from: current_base,
        to: target,
        backed_up: false,
        restored_from_backup: false,
    };

    // If switching to same type, backup modifications first then refresh
    if current_base == target {
        if prompt_path.exists() && has_local_modifications(superego_dir) {
            let backup = backup_path(superego_dir, current_base);
            fs::copy(&prompt_path, &backup)?;
            result.backed_up = true;
        }
        fs::write(&prompt_path, target.content())?;
        return Ok(result);
    }

    // Backup current prompt if it has modifications
    if prompt_path.exists() && has_local_modifications(superego_dir) {
        let backup = backup_path(superego_dir, current_base);
        fs::copy(&prompt_path, &backup)?;
        result.backed_up = true;
    }

    // Check if we have a backup for the target type
    let target_backup = backup_path(superego_dir, target);
    if target_backup.exists() {
        // Restore from backup
        fs::copy(&target_backup, &prompt_path)?;
        result.restored_from_backup = true;
    } else {
        // Use fresh template
        fs::write(&prompt_path, target.content())?;
    }

    // Update config
    set_base_prompt(superego_dir, target)?;

    Ok(result)
}

/// Result of a prompt switch operation
#[derive(Debug)]
pub struct SwitchResult {
    pub from: PromptType,
    pub to: PromptType,
    pub backed_up: bool,
    pub restored_from_backup: bool,
}

/// Info about the current prompt state
#[derive(Debug)]
pub struct PromptInfo {
    pub base: PromptType,
    pub has_modifications: bool,
    pub available_backups: Vec<PromptType>,
}

/// Get info about the current prompt state
pub fn info(superego_dir: &Path) -> Result<PromptInfo, PromptError> {
    if !superego_dir.exists() {
        return Err(PromptError::NotInitialized);
    }

    let base = get_current_base(superego_dir).unwrap_or(PromptType::Code);
    let has_modifications = has_local_modifications(superego_dir);

    let available_backups: Vec<PromptType> = PromptType::all()
        .iter()
        .filter(|pt| backup_path(superego_dir, **pt).exists())
        .copied()
        .collect();

    Ok(PromptInfo {
        base,
        has_modifications,
        available_backups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_superego_dir() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        let superego = dir.path().join(".superego");
        fs::create_dir_all(&superego).unwrap();
        fs::write(superego.join("prompt.md"), PromptType::Code.content()).unwrap();
        fs::write(
            superego.join("config.yaml"),
            "# Config\neval_interval_minutes: 5\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn test_prompt_type_from_name() {
        assert_eq!(PromptType::from_name("code"), Some(PromptType::Code));
        assert_eq!(PromptType::from_name("CODE"), Some(PromptType::Code));
        assert_eq!(PromptType::from_name("writing"), Some(PromptType::Writing));
        assert_eq!(PromptType::from_name("unknown"), None);
    }

    #[test]
    fn test_get_current_base_default() {
        let dir = setup_superego_dir();
        let superego = dir.path().join(".superego");

        // Without base_prompt in config, should default to code
        let base = get_current_base(&superego);
        assert_eq!(base, Some(PromptType::Code));
    }

    #[test]
    fn test_switch_creates_backup() {
        let dir = setup_superego_dir();
        let superego = dir.path().join(".superego");

        // Modify the prompt
        fs::write(superego.join("prompt.md"), "Modified code prompt").unwrap();

        // Switch to writing
        let result = switch(&superego, PromptType::Writing).unwrap();

        assert!(result.backed_up);
        assert!(!result.restored_from_backup);
        assert_eq!(result.from, PromptType::Code);
        assert_eq!(result.to, PromptType::Writing);

        // Backup should exist
        assert!(superego.join("prompt.code.md.bak").exists());

        // Prompt should be writing content
        let content = fs::read_to_string(superego.join("prompt.md")).unwrap();
        assert!(content.contains("Co-Author Reviewer"));
    }

    #[test]
    fn test_switch_restores_backup() {
        let dir = setup_superego_dir();
        let superego = dir.path().join(".superego");

        // Create a backup for writing
        fs::write(
            superego.join("prompt.writing.md.bak"),
            "My custom writing prompt",
        )
        .unwrap();

        // Switch to writing
        let result = switch(&superego, PromptType::Writing).unwrap();

        assert!(result.restored_from_backup);

        // Should have restored our custom content
        let content = fs::read_to_string(superego.join("prompt.md")).unwrap();
        assert_eq!(content, "My custom writing prompt");
    }

    #[test]
    fn test_info() {
        let dir = setup_superego_dir();
        let superego = dir.path().join(".superego");

        let prompt_info = info(&superego).unwrap();
        assert_eq!(prompt_info.base, PromptType::Code);
        assert!(!prompt_info.has_modifications);
        assert!(prompt_info.available_backups.is_empty());

        // Modify and create backup
        fs::write(superego.join("prompt.md"), "Modified").unwrap();
        fs::write(superego.join("prompt.writing.md.bak"), "Backup").unwrap();

        let prompt_info = info(&superego).unwrap();
        assert!(prompt_info.has_modifications);
        assert_eq!(prompt_info.available_backups, vec![PromptType::Writing]);
    }
}

//! Feedback queue for superego
//!
//! Async evaluation writes feedback here, hooks check and retrieve it.
//! AIDEV-NOTE: Simplified to just message. No severity levels -
//! all feedback is informational, Claude decides how to act on it.

use std::fs;
use std::path::{Path, PathBuf};

/// Feedback entry
#[derive(Debug, Clone)]
pub struct Feedback {
    pub message: String,
}

impl Feedback {
    pub fn new(message: impl Into<String>) -> Self {
        Feedback {
            message: message.into(),
        }
    }

    /// Alias for new() - kept for compatibility during transition
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}

/// Feedback queue manager
pub struct FeedbackQueue {
    feedback_path: PathBuf,
}

impl FeedbackQueue {
    pub fn new(superego_dir: &Path) -> Self {
        FeedbackQueue {
            feedback_path: superego_dir.join("feedback"),
        }
    }

    /// Check if there's pending feedback (instant, no parsing)
    pub fn has_feedback(&self) -> bool {
        self.feedback_path.exists() &&
            fs::metadata(&self.feedback_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
    }

    /// Write feedback to queue (overwrites existing)
    pub fn write(&self, feedback: &Feedback) -> std::io::Result<()> {
        fs::write(&self.feedback_path, &feedback.message)
    }

    /// Get feedback and clear queue
    pub fn get_and_clear(&self) -> Option<String> {
        if !self.has_feedback() {
            return None;
        }

        let content = fs::read_to_string(&self.feedback_path).ok()?;
        let _ = fs::remove_file(&self.feedback_path);
        Some(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_no_feedback() {
        let dir = tempdir().unwrap();
        let queue = FeedbackQueue::new(dir.path());
        assert!(!queue.has_feedback());
        assert!(queue.get_and_clear().is_none());
    }

    #[test]
    fn test_write_and_read() {
        let dir = tempdir().unwrap();
        let queue = FeedbackQueue::new(dir.path());

        let fb = Feedback::new("No task in progress");
        queue.write(&fb).unwrap();

        assert!(queue.has_feedback());

        let content = queue.get_and_clear().unwrap();
        assert!(content.contains("No task in progress"));
        assert!(!queue.has_feedback());
    }
}

// noFriction Meetings - Diff Builder
// Computes unified text diffs between snapshots
//
// This module provides:
// 1. Text comparison and diffing
// 2. Unified diff format generation
// 3. Change type classification
// 4. Change summary generation

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Change types for text modifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// New content added
    ContentAdded,
    /// Content removed
    ContentRemoved,
    /// Text reworded but meaning similar
    Reworded,
    /// Only formatting changed (whitespace, etc)
    FormatOnly,
    /// Cursor or selection changes only
    CursorOnly,
    /// Scroll changes only
    ScrollOnly,
    /// Navigation to different section
    Navigation,
    /// New document opened
    NewDocument,
    /// Mixed changes
    ContentChanged,
}

impl Default for ChangeType {
    fn default() -> Self {
        Self::ContentChanged
    }
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContentAdded => "content_added",
            Self::ContentRemoved => "content_removed",
            Self::Reworded => "reworded",
            Self::FormatOnly => "format_only",
            Self::CursorOnly => "cursor_only",
            Self::ScrollOnly => "scroll_only",
            Self::Navigation => "navigation",
            Self::NewDocument => "new_document",
            Self::ContentChanged => "content_changed",
        }
    }
}

/// Result of a text diff operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDiff {
    pub patch_id: String,
    pub episode_id: String,
    pub from_text_hash: String,
    pub to_text_hash: String,
    pub ts: DateTime<Utc>,
    pub unified_diff: String,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub change_type: ChangeType,
    pub change_summary: Option<String>,
}

/// Configuration for diff builder
#[derive(Debug, Clone)]
pub struct DiffConfig {
    /// Maximum lines to include in diff output
    pub max_diff_lines: usize,

    /// Context lines around changes
    pub context_lines: usize,

    /// Whether to generate summaries
    pub generate_summaries: bool,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            max_diff_lines: 500,
            context_lines: 3,
            generate_summaries: true,
        }
    }
}

/// Diff builder for comparing text snapshots
pub struct DiffBuilder {
    config: DiffConfig,
}

impl DiffBuilder {
    pub fn new() -> Self {
        Self::with_config(DiffConfig::default())
    }

    pub fn with_config(config: DiffConfig) -> Self {
        Self { config }
    }

    /// Compute diff between two text snapshots
    pub fn compute_diff(
        &self,
        episode_id: &str,
        from_text: &str,
        to_text: &str,
        timestamp: DateTime<Utc>,
    ) -> TextDiff {
        let from_hash = Self::compute_text_hash(from_text);
        let to_hash = Self::compute_text_hash(to_text);

        // Early exit if identical
        if from_hash == to_hash {
            return TextDiff {
                patch_id: Uuid::new_v4().to_string(),
                episode_id: episode_id.to_string(),
                from_text_hash: from_hash,
                to_text_hash: to_hash,
                ts: timestamp,
                unified_diff: String::new(),
                lines_added: 0,
                lines_removed: 0,
                change_type: ChangeType::CursorOnly, // No visible change
                change_summary: Some("No text changes detected".to_string()),
            };
        }

        // Compute line-by-line diff
        let from_lines: Vec<&str> = from_text.lines().collect();
        let to_lines: Vec<&str> = to_text.lines().collect();

        let (unified_diff, lines_added, lines_removed) =
            self.generate_unified_diff(&from_lines, &to_lines);

        // Classify the change type
        let change_type = self.classify_change(from_text, to_text, lines_added, lines_removed);

        // Generate summary
        let change_summary = if self.config.generate_summaries {
            Some(self.generate_summary(lines_added, lines_removed, &change_type))
        } else {
            None
        };

        TextDiff {
            patch_id: Uuid::new_v4().to_string(),
            episode_id: episode_id.to_string(),
            from_text_hash: from_hash,
            to_text_hash: to_hash,
            ts: timestamp,
            unified_diff,
            lines_added,
            lines_removed,
            change_type,
            change_summary,
        }
    }

    /// Compute SHA256 hash of text
    pub fn compute_text_hash(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check if two texts are similar enough to be considered scroll-only
    pub fn is_scroll_like(&self, from_text: &str, to_text: &str) -> bool {
        // Normalize whitespace and compare
        let from_normalized = self.normalize_for_comparison(from_text);
        let to_normalized = self.normalize_for_comparison(to_text);

        // Calculate word overlap
        let from_words: std::collections::HashSet<&str> =
            from_normalized.split_whitespace().collect();
        let to_words: std::collections::HashSet<&str> = to_normalized.split_whitespace().collect();

        if from_words.is_empty() || to_words.is_empty() {
            return false;
        }

        let intersection = from_words.intersection(&to_words).count();
        let total = from_words.len().max(to_words.len());

        // 95% overlap suggests scroll-only change
        (intersection as f32 / total as f32) >= 0.95
    }

    /// Generate unified diff format
    fn generate_unified_diff(&self, from_lines: &[&str], to_lines: &[&str]) -> (String, i32, i32) {
        let mut diff_output = String::new();
        let mut lines_added = 0i32;
        let mut lines_removed = 0i32;

        // Simple LCS-based diff (production would use a proper diff library)
        let changes = self.compute_line_changes(from_lines, to_lines);

        for change in changes.iter().take(self.config.max_diff_lines) {
            match change {
                LineChange::Unchanged(line) => {
                    diff_output.push_str(&format!(" {}\n", line));
                }
                LineChange::Added(line) => {
                    diff_output.push_str(&format!("+{}\n", line));
                    lines_added += 1;
                }
                LineChange::Removed(line) => {
                    diff_output.push_str(&format!("-{}\n", line));
                    lines_removed += 1;
                }
            }
        }

        (diff_output, lines_added, lines_removed)
    }

    /// Simple line change detection (LCS-based would be more accurate)
    fn compute_line_changes<'a>(
        &self,
        from_lines: &[&'a str],
        to_lines: &[&'a str],
    ) -> Vec<LineChange<'a>> {
        let mut changes = Vec::new();
        let from_set: std::collections::HashSet<&str> = from_lines.iter().copied().collect();
        let to_set: std::collections::HashSet<&str> = to_lines.iter().copied().collect();

        // Lines only in from -> removed
        for line in from_lines {
            if !to_set.contains(line) {
                changes.push(LineChange::Removed(line));
            }
        }

        // Lines only in to -> added
        for line in to_lines {
            if !from_set.contains(line) {
                changes.push(LineChange::Added(line));
            }
        }

        // Add some context (lines in both)
        let context_count = self.config.context_lines;
        for (i, line) in to_lines.iter().enumerate() {
            if from_set.contains(line) && i < context_count {
                changes.insert(0, LineChange::Unchanged(line));
            }
        }

        changes
    }

    /// Classify the type of change
    fn classify_change(
        &self,
        from_text: &str,
        to_text: &str,
        lines_added: i32,
        lines_removed: i32,
    ) -> ChangeType {
        // New document check
        if from_text.is_empty() && !to_text.is_empty() {
            return ChangeType::NewDocument;
        }

        // All removed
        if !from_text.is_empty() && to_text.is_empty() {
            return ChangeType::ContentRemoved;
        }

        // Scroll-like check
        if self.is_scroll_like(from_text, to_text) {
            return ChangeType::ScrollOnly;
        }

        // Format-only check (same words, different whitespace)
        let from_normalized = self.normalize_for_comparison(from_text);
        let to_normalized = self.normalize_for_comparison(to_text);
        if from_normalized == to_normalized {
            return ChangeType::FormatOnly;
        }

        // Content classification
        if lines_added > 0 && lines_removed == 0 {
            ChangeType::ContentAdded
        } else if lines_removed > 0 && lines_added == 0 {
            ChangeType::ContentRemoved
        } else {
            ChangeType::ContentChanged
        }
    }

    /// Normalize text for comparison
    fn normalize_for_comparison(&self, text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Generate human-readable summary
    fn generate_summary(&self, added: i32, removed: i32, change_type: &ChangeType) -> String {
        match change_type {
            ChangeType::ContentAdded => format!("{} lines added", added),
            ChangeType::ContentRemoved => format!("{} lines removed", removed),
            ChangeType::ScrollOnly => "Scroll position changed".to_string(),
            ChangeType::FormatOnly => "Formatting changes only".to_string(),
            ChangeType::NewDocument => "New document opened".to_string(),
            ChangeType::CursorOnly => "No visible changes".to_string(),
            _ => format!("+{} -{} lines", added, removed),
        }
    }
}

/// Type of line change
#[derive(Debug, Clone)]
enum LineChange<'a> {
    Unchanged(&'a str),
    Added(&'a str),
    Removed(&'a str),
}

impl Default for DiffBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_text_no_diff() {
        let builder = DiffBuilder::new();
        let text = "Hello world\nThis is a test";
        let diff = builder.compute_diff("ep1", text, text, Utc::now());

        assert_eq!(diff.lines_added, 0);
        assert_eq!(diff.lines_removed, 0);
    }

    #[test]
    fn test_added_lines() {
        let builder = DiffBuilder::new();
        let from = "Line 1";
        let to = "Line 1\nLine 2\nLine 3";
        let diff = builder.compute_diff("ep1", from, to, Utc::now());

        assert_eq!(diff.lines_added, 2);
        assert_eq!(diff.lines_removed, 0);
        assert_eq!(diff.change_type, ChangeType::ContentAdded);
    }

    #[test]
    fn test_text_hash() {
        let hash1 = DiffBuilder::compute_text_hash("Hello");
        let hash2 = DiffBuilder::compute_text_hash("Hello");
        let hash3 = DiffBuilder::compute_text_hash("World");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_scroll_detection() {
        let builder = DiffBuilder::new();

        // 95%+ overlap should be scroll-like
        let text1 = "A B C D E F G H I J K L M N O P Q R S T";
        let text2 = "A B C D E F G H I J K L M N O P Q R S X"; // Only 1 char different

        // This will depend on implementation
        let is_scroll = builder.is_scroll_like(text1, text2);
        // High overlap = scroll-like
        assert!(is_scroll);
    }
}

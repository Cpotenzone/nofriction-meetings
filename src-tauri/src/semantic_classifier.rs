// noFriction Meetings - Semantic Classifier Module
// Uses LLM (VLM client) to classify text diffs with semantic understanding
//
// Replaces heuristic classification in diff_builder for more accurate results

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::diff_builder::ChangeType;

/// Semantic classification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticClassification {
    /// Classified change type
    pub change_type: ChangeType,
    /// Classification confidence (0.0 - 1.0)
    pub confidence: f32,
    /// LLM reasoning for the classification
    pub reasoning: String,
    /// Entities affected by the change
    pub entities: Vec<String>,
    /// Model used for classification
    pub model: String,
    /// Classification timestamp
    pub classified_at: DateTime<Utc>,
}

/// Configuration for semantic classifier
#[derive(Debug, Clone)]
pub struct ClassifierConfig {
    /// Enable LLM classification (falls back to heuristic if false)
    pub enabled: bool,
    /// Timeout for LLM request in seconds
    pub timeout_secs: u64,
    /// Minimum diff size to use LLM (chars)
    pub min_diff_size: usize,
    /// Maximum diff size to send to LLM (chars)
    pub max_diff_size: usize,
    /// Cache recent classifications
    pub enable_cache: bool,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 5,
            min_diff_size: 20,
            max_diff_size: 4000,
            enable_cache: true,
        }
    }
}

/// Classification cache entry
struct CacheEntry {
    diff_hash: String,
    result: SemanticClassification,
    expires_at: DateTime<Utc>,
}

/// Semantic classifier using VLM
pub struct SemanticClassifier {
    config: ClassifierConfig,
    cache: Arc<RwLock<Vec<CacheEntry>>>,
}

impl SemanticClassifier {
    pub fn new() -> Self {
        Self::with_config(ClassifierConfig::default())
    }

    pub fn with_config(config: ClassifierConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Classify a diff using LLM
    pub async fn classify_diff(
        &self,
        diff: &str,
        context: Option<&DiffContext>,
    ) -> Result<SemanticClassification, String> {
        if !self.config.enabled {
            return Err("Semantic classification disabled".to_string());
        }

        // Check size bounds
        if diff.len() < self.config.min_diff_size {
            return self.classify_heuristic(diff, context);
        }

        // Truncate if too large
        let diff_to_analyze = if diff.len() > self.config.max_diff_size {
            &diff[..self.config.max_diff_size]
        } else {
            diff
        };

        // Check cache
        if self.config.enable_cache {
            let diff_hash = compute_hash(diff_to_analyze);
            let cache = self.cache.read();
            let now = Utc::now();

            for entry in cache.iter() {
                if entry.diff_hash == diff_hash && entry.expires_at > now {
                    return Ok(entry.result.clone());
                }
            }
        }

        // Build prompt
        let prompt = self.build_classification_prompt(diff_to_analyze, context);

        // Call VLM
        match self.call_vlm(&prompt).await {
            Ok(response) => {
                let classification = self.parse_llm_response(&response)?;

                // Cache result
                if self.config.enable_cache {
                    let diff_hash = compute_hash(diff_to_analyze);
                    let mut cache = self.cache.write();

                    // Clean expired entries
                    let now = Utc::now();
                    cache.retain(|e| e.expires_at > now);

                    // Add new entry (cache for 5 minutes)
                    cache.push(CacheEntry {
                        diff_hash,
                        result: classification.clone(),
                        expires_at: now + chrono::Duration::minutes(5),
                    });
                }

                Ok(classification)
            }
            Err(e) => {
                log::warn!("LLM classification failed, using heuristic: {}", e);
                self.classify_heuristic(diff, context)
            }
        }
    }

    /// Heuristic fallback classification
    fn classify_heuristic(
        &self,
        diff: &str,
        _context: Option<&DiffContext>,
    ) -> Result<SemanticClassification, String> {
        // Count addition and deletion lines
        let mut additions = 0;
        let mut deletions = 0;

        for line in diff.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }

        let change_type = if additions > 0 && deletions == 0 {
            ChangeType::ContentAdded
        } else if deletions > 0 && additions == 0 {
            ChangeType::ContentRemoved
        } else if additions > 0 && deletions > 0 {
            // Mixed changes - could be reworded or general content change
            if (additions as i32 - deletions as i32).abs() <= 2 {
                ChangeType::Reworded
            } else {
                ChangeType::ContentChanged
            }
        } else {
            ChangeType::ContentChanged // Fallback
        };

        Ok(SemanticClassification {
            change_type,
            confidence: 0.6, // Lower confidence for heuristic
            reasoning: format!(
                "Heuristic: {} additions, {} deletions",
                additions, deletions
            ),
            entities: Vec::new(),
            model: "heuristic".to_string(),
            classified_at: Utc::now(),
        })
    }

    /// Build LLM prompt for classification
    fn build_classification_prompt(&self, diff: &str, context: Option<&DiffContext>) -> String {
        let context_str = if let Some(ctx) = context {
            format!(
                "Context:\n- App: {}\n- Window: {}\n- Previous state: {}\n\n",
                ctx.app_name.as_deref().unwrap_or("unknown"),
                ctx.window_title.as_deref().unwrap_or("unknown"),
                ctx.previous_summary.as_deref().unwrap_or("none")
            )
        } else {
            String::new()
        };

        format!(
            r#"Analyze this text diff and classify the type of change.

{}Diff:
```
{}
```

Respond in JSON format:
{{
  "change_type": "one of: content_added, content_removed, content_reworded, content_expanded, content_condensed, formatting_only, navigation, cursor_only, scroll_only, unknown",
  "confidence": 0.0-1.0,
  "reasoning": "brief explanation",
  "entities": ["affected entities like file names, function names, etc"]
}}

Only respond with the JSON, no other text."#,
            context_str, diff
        )
    }

    /// Call VLM for classification
    async fn call_vlm(&self, prompt: &str) -> Result<String, String> {
        use crate::vlm_client;

        let timeout = tokio::time::Duration::from_secs(self.config.timeout_secs);

        match tokio::time::timeout(timeout, vlm_client::vlm_chat(prompt)).await {
            Ok(result) => result,
            Err(_) => Err("VLM request timed out".to_string()),
        }
    }

    /// Parse LLM JSON response
    fn parse_llm_response(&self, response: &str) -> Result<SemanticClassification, String> {
        // Try to extract JSON from response
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        #[derive(Deserialize)]
        struct LlmResponse {
            change_type: String,
            confidence: f32,
            reasoning: String,
            #[serde(default)]
            entities: Vec<String>,
        }

        let parsed: LlmResponse = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        let change_type = match parsed.change_type.to_lowercase().as_str() {
            "content_added" => ChangeType::ContentAdded,
            "content_removed" => ChangeType::ContentRemoved,
            "content_reworded" | "reworded" => ChangeType::Reworded,
            "content_expanded" | "content_condensed" | "content_changed" => {
                ChangeType::ContentChanged
            }
            "formatting_only" | "format_only" => ChangeType::FormatOnly,
            "navigation" | "new_document" => ChangeType::Navigation,
            "cursor_only" => ChangeType::CursorOnly,
            "scroll_only" => ChangeType::ScrollOnly,
            _ => ChangeType::ContentChanged,
        };

        Ok(SemanticClassification {
            change_type,
            confidence: parsed.confidence.clamp(0.0, 1.0),
            reasoning: parsed.reasoning,
            entities: parsed.entities,
            model: "vlm".to_string(),
            classified_at: Utc::now(),
        })
    }

    /// Classify a batch of diffs
    pub async fn classify_batch(
        &self,
        diffs: Vec<(&str, Option<&DiffContext>)>,
    ) -> Vec<Result<SemanticClassification, String>> {
        let mut results = Vec::with_capacity(diffs.len());

        for (diff, context) in diffs {
            results.push(self.classify_diff(diff, context).await);
        }

        results
    }

    /// Clear the classification cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }
}

impl Default for SemanticClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for diff classification
#[derive(Debug, Clone)]
pub struct DiffContext {
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub previous_summary: Option<String>,
}

/// Compute a simple hash for caching
fn compute_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();

    format!("{:x}", result)[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ClassifierConfig::default();
        assert!(config.enabled);
        assert_eq!(config.timeout_secs, 5);
        assert!(config.enable_cache);
    }

    #[test]
    fn test_heuristic_additions() {
        let classifier = SemanticClassifier::new();
        let diff = "+line1\n+line2\n+line3";

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { classifier.classify_diff(diff, None).await });

        assert!(result.is_ok());
        let classification = result.unwrap();
        assert_eq!(classification.change_type, ChangeType::ContentAdded);
    }

    #[test]
    fn test_heuristic_deletions() {
        let classifier = SemanticClassifier::new();
        let diff = "-line1\n-line2\n-line3";

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { classifier.classify_diff(diff, None).await });

        assert!(result.is_ok());
        let classification = result.unwrap();
        assert_eq!(classification.change_type, ChangeType::ContentRemoved);
    }

    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash("test content");
        let hash2 = compute_hash("test content");
        let hash3 = compute_hash("different content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_parse_response() {
        let classifier = SemanticClassifier::new();
        let response = r#"{"change_type": "content_added", "confidence": 0.95, "reasoning": "New code added", "entities": ["main.rs"]}"#;

        let result = classifier.parse_llm_response(response);
        assert!(result.is_ok());

        let classification = result.unwrap();
        assert_eq!(classification.change_type, ChangeType::ContentAdded);
        assert_eq!(classification.confidence, 0.95);
        assert!(classification.entities.contains(&"main.rs".to_string()));
    }
}

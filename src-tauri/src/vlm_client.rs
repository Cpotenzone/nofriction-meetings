//! Ollama VLM Client for screenshot analysis
//! 
//! Uses LLaVA model via Ollama to analyze screenshots and extract activity context.

use base64::Engine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Activity context extracted from a screenshot by VLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityContext {
    /// Primary application being used
    pub app_name: Option<String>,
    /// Window title or document name
    pub window_title: Option<String>,
    /// High-level category (development, communication, research, etc.)
    pub category: String,
    /// What the user appears to be doing
    pub summary: String,
    /// Specific focus area or task
    pub focus_area: Option<String>,
    /// Visible project or file names
    pub visible_files: Vec<String>,
    /// Confidence score 0-1
    pub confidence: f32,
}

/// Response from Ollama generate API
#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
    done: bool,
}

/// VLM Client for local Ollama instance
pub struct VLMClient {
    base_url: Arc<RwLock<String>>,
    model: Arc<RwLock<String>>,
}

impl VLMClient {
    pub fn new() -> Self {
        Self {
            base_url: Arc::new(RwLock::new("http://localhost:11434".to_string())),
            model: Arc::new(RwLock::new("llava".to_string())),
        }
    }

    pub fn set_base_url(&self, url: String) {
        *self.base_url.write() = url;
    }

    pub fn set_model(&self, model: String) {
        *self.model.write() = model;
    }

    /// Check if Ollama is available
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url.read());
        match reqwest::get(&url).await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Check if the vision model is available
    pub async fn has_vision_model(&self) -> Result<bool, String> {
        let url = format!("{}/api/tags", self.base_url.read());
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let tags: TagsResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        let model_name = self.model.read().clone();
        Ok(tags.models.iter().any(|m| m.name.starts_with(&model_name)))
    }

    /// Analyze a screenshot and extract activity context
    pub async fn analyze_frame(&self, image_path: &str) -> Result<ActivityContext, String> {
        // Read and encode image as base64
        let image_data = std::fs::read(image_path)
            .map_err(|e| format!("Failed to read image: {}", e))?;
        let base64_image = base64::engine::general_purpose::STANDARD.encode(&image_data);

        let prompt = r#"Analyze this screenshot and describe what the user is doing. 
Respond in JSON format with these fields:
{
  "app_name": "name of the main application visible",
  "window_title": "title of the window or document",
  "category": "one of: development, communication, research, writing, design, media, browsing, system, other",
  "summary": "brief description of what the user is doing",
  "focus_area": "specific task or project they appear to be working on",
  "visible_files": ["list", "of", "visible", "file", "names"],
  "confidence": 0.8
}
Only respond with valid JSON, no other text."#;

        let url = format!("{}/api/generate", self.base_url.read());
        let model = self.model.read().clone();

        let request_body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "images": [base64_image],
            "stream": false,
            "options": {
                "temperature": 0.1
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&request_body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| format!("Failed to send request to Ollama: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Ollama returned error: {}", resp.status()));
        }

        let ollama_resp: OllamaGenerateResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        // Parse the JSON response from the model
        self.parse_activity_response(&ollama_resp.response)
    }

    /// Parse VLM response into ActivityContext
    fn parse_activity_response(&self, response: &str) -> Result<ActivityContext, String> {
        // Try to extract JSON from the response (model might include extra text)
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        // Parse JSON
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse VLM JSON response: {} - Response was: {}", e, response))?;

        Ok(ActivityContext {
            app_name: parsed.get("app_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            window_title: parsed.get("window_title").and_then(|v| v.as_str()).map(|s| s.to_string()),
            category: parsed.get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("other")
                .to_string(),
            summary: parsed.get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown activity")
                .to_string(),
            focus_area: parsed.get("focus_area").and_then(|v| v.as_str()).map(|s| s.to_string()),
            visible_files: parsed.get("visible_files")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            confidence: parsed.get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32,
        })
    }

    /// Analyze multiple frames and aggregate context
    pub async fn analyze_frames_batch(&self, image_paths: &[String]) -> Result<Vec<ActivityContext>, String> {
        let mut results = Vec::new();
        
        for path in image_paths {
            match self.analyze_frame(path).await {
                Ok(context) => results.push(context),
                Err(e) => {
                    log::warn!("Failed to analyze frame {}: {}", path, e);
                    // Continue with other frames
                }
            }
        }

        Ok(results)
    }
}

impl Default for VLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VLMClient {
    /// Get URL and model (for async operations without holding guard)
    pub fn get_url_and_model(&self) -> (String, String) {
        (self.base_url.read().clone(), self.model.read().clone())
    }
}

// ============================================
// Standalone async functions (avoid RwLock guard issues)
// ============================================

/// Check if Ollama is available (standalone)
pub async fn vlm_is_available(base_url: &str) -> bool {
    let url = format!("{}/api/tags", base_url);
    match reqwest::get(&url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Analyze a frame with VLM (standalone)
pub async fn vlm_analyze_frame(
    base_url: &str,
    model: &str,
    image_path: &str,
) -> Result<ActivityContext, String> {
    // Read and encode image as base64
    let image_data = std::fs::read(image_path)
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let base64_image = base64::engine::general_purpose::STANDARD.encode(&image_data);

    let prompt = r#"Analyze this screenshot and describe what the user is doing. 
Respond in JSON format with these fields:
{
  "app_name": "name of the main application visible",
  "window_title": "title of the window or document",
  "category": "one of: development, communication, research, writing, design, media, browsing, system, other",
  "summary": "brief description of what the user is doing",
  "focus_area": "specific task or project they appear to be working on",
  "visible_files": ["list", "of", "visible", "file", "names"],
  "confidence": 0.8
}
Only respond with valid JSON, no other text."#;

    let url = format!("{}/api/generate", base_url);

    let request_body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "images": [base64_image],
        "stream": false,
        "options": {
            "temperature": 0.1
        }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Ollama: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned error: {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct OllamaResp {
        response: String,
    }

    let ollama_resp: OllamaResp = resp.json().await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    // Parse the JSON response from the model
    parse_activity_response(&ollama_resp.response)
}

/// Parse VLM response into ActivityContext (helper)
fn parse_activity_response(response: &str) -> Result<ActivityContext, String> {
    // Try to extract JSON from the response (model might include extra text)
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    // Parse JSON
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse VLM JSON response: {} - Response was: {}", e, response))?;

    Ok(ActivityContext {
        app_name: parsed.get("app_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        window_title: parsed.get("window_title").and_then(|v| v.as_str()).map(|s| s.to_string()),
        category: parsed.get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("other")
            .to_string(),
        summary: parsed.get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown activity")
            .to_string(),
        focus_area: parsed.get("focus_area").and_then(|v| v.as_str()).map(|s| s.to_string()),
        visible_files: parsed.get("visible_files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
        confidence: parsed.get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32,
    })
}

/// Check if vision model is available (standalone)
pub async fn vlm_has_vision_model(base_url: &str, model: &str) -> Result<bool, String> {
    let url = format!("{}/api/tags", base_url);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

    #[derive(Deserialize)]
    struct TagsResponse {
        models: Vec<ModelInfo>,
    }

    #[derive(Deserialize)]
    struct ModelInfo {
        name: String,
    }

    let tags: TagsResponse = resp.json().await
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    Ok(tags.models.iter().any(|m| m.name.starts_with(model)))
}

/// Analyze multiple frames with VLM (standalone)
pub async fn vlm_analyze_frames_batch(
    base_url: &str,
    model: &str,
    image_paths: &[String],
) -> Result<Vec<ActivityContext>, String> {
    let mut results = Vec::new();
    
    for path in image_paths {
        match vlm_analyze_frame(base_url, model, path).await {
            Ok(context) => results.push(context),
            Err(e) => {
                log::warn!("Failed to analyze frame {}: {}", path, e);
                // Continue with other frames
            }
        }
    }

    Ok(results)
}

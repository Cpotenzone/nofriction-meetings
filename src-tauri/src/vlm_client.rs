//! VLM Client for Centralized qwen2.5vl API
//!
//! Uses Ollama-compatible REST API via SSH tunnel with bearer token authentication.
//! Models: qwen2.5vl:7b (primary), qwen2.5vl:3b (fallback)

use base64::Engine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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
    /// Extracted entities (people, companies, etc.)
    pub entities: Option<serde_json::Value>,
}

/// Message for chat API
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

/// Request payload for /api/chat
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptions>,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// Response from /api/chat
#[derive(Debug, Deserialize)]
struct ChatResponse {
    model: String,
    message: ChatMessageResponse,
    done: bool,
    #[serde(default)]
    total_duration: Option<u64>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    role: String,
    content: String,
}

/// Response from /api/tags
#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    name: String,
}

/// VLM Client for centralized API via SSH tunnel
pub struct VLMClient {
    /// Base URL (e.g., http://localhost:8080)
    base_url: Arc<RwLock<String>>,
    /// Bearer token for authentication
    bearer_token: Arc<RwLock<Option<String>>>,
    /// Primary model (qwen2.5vl:7b)
    model_primary: Arc<RwLock<String>>,
    /// Fallback model (qwen2.5vl:3b)
    model_fallback: Arc<RwLock<String>>,
    /// HTTP client with timeout
    client: reqwest::Client,
}

impl VLMClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();

        Self {
            base_url: Arc::new(RwLock::new("http://localhost:8080".to_string())),
            bearer_token: Arc::new(RwLock::new(None)),
            model_primary: Arc::new(RwLock::new("qwen2.5vl:7b".to_string())),
            model_fallback: Arc::new(RwLock::new("qwen2.5vl:3b".to_string())),
            client,
        }
    }

    /// Configure the client
    pub fn configure(&self, base_url: String, token: Option<String>) {
        *self.base_url.write() = base_url;
        *self.bearer_token.write() = token;
    }

    pub fn set_base_url(&self, url: String) {
        *self.base_url.write() = url;
    }

    pub fn set_bearer_token(&self, token: String) {
        *self.bearer_token.write() = Some(token);
    }

    pub fn set_model(&self, model: String) {
        *self.model_primary.write() = model;
    }

    pub fn set_fallback_model(&self, model: String) {
        *self.model_fallback.write() = model;
    }

    /// Get base URL (for external use)
    pub fn get_base_url(&self) -> String {
        self.base_url.read().clone()
    }

    /// Get authorization header value (for external use)
    pub fn get_auth_header(&self) -> Option<String> {
        self.bearer_token
            .read()
            .as_ref()
            .map(|t| format!("Bearer {}", t))
    }

    /// Check if the API is available
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url.read());

        let mut request = self.client.get(&url);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        match request.send().await {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                log::warn!("VLM API not available: {}", e);
                false
            }
        }
    }

    /// Check if vision models are available
    pub async fn has_vision_model(&self) -> Result<bool, String> {
        let url = format!("{}/api/tags", self.base_url.read());

        let mut request = self.client.get(&url);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        let resp = request
            .send()
            .await
            .map_err(|e| format!("Failed to connect to VLM API: {}", e))?;

        if resp.status() == 401 {
            return Err("Unauthorized: Invalid or missing bearer token".to_string());
        }

        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse API response: {}", e))?;

        let primary = self.model_primary.read().clone();
        let has_model = tags.models.iter().any(|m| {
            m.name
                .starts_with(&primary.split(':').next().unwrap_or(&primary))
        });

        Ok(has_model)
    }

    /// Analyze a screenshot using VLM with retry logic
    pub async fn analyze_frame(
        &self,
        image_path: &str,
        prompt: &str,
    ) -> Result<ActivityContext, String> {
        // Read and encode image as base64
        let image_data =
            std::fs::read(image_path).map_err(|e| format!("Failed to read image: {}", e))?;
        let base64_image = base64::engine::general_purpose::STANDARD.encode(&image_data);

        // Try primary model first
        let primary_model = self.model_primary.read().clone();
        match self
            .call_chat_api(&base64_image, prompt, &primary_model)
            .await
        {
            Ok(response) => return self.parse_response(&response, &primary_model),
            Err(e) => {
                log::warn!(
                    "Primary model {} failed: {}, trying fallback",
                    primary_model,
                    e
                );
            }
        }

        // Fallback to 3B model
        let fallback_model = self.model_fallback.read().clone();
        let response = self
            .call_chat_api(&base64_image, prompt, &fallback_model)
            .await?;
        self.parse_response(&response, &fallback_model)
    }

    /// Call the /api/chat endpoint with retry
    async fn call_chat_api(
        &self,
        image_b64: &str,
        prompt: &str,
        model: &str,
    ) -> Result<String, String> {
        let url = format!("{}/api/chat", self.base_url.read());

        let request_body = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                images: Some(vec![image_b64.to_string()]),
            }],
            stream: false,
            options: Some(ChatOptions {
                temperature: 0.1,
                max_tokens: Some(800),
            }),
        };

        // Retry with exponential backoff
        let mut last_error = String::new();
        for attempt in 0..3 {
            if attempt > 0 {
                let delay = Duration::from_millis(1000 * 2u64.pow(attempt));
                tokio::time::sleep(delay).await;
            }

            let mut request = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");

            if let Some(auth) = self.get_auth_header() {
                request = request.header("Authorization", auth);
            }

            match request.json(&request_body).send().await {
                Ok(resp) => {
                    if resp.status() == 401 {
                        return Err("Unauthorized: Invalid or missing bearer token".to_string());
                    }
                    if resp.status() == 503 {
                        last_error = "Service unavailable, retrying...".to_string();
                        continue;
                    }
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(format!("API error {}: {}", status, body));
                    }

                    let chat_resp: ChatResponse = resp
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse response: {}", e))?;

                    log::info!("VLM analysis complete using {}", model);
                    return Ok(chat_resp.message.content);
                }
                Err(e) => {
                    last_error = format!("Request failed: {}", e);
                    continue;
                }
            }
        }

        Err(format!("VLM API failed after 3 attempts: {}", last_error))
    }

    /// Parse VLM response into ActivityContext
    fn parse_response(&self, response: &str, model: &str) -> Result<ActivityContext, String> {
        // Try to extract JSON from response
        let json_start = response.find('{');
        let json_end = response.rfind('}');

        if let (Some(start), Some(end)) = (json_start, json_end) {
            let json_str = &response[start..=end];
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Ok(ActivityContext {
                    app_name: parsed
                        .get("app_name")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    window_title: parsed
                        .get("window_title")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    category: parsed
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    summary: parsed
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&response[..response.len().min(200)])
                        .to_string(),
                    focus_area: parsed
                        .get("focus_area")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    visible_files: parsed
                        .get("visible_files")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    confidence: parsed
                        .get("confidence")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.7) as f32,
                    entities: parsed.get("entities").cloned(),
                });
            }
        }

        // Fallback: create context from raw response
        Ok(ActivityContext {
            app_name: None,
            window_title: None,
            category: "unknown".to_string(),
            summary: response[..response.len().min(200)].to_string(),
            focus_area: None,
            visible_files: vec![],
            confidence: 0.5,
            entities: None,
        })
    }

    /// Simple text chat (no image)
    pub async fn chat(&self, prompt: &str) -> Result<String, String> {
        let url = format!("{}/api/chat", self.base_url.read());
        let model = self.model_primary.read().clone();

        let request_body = ChatRequest {
            model,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                images: None,
            }],
            stream: false,
            options: Some(ChatOptions {
                temperature: 0.3,
                max_tokens: Some(500),
            }),
        };

        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        let resp = request
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Chat request failed: {}", e))?;

        if resp.status() == 401 {
            return Err("Unauthorized: Invalid or missing bearer token".to_string());
        }

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Chat API error: {}", body));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(chat_resp.message.content)
    }
}

impl Default for VLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for VLMClient {
    fn clone(&self) -> Self {
        Self {
            base_url: Arc::clone(&self.base_url),
            bearer_token: Arc::clone(&self.bearer_token),
            model_primary: Arc::clone(&self.model_primary),
            model_fallback: Arc::clone(&self.model_fallback),
            client: self.client.clone(),
        }
    }
}

// ============================================================================
// Standalone Functions (for commands.rs compatibility)
// ============================================================================

use std::sync::OnceLock;

static VLM_CLIENT: OnceLock<VLMClient> = OnceLock::new();

/// Get or initialize the global VLM client
fn get_client() -> &'static VLMClient {
    VLM_CLIENT.get_or_init(VLMClient::new)
}

/// Configure the global VLM client
pub fn vlm_configure(base_url: &str, token: Option<&str>) {
    let client = get_client();
    client.set_base_url(base_url.to_string());
    if let Some(t) = token {
        client.set_bearer_token(t.to_string());
    }
}

/// Check if VLM API is available
pub async fn vlm_is_available() -> bool {
    get_client().is_available().await
}

/// Check if vision model is available
pub async fn vlm_has_vision_model() -> Result<bool, String> {
    get_client().has_vision_model().await
}

/// Analyze a single frame
pub async fn vlm_analyze_frame(image_path: &str, prompt: &str) -> Result<ActivityContext, String> {
    get_client().analyze_frame(image_path, prompt).await
}

/// Analyze multiple frames (batch)
pub async fn vlm_analyze_frames_batch(
    frames: Vec<(String, String)>, // (path, prompt) pairs
) -> Vec<Result<ActivityContext, String>> {
    let mut results = Vec::new();
    for (path, prompt) in frames {
        results.push(vlm_analyze_frame(&path, &prompt).await);
    }
    results
}

/// Simple text chat (no image)
pub async fn vlm_chat(prompt: &str) -> Result<String, String> {
    get_client().chat(prompt).await
}

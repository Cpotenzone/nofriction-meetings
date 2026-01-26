// noFriction Meetings - AI Client (Centralized API Integration)
// Provides AI capabilities for meeting analysis using centralized qwen2.5vl API

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// AI Model preset for different use cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model: String,
    pub system_prompt: String,
    pub temperature: f32,
}

/// Default presets for common meeting tasks
impl AIPreset {
    pub fn summarize() -> Self {
        Self {
            id: "summarize".to_string(),
            name: "Summarize Meeting".to_string(),
            description: "Generate a concise summary of the meeting".to_string(),
            model: "qwen2.5vl:7b".to_string(),
            system_prompt: r#"You are a professional meeting assistant. Your task is to summarize meeting content concisely and accurately.
Focus on:
- Key discussion points
- Decisions made
- Important numbers, dates, or commitments mentioned
- Overall sentiment and tone

Keep summaries clear and actionable. Use bullet points when appropriate."#.to_string(),
            temperature: 0.3,
        }
    }

    pub fn action_items() -> Self {
        Self {
            id: "action_items".to_string(),
            name: "Extract Action Items".to_string(),
            description: "Identify tasks and action items from the meeting".to_string(),
            model: "qwen2.5vl:7b".to_string(),
            system_prompt: r#"You are a task extraction assistant. Your job is to identify action items, tasks, and commitments from meeting content.
For each action item, extract:
- The task description
- Who is responsible (if mentioned)
- Due date or timeline (if mentioned)
- Priority level based on context

Format as a clear, actionable checklist."#.to_string(),
            temperature: 0.2,
        }
    }

    pub fn qa() -> Self {
        Self {
            id: "qa".to_string(),
            name: "Q&A Assistant".to_string(),
            description: "Answer questions about the meeting content".to_string(),
            model: "qwen2.5vl:7b".to_string(),
            system_prompt: r#"You are a helpful meeting assistant with access to meeting transcripts and screen content.
Answer questions based solely on the meeting content provided. If the answer isn't in the content, say so.
Be precise and cite specific parts of the meeting when relevant."#.to_string(),
            temperature: 0.5,
        }
    }

    pub fn get_all_presets() -> Vec<AIPreset> {
        vec![Self::summarize(), Self::action_items(), Self::qa()]
    }
}

/// Ollama model info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: String,
    pub modified_at: String,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", or "system"
    pub content: String,
}

/// Ollama API response for model list
#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    name: String,
    size: i64,
    modified_at: String,
}

/// Ollama chat response
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

/// AI Client for Centralized API (via SSH tunnel)
pub struct AIClient {
    base_url: Arc<RwLock<String>>,
    bearer_token: Arc<RwLock<Option<String>>>,
    client: reqwest::Client,
}

impl AIClient {
    pub fn new() -> Self {
        Self {
            base_url: Arc::new(RwLock::new("http://localhost:8080".to_string())),
            bearer_token: Arc::new(RwLock::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap(),
        }
    }

    pub fn with_url(base_url: &str) -> Self {
        Self {
            base_url: Arc::new(RwLock::new(base_url.to_string())),
            bearer_token: Arc::new(RwLock::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap(),
        }
    }

    /// Configure the client with URL and token
    pub fn configure(&self, base_url: &str, token: Option<&str>) {
        *self.base_url.write() = base_url.to_string();
        *self.bearer_token.write() = token.map(|s| s.to_string());
    }

    pub fn set_base_url(&self, url: String) {
        *self.base_url.write() = url;
    }

    pub fn set_bearer_token(&self, token: String) {
        *self.bearer_token.write() = Some(token);
    }

    /// Get authorization header if token is set
    fn get_auth_header(&self) -> Option<String> {
        self.bearer_token
            .read()
            .as_ref()
            .map(|t| format!("Bearer {}", t))
    }

    /// Check if API is available
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url.read());

        let mut request = self.client.get(&url);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        match request.send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get list of available models
    pub async fn list_models(&self) -> Result<Vec<OllamaModel>, String> {
        let url = format!("{}/api/tags", self.base_url.read());

        let mut request = self.client.get(&url);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to connect to API: {}", e))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized: Invalid or missing bearer token".to_string());
        }

        if !response.status().is_success() {
            return Err(format!("API returned error: {}", response.status()));
        }

        let data: OllamaModelsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data
            .models
            .into_iter()
            .map(|m| OllamaModel {
                name: m.name,
                size: format_size(m.size),
                modified_at: m.modified_at,
            })
            .collect())
    }

    /// Chat with a model using a preset
    pub async fn chat(
        &self,
        preset: &AIPreset,
        messages: Vec<ChatMessage>,
        context: Option<&str>,
    ) -> Result<String, String> {
        let url = format!("{}/api/chat", self.base_url.read());

        // Build messages with system prompt and context
        let mut chat_messages = vec![serde_json::json!({
            "role": "system",
            "content": &preset.system_prompt
        })];

        // Add context if provided
        if let Some(ctx) = context {
            chat_messages.push(serde_json::json!({
                "role": "system",
                "content": format!("Here is the meeting content for reference:\n\n{}", ctx)
            }));
        }

        // Add user messages
        for msg in messages {
            chat_messages.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        let body = serde_json::json!({
            "model": &preset.model,
            "messages": chat_messages,
            "stream": false,
            "options": {
                "temperature": preset.temperature
            }
        });

        let mut request = self.client.post(&url).json(&body);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized: Invalid or missing bearer token".to_string());
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, text));
        }

        let data: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data.message.content)
    }

    /// Quick summarize helper
    pub async fn summarize(&self, content: &str) -> Result<String, String> {
        let preset = AIPreset::summarize();
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Please summarize this meeting.".to_string(),
        }];
        self.chat(&preset, messages, Some(content)).await
    }

    /// Quick action items helper
    pub async fn extract_action_items(&self, content: &str) -> Result<String, String> {
        let preset = AIPreset::action_items();
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Please extract the action items from this meeting.".to_string(),
        }];
        self.chat(&preset, messages, Some(content)).await
    }

    /// Generic prompt completion
    pub async fn complete(&self, prompt: &str) -> Result<String, String> {
        let url = format!("{}/api/generate", self.base_url.read());

        let body = serde_json::json!({
            "model": "qwen2.5vl:7b",
            "prompt": prompt,
            "stream": false
        });

        let mut request = self.client.post(&url).json(&body);
        if let Some(auth) = self.get_auth_header() {
            request = request.header("Authorization", auth);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to call API: {}", e))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized: Invalid or missing bearer token".to_string());
        }

        if !response.status().is_success() {
            return Err(format!("API returned error: {}", response.status()));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        result
            .get("response")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No response from API".to_string())
    }
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

impl Default for AIClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AIClient {
    fn clone(&self) -> Self {
        Self {
            base_url: Arc::clone(&self.base_url),
            bearer_token: Arc::clone(&self.bearer_token),
            client: self.client.clone(),
        }
    }
}

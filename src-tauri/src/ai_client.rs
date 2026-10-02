// noFriction Meetings - AI Client facade
//
// Every text AI call in the app goes through here. `AIClient` keeps its old
// API (chat / complete / summarize / extract_action_items) but now delegates
// to the user's active text provider (crate::ai): OpenAI by default, any
// preset in docs/AI_PROVIDERS.md, or a local Ollama / LM Studio / custom
// OpenAI-compatible server. No servers of ours, no hardcoded hosts or keys.

use crate::ai::{self, Msg, Opts, Role};
use serde::{Deserialize, Serialize};

/// Output cap for general answers. Always sent (guardrail).
pub const DEFAULT_MAX_TOKENS: u32 = 1_500;
/// Structured JSON reports need more room.
pub const REPORT_MAX_TOKENS: u32 = 2_500;

/// AI prompt preset for different use cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Legacy field (the model now comes from Settings → AI Engine)
    pub model: String,
    pub system_prompt: String,
    pub temperature: f32,
}

impl AIPreset {
    pub fn summarize() -> Self {
        Self {
            id: "summarize".to_string(),
            name: "Summarize Meeting".to_string(),
            description: "Generate a concise summary of the meeting".to_string(),
            model: String::new(),
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
            model: String::new(),
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
            model: String::new(),
            system_prompt: r#"You are a helpful meeting assistant with access to meeting transcripts and screen content.
Answer questions based solely on the meeting content provided. If the answer isn't in the content, say so.
Be precise and cite specific parts of the meeting when relevant."#.to_string(),
            temperature: 0.5,
        }
    }

    pub fn get_all_presets() -> Vec<AIPreset> {
        vec![Self::summarize(), Self::action_items(), Self::qa()]
    }

    /// Construct an AIPreset from a database Prompt record
    pub fn from_prompt(prompt: &crate::prompt_manager::Prompt) -> Self {
        Self {
            id: prompt.id.clone(),
            name: prompt.name.clone(),
            description: prompt.description.clone().unwrap_or_default(),
            model: String::new(),
            system_prompt: prompt.system_prompt.clone(),
            temperature: prompt.temperature,
        }
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", or "system"
    pub content: String,
}

/// Stateless handle to the active text provider. Cheap to create and clone.
#[derive(Debug, Clone, Default)]
pub struct AIClient;

impl AIClient {
    pub fn new() -> Self {
        Self
    }

    /// A text provider is selected, configured and allowed (no network call).
    pub async fn is_available(&self) -> bool {
        ai::is_ready(ai::Kind::Text)
    }

    /// Chat with a preset (system prompt + optional meeting context).
    pub async fn chat(
        &self,
        preset: &AIPreset,
        messages: Vec<ChatMessage>,
        context: Option<&str>,
    ) -> Result<String, String> {
        let mut msgs = vec![Msg::system(preset.system_prompt.clone())];
        if let Some(ctx) = context.filter(|c| !c.trim().is_empty()) {
            msgs.push(Msg::system(format!("Here is the meeting content for reference:\n\n{}", ctx)));
        }
        msgs.extend(messages.into_iter().map(|m| Msg::new(Role::parse(&m.role), m.content)));
        ai::complete_text(
            msgs,
            Opts { max_tokens: DEFAULT_MAX_TOKENS, temperature: Some(preset.temperature) },
        )
        .await
        .map_err(String::from)
    }

    /// Quick summarize helper
    pub async fn summarize(&self, content: &str) -> Result<String, String> {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Please summarize this meeting.".to_string(),
        }];
        self.chat(&AIPreset::summarize(), messages, Some(content)).await
    }

    /// Quick action items helper
    pub async fn extract_action_items(&self, content: &str) -> Result<String, String> {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Please extract the action items from this meeting.".to_string(),
        }];
        self.chat(&AIPreset::action_items(), messages, Some(content)).await
    }

    /// Generic single-prompt completion.
    pub async fn complete(&self, prompt: &str) -> Result<String, String> {
        self.complete_with(None, prompt, REPORT_MAX_TOKENS, 0.3).await
    }

    /// Single prompt with an optional system prompt and explicit limits.
    pub async fn complete_with(
        &self,
        system: Option<&str>,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String, String> {
        let mut msgs = Vec::new();
        if let Some(s) = system.filter(|s| !s.trim().is_empty()) {
            msgs.push(Msg::system(s.to_string()));
        }
        msgs.push(Msg::user(prompt.to_string()));
        ai::complete_text(msgs, Opts { max_tokens, temperature: Some(temperature) })
            .await
            .map_err(String::from)
    }
}

/// True if an error string from the AI layer means "ask the user for consent".
pub fn is_consent_error(e: &str) -> bool {
    e.starts_with("CONSENT_REQUIRED:")
}

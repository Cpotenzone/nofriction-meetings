// noFriction Meetings - Dork Mode (Study Mode)
// Observes study sessions and generates AI study materials at session end

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::ai_client::AIClient;

/// Study materials generated at session end
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StudyMaterials {
    pub session_id: String,
    pub summary: String,
    pub concepts: Vec<KeyConcept>,
    pub quiz: Vec<QuizQuestion>,
    pub created_at: DateTime<Utc>,
}

/// A key concept extracted from the study session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConcept {
    pub term: String,
    pub definition: String,
}

/// A quiz question generated from study content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
}

/// Dork Mode session tracker
pub struct DorkModeSession {
    pub session_id: String,
    pub start_time: DateTime<Utc>,
    pub content_buffer: RwLock<Vec<String>>,
    pub is_active: RwLock<bool>,
}

impl DorkModeSession {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            start_time: Utc::now(),
            content_buffer: RwLock::new(Vec::new()),
            is_active: RwLock::new(true),
        }
    }

    /// Accumulate transcript text during the session
    pub fn accumulate_content(&self, text: &str) {
        if !text.trim().is_empty() && *self.is_active.read() {
            self.content_buffer.write().push(text.to_string());
        }
    }

    /// Get all accumulated content as a single string
    pub fn get_all_content(&self) -> String {
        self.content_buffer.read().join("\n")
    }

    /// End the session
    pub fn end_session(&self) {
        *self.is_active.write() = false;
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        *self.is_active.read()
    }
}

/// Generate study materials from accumulated content using AI
pub async fn generate_study_materials(
    ai_client: &AIClient,
    session_id: &str,
    content: &str,
) -> Result<StudyMaterials, String> {
    if content.trim().is_empty() {
        return Err("No content to generate study materials from".to_string());
    }

    log::info!(
        "📚 Generating study materials for session {} ({} chars)",
        session_id,
        content.len()
    );

    // Generate summary
    let summary = generate_summary(ai_client, content).await?;

    // Extract key concepts
    let concepts = extract_concepts(ai_client, content).await?;

    // Generate quiz questions
    let quiz = generate_quiz(ai_client, content).await?;

    Ok(StudyMaterials {
        session_id: session_id.to_string(),
        summary,
        concepts,
        quiz,
        created_at: Utc::now(),
    })
}

async fn generate_summary(ai_client: &AIClient, content: &str) -> Result<String, String> {
    let prompt = format!(
        r#"You are a study assistant. Summarize the following study session content into a clear, structured summary. Use bullet points for key topics and include any important details mentioned.

STUDY SESSION CONTENT:
{}

Provide a well-organized summary that would help a student review what they learned."#,
        content
    );

    ai_client.complete(&prompt).await
}

async fn extract_concepts(ai_client: &AIClient, content: &str) -> Result<Vec<KeyConcept>, String> {
    let prompt = format!(
        r#"You are a study assistant. Extract the key terms and concepts from this study session. For each term, provide a clear, concise definition.

STUDY SESSION CONTENT:
{}

Return your response as a JSON array of objects with "term" and "definition" fields. Example:
[{{"term": "Photosynthesis", "definition": "The process by which plants convert sunlight into energy"}}]

Return ONLY the JSON array, no other text."#,
        content
    );

    let response = ai_client.complete(&prompt).await?;

    // Parse JSON response
    let concepts: Vec<KeyConcept> = serde_json::from_str(&response).map_err(|e| {
        log::warn!(
            "Failed to parse concepts JSON: {}. Response: {}",
            e,
            response
        );
        // Try to extract manually if JSON parsing fails
        "Failed to parse concepts".to_string()
    })?;

    Ok(concepts)
}

async fn generate_quiz(ai_client: &AIClient, content: &str) -> Result<Vec<QuizQuestion>, String> {
    let prompt = format!(
        r#"You are a study assistant. Generate 5 multiple-choice quiz questions based on this study session content. Each question should test understanding of key concepts.

STUDY SESSION CONTENT:
{}

Return your response as a JSON array of objects with these fields:
- "question": The question text
- "options": Array of 4 possible answers
- "correct_index": Index (0-3) of the correct answer
- "explanation": Brief explanation of why the answer is correct

Example:
[{{"question": "What is the primary function of mitochondria?", "options": ["Cell division", "Energy production", "Protein synthesis", "Waste removal"], "correct_index": 1, "explanation": "Mitochondria are the powerhouse of the cell, producing ATP through cellular respiration."}}]

Return ONLY the JSON array, no other text."#,
        content
    );

    let response = ai_client.complete(&prompt).await?;

    // Parse JSON response
    let quiz: Vec<QuizQuestion> = serde_json::from_str(&response).map_err(|e| {
        log::warn!("Failed to parse quiz JSON: {}. Response: {}", e, response);
        "Failed to parse quiz".to_string()
    })?;

    Ok(quiz)
}

impl Default for DorkModeSession {
    fn default() -> Self {
        Self::new(uuid::Uuid::new_v4().to_string())
    }
}

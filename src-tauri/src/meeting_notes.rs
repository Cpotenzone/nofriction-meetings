// noFriction Meetings - Meeting Notes Generator
// AI-powered meeting analysis and notes generation

use crate::ai_client::AIClient;
use crate::database::DatabaseManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Generated meeting notes structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedNotes {
    pub summary: String,
    pub key_topics: Vec<String>,
    pub decisions: Vec<Decision>,
    pub action_items: Vec<ActionItem>,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub text: String,
    pub made_by: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub task: String,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub priority: Option<String>,
}

/// Meeting Notes Generator
pub struct MeetingNotesGenerator {
    ai_client: AIClient,
}

impl MeetingNotesGenerator {
    pub fn new(ai_client: AIClient) -> Self {
        Self { ai_client }
    }

    /// Generate notes from meeting transcripts
    pub async fn generate_notes(
        &self,
        meeting_id: &str,
        database: &Arc<DatabaseManager>,
    ) -> Result<GeneratedNotes, String> {
        // Get all transcripts for the meeting
        let transcripts = database
            .get_transcripts(meeting_id)
            .await
            .map_err(|e| format!("Failed to get transcripts: {}", e))?;

        if transcripts.is_empty() {
            return Err("No transcripts found for this meeting".to_string());
        }

        // Combine transcripts into a single text block
        let full_transcript: String = transcripts
            .iter()
            .map(|t| {
                if let Some(ref speaker) = t.speaker {
                    format!("{}: {}", speaker, t.text)
                } else {
                    t.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Generate notes using AI
        let notes = self.analyze_transcript(&full_transcript).await?;

        // Save to database
        let notes_id = Uuid::new_v4().to_string();
        let key_topics_json = serde_json::to_string(&notes.key_topics).unwrap_or_default();
        let decisions_json = serde_json::to_string(&notes.decisions).unwrap_or_default();
        let action_items_json = serde_json::to_string(&notes.action_items).unwrap_or_default();
        let participants_json = serde_json::to_string(&notes.participants).unwrap_or_default();

        database
            .save_meeting_notes(
                &notes_id,
                meeting_id,
                Some(&notes.summary),
                Some(&key_topics_json),
                Some(&decisions_json),
                Some(&action_items_json),
                Some(&participants_json),
                Some("default"),
            )
            .await
            .map_err(|e| format!("Failed to save notes: {}", e))?;

        Ok(notes)
    }

    /// Analyze transcript and extract structured notes
    async fn analyze_transcript(&self, transcript: &str) -> Result<GeneratedNotes, String> {
        let prompt = format!(
            r#"Analyze this meeting transcript and extract:
1. A brief summary (2-3 sentences)
2. Key topics discussed (list of 3-7 topics)
3. Decisions made (who decided what)
4. Action items (task, assignee if mentioned, priority)
5. Participants mentioned

Return as JSON:
{{
  "summary": "...",
  "key_topics": ["topic1", "topic2"],
  "decisions": [{{"text": "...", "made_by": "...", "context": "..."}}],
  "action_items": [{{"task": "...", "assignee": "...", "priority": "high/medium/low"}}],
  "participants": ["name1", "name2"]
}}

TRANSCRIPT:
{}

JSON RESPONSE:"#,
            transcript.chars().take(8000).collect::<String>()
        );

        let response = self
            .ai_client
            .complete(&prompt)
            .await
            .map_err(|e| format!("AI analysis failed: {}", e))?;

        // Parse JSON response
        let notes: GeneratedNotes = serde_json::from_str(&response).map_err(|e| {
            format!(
                "Failed to parse AI response: {} - Response: {}",
                e, response
            )
        })?;

        Ok(notes)
    }

    /// Generate a quick summary (faster, less detailed)
    pub async fn generate_quick_summary(&self, transcript: &str) -> Result<String, String> {
        let prompt = format!(
            "Summarize this meeting in 2-3 sentences:\n\n{}",
            transcript.chars().take(4000).collect::<String>()
        );

        self.ai_client
            .complete(&prompt)
            .await
            .map_err(|e| format!("Summary generation failed: {}", e))
    }

    /// Extract action items only
    pub async fn extract_action_items(&self, transcript: &str) -> Result<Vec<ActionItem>, String> {
        let prompt = format!(
            r#"Extract action items from this meeting transcript. Return as JSON array:
[{{"task": "...", "assignee": "...", "priority": "high/medium/low"}}]

TRANSCRIPT:
{}

JSON ARRAY:"#,
            transcript.chars().take(6000).collect::<String>()
        );

        let response = self
            .ai_client
            .complete(&prompt)
            .await
            .map_err(|e| format!("Action item extraction failed: {}", e))?;

        serde_json::from_str(&response).map_err(|e| format!("Failed to parse action items: {}", e))
    }
}

/// Cluster transcripts into logical segments based on time gaps and topic similarity
pub fn cluster_transcripts_by_time(
    transcripts: &[crate::database::Transcript],
    gap_threshold_seconds: i64,
) -> Vec<Vec<usize>> {
    if transcripts.is_empty() {
        return vec![];
    }

    let mut clusters: Vec<Vec<usize>> = vec![vec![0]];

    for i in 1..transcripts.len() {
        let prev_ts = transcripts[i - 1].timestamp;
        let curr_ts = transcripts[i].timestamp;
        let gap = (curr_ts - prev_ts).num_seconds();

        if gap > gap_threshold_seconds {
            // Start a new cluster
            clusters.push(vec![i]);
        } else {
            // Add to current cluster
            clusters.last_mut().unwrap().push(i);
        }
    }

    clusters
}

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
        let full_transcript = transcript_for_prompt(&transcripts);

        // Generate notes using AI, in the recording type's style (meeting,
        // lecture or personal notes; recording_kind.rs)
        let (kind, notebook) = database.get_kind_and_notebook(meeting_id).await.unwrap_or_default();
        let (prompt, label) = crate::recording_kind::report_prompt(
            kind,
            notebook.as_deref(),
            crate::settings::DEFAULT_REPORT_PROMPT,
            "default",
        );
        let notes = self.analyze_transcript(&full_transcript, Some(&prompt)).await?;

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
                Some(label),
            )
            .await
            .map_err(|e| format!("Failed to save notes: {}", e))?;

        Ok(notes)
    }

    /// Analyze transcript and extract structured notes
    async fn analyze_transcript(
        &self,
        transcript: &str,
        custom_prompt: Option<&str>,
    ) -> Result<GeneratedNotes, String> {
        let base_prompt = custom_prompt.unwrap_or(crate::settings::DEFAULT_REPORT_PROMPT);
        let prompt = format!(
            "{}\n\nTRANSCRIPT:\n{}\n\nJSON RESPONSE:",
            base_prompt,
            transcript.chars().take(8000).collect::<String>()
        );

        let response = self
            .ai_client
            .complete(&prompt)
            .await
            .map_err(|e| format!("AI analysis failed: {}", e))?;

        // Try to extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = extract_json_from_response(&response);

        // Parse JSON response
        let notes: GeneratedNotes = serde_json::from_str(&json_str).map_err(|e| {
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

    /// Generate notes with a custom prompt (for configurable reports)
    pub async fn generate_notes_with_prompt(
        &self,
        meeting_id: &str,
        database: &Arc<DatabaseManager>,
        custom_prompt: &str,
    ) -> Result<GeneratedNotes, String> {
        let transcripts = database
            .get_transcripts(meeting_id)
            .await
            .map_err(|e| format!("Failed to get transcripts: {}", e))?;

        if transcripts.is_empty() {
            return Err("No transcripts found for this meeting".to_string());
        }

        let full_transcript = transcript_for_prompt(&transcripts);

        // A class gets lecture notes and a personal recording personal
        // notes instead of the meeting report prompt
        let (kind, notebook) = database.get_kind_and_notebook(meeting_id).await.unwrap_or_default();
        let (prompt, label) =
            crate::recording_kind::report_prompt(kind, notebook.as_deref(), custom_prompt, "auto-report");
        let notes = self.analyze_transcript(&full_transcript, Some(&prompt)).await?;

        let notes_id = uuid::Uuid::new_v4().to_string();
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
                Some(label),
            )
            .await
            .map_err(|e| format!("Failed to save notes: {}", e))?;

        Ok(notes)
    }
}

// ─── Follow-up email (same prompt rules as iOS MeetingAI.emailSystem) ───────

pub const FOLLOWUP_EMAIL_SYSTEM: &str = "Draft a short, friendly follow-up email to the people in this meeting: thank them, \
recap what was decided, and list next steps. Plain text, no Markdown. Start with a \
\"Subject:\" line. Only state facts that are in the transcript; never invent owners or dates. \
Text shown as [stricken from the record] was removed by the user: never guess at or mention what it said.";

/// A drafted follow-up email.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowUpEmail {
    pub subject: String,
    pub body: String,
    /// Attendee emails (calendar-linked meetings), excluding the user
    pub to: Vec<String>,
}

/// The user message for the follow-up email: meeting facts + transcript
/// (stricken spans already rendered as `[stricken from the record]`).
pub fn followup_context(
    title: &str,
    when: &str,
    attendees: &[(String, Option<String>)],
    transcript: &str,
) -> String {
    let mut s = format!("Meeting: {}\nWhen: {}\n", title.trim(), when);
    if !attendees.is_empty() {
        let list = attendees
            .iter()
            .map(|(name, company)| match company {
                Some(c) if !c.trim().is_empty() => format!("{} ({})", name, c.trim()),
                _ => name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("Attendees: {}\n", list));
    }
    s.push_str("\nTranscript:\n");
    s.push_str(transcript);
    s
}

/// Split a drafted email into (subject, body). Accepts "Subject: …" on the
/// first non-empty line (any case, optional Markdown bold); without one the
/// subject falls back to "Follow-up: <title>".
pub fn parse_followup_email(raw: &str, title: &str) -> (String, String) {
    let text = raw.trim().trim_start_matches("```").trim_end_matches("```").trim();
    let mut lines = text.lines();
    let mut subject = None;
    let mut rest: Vec<&str> = Vec::new();
    for line in lines.by_ref() {
        let l = line.trim().trim_matches('*').trim();
        if l.is_empty() {
            continue;
        }
        if l.len() >= 8 && l[..8].eq_ignore_ascii_case("subject:") {
            subject = Some(l[8..].trim().trim_matches('*').trim().to_string());
        } else {
            rest.push(line);
        }
        break;
    }
    rest.extend(lines);
    let body = rest.join("\n").trim().to_string();
    let subject = subject
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Follow-up: {}", title.trim()));
    (subject, body)
}

/// Extract JSON from an AI response that may contain markdown code blocks
/// The transcript block sent to the AI. `get_transcripts` already renders
/// stricken spans as `[stricken from the record]`; render again here so a
/// caller holding marked rows can never leak a marker id (or skip it).
pub fn transcript_for_prompt(transcripts: &[crate::database::Transcript]) -> String {
    transcripts
        .iter()
        .map(|t| {
            let text = crate::redaction::render_plain(&t.text);
            match t.speaker {
                Some(ref speaker) => format!("{}: {}", speaker, text),
                None => text,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_json_from_response(response: &str) -> String {
    let trimmed = response.trim();

    // Try to find JSON in markdown code block
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim().to_string();
        }
    }

    // Try backtick code block without language
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        if let Some(end) = trimmed[json_start..].find("```") {
            let content = trimmed[json_start..json_start + end].trim();
            if content.starts_with('{') {
                return content.to_string();
            }
        }
    }

    // Try to find raw JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
    }

    // Return as-is
    trimmed.to_string()
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

#[cfg(test)]
mod followup_tests {
    #[test]
    fn followup_email_parsing() {
        let (s, b) = super::parse_followup_email("Subject: Thanks for today\n\nHi all,\nThanks!", "Weekly");
        assert_eq!(s, "Thanks for today");
        assert_eq!(b, "Hi all,\nThanks!");
        let (s, b) = super::parse_followup_email("**Subject:** Recap\nBody", "Weekly");
        assert_eq!(s, "Recap");
        assert_eq!(b, "Body");
        let (s, b) = super::parse_followup_email("Hi team,\nthanks", "Weekly sync");
        assert_eq!(s, "Follow-up: Weekly sync");
        assert_eq!(b, "Hi team,\nthanks");
    }

    #[test]
    fn followup_context_lists_attendees_and_keeps_stricken_marker() {
        let c = super::followup_context(
            "Weekly",
            "Thu 2 Oct 2026 10:00",
            &[("Ana".into(), Some("Acme".into())), ("Bo".into(), None)],
            "We agreed to ship. [stricken from the record]",
        );
        assert!(c.starts_with("Meeting: Weekly\nWhen: Thu 2 Oct 2026 10:00\nAttendees: Ana (Acme), Bo\n"));
        assert!(c.ends_with("[stricken from the record]"));
        assert!(super::FOLLOWUP_EMAIL_SYSTEM.contains("never invent owners or dates"));
    }
}

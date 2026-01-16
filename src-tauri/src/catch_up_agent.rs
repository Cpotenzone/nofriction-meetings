// Catch-Up Agent
// Generates catch-up capsules for users joining meetings late

use serde::{Deserialize, Serialize};
use crate::ai_client::AIClient;

/// A citation pointing to a specific transcript moment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptCitation {
    pub segment_id: String,
    pub timestamp_ms: i64,
    pub speaker: Option<String>,
    pub text_excerpt: String,
}

/// An insight item with optional citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightItem {
    pub text: String,
    pub importance: f32,  // 0.0-1.0
    pub citation: Option<TranscriptCitation>,
}

/// A decision recorded during the meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub text: String,
    pub made_by: Option<String>,
    pub citation: Option<TranscriptCitation>,
}

/// A risk or landmine detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSignal {
    pub text: String,
    pub severity: f32,  // 0.0-1.0
    pub signal_type: RiskType,
    pub citation: Option<TranscriptCitation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskType {
    Tension,
    Disagreement,
    SensitiveTopic,
    Deadline,
    Blocker,
    Unknown,
}

/// The complete catch-up capsule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatchUpCapsule {
    pub what_missed: Vec<InsightItem>,
    pub current_topic: String,
    pub decisions: Vec<Decision>,
    pub open_threads: Vec<String>,
    pub next_moves: Vec<String>,
    pub risks: Vec<RiskSignal>,
    pub questions_to_ask: Vec<String>,
    pub ten_second_version: String,
    pub sixty_second_version: String,
    pub confidence: f32,
    pub citations: Vec<TranscriptCitation>,
    pub generated_at_minute: i32,
}

impl Default for CatchUpCapsule {
    fn default() -> Self {
        Self {
            what_missed: Vec::new(),
            current_topic: "Unknown".to_string(),
            decisions: Vec::new(),
            open_threads: Vec::new(),
            next_moves: Vec::new(),
            risks: Vec::new(),
            questions_to_ask: Vec::new(),
            ten_second_version: "Meeting in progress. No summary available yet.".to_string(),
            sixty_second_version: "Meeting in progress. Transcript data insufficient for summary.".to_string(),
            confidence: 0.0,
            citations: Vec::new(),
            generated_at_minute: 0,
        }
    }
}

/// Meeting metadata for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub title: String,
    pub description: Option<String>,
    pub attendees: Vec<String>,
    pub scheduled_duration_min: Option<i32>,
}

/// A transcript segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub timestamp_ms: i64,
    pub speaker: Option<String>,
    pub text: String,
}

/// Prior interaction history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySnippet {
    pub date: String,
    pub context: String,
    pub summary: String,
}

/// Catch-Up Agent for generating late-join summaries
pub struct CatchUpAgent {
    ai_client: AIClient,
}

impl CatchUpAgent {
    pub fn new(ai_client: AIClient) -> Self {
        Self { ai_client }
    }

    /// Generate a catch-up capsule from transcript data
    pub async fn generate(
        &self,
        transcript_segments: &[TranscriptSegment],
        meeting_metadata: &MeetingMetadata,
        minutes_since_start: i32,
        _prior_history: Option<&[HistorySnippet]>,
    ) -> Result<CatchUpCapsule, String> {
        // Build transcript text
        let transcript_text = self.build_transcript_text(transcript_segments);
        
        if transcript_text.is_empty() {
            return Ok(CatchUpCapsule {
                ten_second_version: "No transcript data available yet.".to_string(),
                sixty_second_version: "Meeting is in progress but no transcript has been captured. Start live capture to enable catch-up summaries.".to_string(),
                ..Default::default()
            });
        }

        // Build the prompt
        let prompt = self.build_catch_up_prompt(
            &transcript_text,
            meeting_metadata,
            minutes_since_start,
        );

        // Call AI
        let response = self.ai_client.complete(&prompt).await?;

        // Parse response into capsule
        let capsule = self.parse_catch_up_response(&response, minutes_since_start, transcript_segments)?;

        Ok(capsule)
    }

    fn build_transcript_text(&self, segments: &[TranscriptSegment]) -> String {
        segments
            .iter()
            .map(|s| {
                if let Some(ref speaker) = s.speaker {
                    format!("[{}] {}: {}", Self::format_timestamp(s.timestamp_ms), speaker, s.text)
                } else {
                    format!("[{}] {}", Self::format_timestamp(s.timestamp_ms), s.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_timestamp(ms: i64) -> String {
        let seconds = (ms / 1000) % 60;
        let minutes = (ms / 1000 / 60) % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }

    fn build_catch_up_prompt(
        &self,
        transcript: &str,
        metadata: &MeetingMetadata,
        minutes_since_start: i32,
    ) -> String {
        let attendees = if metadata.attendees.is_empty() {
            "Unknown".to_string()
        } else {
            metadata.attendees.join(", ")
        };

        format!(r#"You are analyzing a meeting transcript. The user just joined {minutes_since_start} minutes late and needs to quickly understand what happened.

MEETING: {title}
ATTENDEES: {attendees}
TRANSCRIPT SO FAR:
{transcript}

Generate a Catch-Up Capsule in this exact JSON format:
{{
  "what_missed": ["key point 1", "key point 2", "key point 3"],
  "current_topic": "what is being discussed right now",
  "decisions": ["decision 1 if any"],
  "open_threads": ["unresolved question 1", "unresolved question 2"],
  "next_moves": ["suggestion 1 for what to say/do", "suggestion 2"],
  "risks": ["any tension or sensitive topics detected"],
  "questions_to_ask": ["good question to ask based on discussion"],
  "ten_second_version": "3-4 ultra-short bullet points for quick scan",
  "sixty_second_version": "fuller summary paragraph",
  "confidence": 0.85
}}

HARD RULES:
- Be factual. Only cite what's actually in the transcript.
- If uncertain, use "Possibly:" prefix or omit entirely.
- No hallucination. If transcript is unclear, say so.
- Keep ten_second_version to 3-4 bullet points max.
- Make next_moves actionable and specific.

Return ONLY valid JSON, no other text."#,
            title = metadata.title,
            minutes_since_start = minutes_since_start,
            attendees = attendees,
            transcript = transcript,
        )
    }

    fn parse_catch_up_response(
        &self,
        response: &str,
        minutes_since_start: i32,
        _segments: &[TranscriptSegment],
    ) -> Result<CatchUpCapsule, String> {
        // Try to parse JSON from response
        let json_str = self.extract_json(response)?;
        
        #[derive(Deserialize)]
        struct RawCapsule {
            what_missed: Option<Vec<String>>,
            current_topic: Option<String>,
            decisions: Option<Vec<String>>,
            open_threads: Option<Vec<String>>,
            next_moves: Option<Vec<String>>,
            risks: Option<Vec<String>>,
            questions_to_ask: Option<Vec<String>>,
            ten_second_version: Option<String>,
            sixty_second_version: Option<String>,
            confidence: Option<f32>,
        }

        let raw: RawCapsule = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse AI response: {}", e))?;

        Ok(CatchUpCapsule {
            what_missed: raw.what_missed.unwrap_or_default()
                .into_iter()
                .map(|text| InsightItem { text, importance: 0.8, citation: None })
                .collect(),
            current_topic: raw.current_topic.unwrap_or_else(|| "Unknown".to_string()),
            decisions: raw.decisions.unwrap_or_default()
                .into_iter()
                .map(|text| Decision { text, made_by: None, citation: None })
                .collect(),
            open_threads: raw.open_threads.unwrap_or_default(),
            next_moves: raw.next_moves.unwrap_or_default(),
            risks: raw.risks.unwrap_or_default()
                .into_iter()
                .map(|text| RiskSignal {
                    text,
                    severity: 0.5,
                    signal_type: RiskType::Unknown,
                    citation: None,
                })
                .collect(),
            questions_to_ask: raw.questions_to_ask.unwrap_or_default(),
            ten_second_version: raw.ten_second_version
                .unwrap_or_else(|| "Summary not available".to_string()),
            sixty_second_version: raw.sixty_second_version
                .unwrap_or_else(|| "Summary not available".to_string()),
            confidence: raw.confidence.unwrap_or(0.5),
            citations: Vec::new(),
            generated_at_minute: minutes_since_start,
        })
    }

    fn extract_json(&self, response: &str) -> Result<String, String> {
        // Find JSON in response (may be wrapped in markdown code blocks)
        let trimmed = response.trim();
        
        // Check for code block
        if trimmed.contains("```json") {
            let start = trimmed.find("```json").unwrap() + 7;
            let end = trimmed[start..].find("```").map(|i| start + i).unwrap_or(trimmed.len());
            return Ok(trimmed[start..end].trim().to_string());
        }
        
        if trimmed.contains("```") {
            let start = trimmed.find("```").unwrap() + 3;
            let end = trimmed[start..].find("```").map(|i| start + i).unwrap_or(trimmed.len());
            return Ok(trimmed[start..end].trim().to_string());
        }

        // Try to find JSON object
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return Ok(trimmed[start..=end].to_string());
            }
        }

        Err("No JSON found in response".to_string())
    }
}

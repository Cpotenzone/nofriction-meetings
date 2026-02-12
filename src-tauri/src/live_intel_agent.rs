// Live Intelligence Agent
// Extracts real-time insights from incoming transcript segments

use crate::catch_up_agent::TranscriptSegment;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Types of live insight events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveInsightEvent {
    ActionItem {
        id: String,
        text: String,
        assignee: Option<String>,
        timestamp_ms: i64,
    },
    Decision {
        id: String,
        text: String,
        context: String,
        timestamp_ms: i64,
    },
    RiskSignal {
        id: String,
        text: String,
        severity: f32,
        timestamp_ms: i64,
    },
    QuestionSuggestion {
        id: String,
        text: String,
        reason: String,
        timestamp_ms: i64,
    },
    Commitment {
        id: String,
        text: String,
        by: Option<String>,
        timestamp_ms: i64,
    },
    TopicShift {
        id: String,
        from_topic: String,
        to_topic: String,
        timestamp_ms: i64,
    },
}

impl LiveInsightEvent {
    pub fn id(&self) -> &str {
        match self {
            LiveInsightEvent::ActionItem { id, .. } => id,
            LiveInsightEvent::Decision { id, .. } => id,
            LiveInsightEvent::RiskSignal { id, .. } => id,
            LiveInsightEvent::QuestionSuggestion { id, .. } => id,
            LiveInsightEvent::Commitment { id, .. } => id,
            LiveInsightEvent::TopicShift { id, .. } => id,
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            LiveInsightEvent::ActionItem { timestamp_ms, .. } => *timestamp_ms,
            LiveInsightEvent::Decision { timestamp_ms, .. } => *timestamp_ms,
            LiveInsightEvent::RiskSignal { timestamp_ms, .. } => *timestamp_ms,
            LiveInsightEvent::QuestionSuggestion { timestamp_ms, .. } => *timestamp_ms,
            LiveInsightEvent::Commitment { timestamp_ms, .. } => *timestamp_ms,
            LiveInsightEvent::TopicShift { timestamp_ms, .. } => *timestamp_ms,
        }
    }
}

/// Conversation state tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationState {
    pub current_topic: String,
    pub unresolved_questions: Vec<String>,
    pub sentiment_score: f32, // -1.0 to 1.0
    pub speaker_talk_time: std::collections::HashMap<String, i64>,
}

/// Live Intelligence Agent for real-time insight extraction
pub struct LiveIntelAgent {
    /// Rolling context window (2-5 minutes)
    context_window: VecDeque<TranscriptSegment>,
    /// Maximum segments to keep in context
    max_context_segments: usize,
    /// Current conversation state
    pub conversation_state: ConversationState,
    /// Generated insight counter
    insight_counter: u64,
    /// Emitted events
    emitted_events: Vec<LiveInsightEvent>,
}

impl LiveIntelAgent {
    pub fn new() -> Self {
        Self {
            context_window: VecDeque::new(),
            max_context_segments: 50, // ~5 minutes at typical speaking rate
            conversation_state: ConversationState::default(),
            insight_counter: 0,
            emitted_events: Vec::new(),
        }
    }

    /// Process a new transcript segment and extract insights
    pub fn process_segment(&mut self, segment: TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();

        // Add to context window
        self.context_window.push_back(segment.clone());

        // Trim context if too large
        while self.context_window.len() > self.max_context_segments {
            self.context_window.pop_front();
        }

        // Update speaker talk time
        if let Some(ref speaker) = segment.speaker {
            let entry = self
                .conversation_state
                .speaker_talk_time
                .entry(speaker.clone())
                .or_insert(0);
            *entry += segment.text.len() as i64;
        }

        // Extract insights from segment
        events.extend(self.detect_action_items(&segment));
        events.extend(self.detect_decisions(&segment));
        events.extend(self.detect_commitments(&segment));
        events.extend(self.detect_risks(&segment));
        events.extend(self.detect_topic_shifts(&segment));
        events.extend(self.generate_question_suggestions(&segment));

        // Store emitted events
        self.emitted_events.extend(events.clone());

        events
    }

    /// Get all events emitted so far
    pub fn get_all_events(&self) -> &[LiveInsightEvent] {
        &self.emitted_events
    }

    /// Clear context and reset state
    pub fn reset(&mut self) {
        self.context_window.clear();
        self.conversation_state = ConversationState::default();
        self.emitted_events.clear();
    }

    fn generate_id(&mut self, prefix: &str) -> String {
        self.insight_counter += 1;
        format!("{}_{}", prefix, self.insight_counter)
    }

    /// Detect action items in text
    fn detect_action_items(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        // Action item patterns
        let patterns = [
            ("can you", None),
            ("could you", None),
            ("please", None),
            ("need to", None),
            ("should", None),
            ("will you", None),
            ("action item", None),
            ("follow up", None),
            ("let's make sure", None),
            ("i'll take care of", Some("speaker")),
            ("i will", Some("speaker")),
            ("i can do", Some("speaker")),
        ];

        for (pattern, assignee_hint) in patterns {
            if text_lower.contains(pattern) {
                let assignee = match assignee_hint {
                    Some("speaker") => segment.speaker.clone(),
                    _ => None,
                };

                events.push(LiveInsightEvent::ActionItem {
                    id: self.generate_id("action"),
                    text: segment.text.clone(),
                    assignee,
                    timestamp_ms: segment.timestamp_ms,
                });
                break; // One action per segment
            }
        }

        events
    }

    /// Detect decisions
    fn detect_decisions(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        let patterns = [
            "we decided",
            "we agreed",
            "let's go with",
            "the decision is",
            "we're going to",
            "we'll do",
            "that's the plan",
            "sounds good, let's",
            "approved",
            "settled on",
        ];

        for pattern in patterns {
            if text_lower.contains(pattern) {
                events.push(LiveInsightEvent::Decision {
                    id: self.generate_id("decision"),
                    text: segment.text.clone(),
                    context: self.get_recent_context(3),
                    timestamp_ms: segment.timestamp_ms,
                });
                break;
            }
        }

        events
    }

    /// Detect commitments
    fn detect_commitments(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        let patterns = [
            "i commit",
            "i promise",
            "you have my word",
            "i guarantee",
            "i'll make sure",
            "count on me",
            "i'll get it done by",
        ];

        for pattern in patterns {
            if text_lower.contains(pattern) {
                events.push(LiveInsightEvent::Commitment {
                    id: self.generate_id("commit"),
                    text: segment.text.clone(),
                    by: segment.speaker.clone(),
                    timestamp_ms: segment.timestamp_ms,
                });
                break;
            }
        }

        events
    }

    /// Detect risks/issues
    fn detect_risks(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        let risk_patterns = [
            ("concern", 0.5),
            ("worried", 0.6),
            ("problem", 0.5),
            ("issue", 0.4),
            ("risk", 0.6),
            ("blocker", 0.7),
            ("blocked", 0.6),
            ("disagree", 0.5),
            ("frustrated", 0.7),
            ("deadline", 0.5),
            ("delayed", 0.6),
            ("not going to make it", 0.8),
            ("pushback", 0.5),
        ];

        for (pattern, severity) in risk_patterns {
            if text_lower.contains(pattern) {
                events.push(LiveInsightEvent::RiskSignal {
                    id: self.generate_id("risk"),
                    text: segment.text.clone(),
                    severity,
                    timestamp_ms: segment.timestamp_ms,
                });
                break;
            }
        }

        events
    }

    /// Detect topic shifts
    fn detect_topic_shifts(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        let topic_shift_patterns = [
            "let's move on to",
            "moving on",
            "next topic",
            "switching gears",
            "let's talk about",
            "onto the next",
            "can we discuss",
        ];

        for pattern in topic_shift_patterns {
            if text_lower.contains(pattern) {
                let old_topic = self.conversation_state.current_topic.clone();
                let new_topic = self.extract_topic(&segment.text);

                if !new_topic.is_empty() && new_topic != old_topic {
                    self.conversation_state.current_topic = new_topic.clone();

                    events.push(LiveInsightEvent::TopicShift {
                        id: self.generate_id("topic"),
                        from_topic: old_topic,
                        to_topic: new_topic,
                        timestamp_ms: segment.timestamp_ms,
                    });
                }
                break;
            }
        }

        events
    }

    /// Generate question suggestions
    fn generate_question_suggestions(
        &mut self,
        segment: &TranscriptSegment,
    ) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();
        let text_lower = segment.text.to_lowercase();

        // Detect incomplete or unclear statements
        let unclear_patterns = [
            ("i'm not sure", "Can you clarify what you mean?"),
            ("maybe", "What would help you decide?"),
            ("we should probably", "What's the specific timeline?"),
            ("at some point", "When specifically should this happen?"),
            ("someone should", "Who specifically will own this?"),
        ];

        for (pattern, suggestion) in unclear_patterns {
            if text_lower.contains(pattern) {
                events.push(LiveInsightEvent::QuestionSuggestion {
                    id: self.generate_id("question"),
                    text: suggestion.to_string(),
                    reason: format!(
                        "Based on: \"{}\"",
                        segment.text.chars().take(50).collect::<String>()
                    ),
                    timestamp_ms: segment.timestamp_ms,
                });
                break;
            }
        }

        events
    }

    fn get_recent_context(&self, n: usize) -> String {
        self.context_window
            .iter()
            .rev()
            .take(n)
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_topic(&self, text: &str) -> String {
        // Simple extraction - take text after "about" or "to"
        let lower = text.to_lowercase();
        for marker in ["about ", "to discuss ", "discuss "] {
            if let Some(idx) = lower.find(marker) {
                let start = idx + marker.len();
                let topic: String = text[start..]
                    .chars()
                    .take(50)
                    .take_while(|c| *c != '.' && *c != ',' && *c != '?')
                    .collect();
                return topic.trim().to_string();
            }
        }
        String::new()
    }

    /// AI-powered batch analysis of transcript segments
    /// Uses the persona-specific live_intel_system prompt for deep extraction
    pub async fn ai_analyze(
        &mut self,
        segments: &[TranscriptSegment],
        system_prompt: &str,
    ) -> Vec<LiveInsightEvent> {
        let ai_client = crate::ai_client::AIClient::new();

        // Build transcript text from segments
        let transcript_text: String = segments
            .iter()
            .map(|s| {
                if let Some(ref speaker) = s.speaker {
                    format!("[{}ms] {}: {}", s.timestamp_ms, speaker, s.text)
                } else {
                    format!("[{}ms] {}", s.timestamp_ms, s.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if transcript_text.is_empty() {
            return Vec::new();
        }

        let prompt = format!(
            "{}\n\nTRANSCRIPT SEGMENTS:\n{}\n\nJSON RESPONSE:",
            system_prompt, transcript_text
        );

        let response = match ai_client.complete(&prompt).await {
            Ok(r) => r,
            Err(e) => {
                log::warn!(
                    "AI live intel analysis failed, using rule-based only: {}",
                    e
                );
                return Vec::new();
            }
        };

        // Parse AI response into events
        self.parse_ai_insights(&response, segments)
    }

    /// Parse AI JSON response into LiveInsightEvent variants
    fn parse_ai_insights(
        &mut self,
        response: &str,
        segments: &[TranscriptSegment],
    ) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();

        // Extract JSON from response
        let json_str = {
            let trimmed = response.trim();
            if trimmed.contains("```json") {
                let start = trimmed.find("```json").unwrap() + 7;
                let end = trimmed[start..]
                    .find("```")
                    .map(|i| start + i)
                    .unwrap_or(trimmed.len());
                trimmed[start..end].trim().to_string()
            } else if let Some(start) = trimmed.find('{') {
                if let Some(end) = trimmed.rfind('}') {
                    trimmed[start..=end].to_string()
                } else {
                    return events;
                }
            } else {
                return events;
            }
        };

        // Parse the JSON
        #[derive(serde::Deserialize, Default)]
        struct AiInsights {
            #[serde(default)]
            action_items: Vec<AiActionItem>,
            #[serde(default)]
            decisions: Vec<AiDecision>,
            #[serde(default)]
            risks: Vec<AiRisk>,
            #[serde(default)]
            key_insights: Vec<AiKeyInsight>,
        }

        #[derive(serde::Deserialize)]
        struct AiActionItem {
            text: String,
            #[serde(default)]
            assignee: Option<String>,
            #[serde(default)]
            priority: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct AiDecision {
            text: String,
            #[serde(default)]
            made_by: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct AiRisk {
            text: String,
            #[serde(default)]
            severity: Option<f32>,
            #[serde(default)]
            r#type: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct AiKeyInsight {
            text: String,
            #[serde(default)]
            importance: Option<f32>,
        }

        let insights: AiInsights = match serde_json::from_str(&json_str) {
            Ok(i) => i,
            Err(e) => {
                log::warn!("Failed to parse AI live intel response: {}", e);
                return events;
            }
        };

        // Default timestamp from latest segment
        let default_ts = segments.last().map(|s| s.timestamp_ms).unwrap_or(0);

        for item in insights.action_items {
            if !item.text.is_empty() {
                let assignee = item.assignee.filter(|a| !a.is_empty());
                events.push(LiveInsightEvent::ActionItem {
                    id: self.generate_id("ai_action"),
                    text: if let Some(ref p) = item.priority {
                        format!("[{}] {}", p.to_uppercase(), item.text)
                    } else {
                        item.text
                    },
                    assignee,
                    timestamp_ms: default_ts,
                });
            }
        }

        for dec in insights.decisions {
            if !dec.text.is_empty() {
                let context = dec.made_by.as_deref().unwrap_or("Unknown").to_string();
                events.push(LiveInsightEvent::Decision {
                    id: self.generate_id("ai_decision"),
                    text: dec.text,
                    context,
                    timestamp_ms: default_ts,
                });
            }
        }

        for risk in insights.risks {
            if !risk.text.is_empty() {
                events.push(LiveInsightEvent::RiskSignal {
                    id: self.generate_id("ai_risk"),
                    text: risk.text,
                    severity: risk.severity.unwrap_or(0.5).min(1.0),
                    timestamp_ms: default_ts,
                });
            }
        }

        for insight in insights.key_insights {
            if !insight.text.is_empty() {
                // Map key insights as question suggestions (closest event type)
                events.push(LiveInsightEvent::QuestionSuggestion {
                    id: self.generate_id("ai_insight"),
                    text: insight.text,
                    reason: format!(
                        "AI insight (importance: {:.0}/5)",
                        insight.importance.unwrap_or(3.0)
                    ),
                    timestamp_ms: default_ts,
                });
            }
        }

        events
    }
}

impl Default for LiveIntelAgent {
    fn default() -> Self {
        Self::new()
    }
}

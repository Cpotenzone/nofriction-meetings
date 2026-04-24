// Live Intelligence Agent v2
// Real-time meeting intelligence engine with smarter extraction,
// sentiment tracking, energy scoring, and AI-powered deep analysis.

use crate::catch_up_agent::TranscriptSegment;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

// ═══════════════════════════════════════════════════════════════════════════
// Event Types
// ═══════════════════════════════════════════════════════════════════════════

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
    KeyInsight {
        id: String,
        text: String,
        importance: f32,
        timestamp_ms: i64,
    },
    Deadline {
        id: String,
        text: String,
        deadline_ref: String,
        owner: Option<String>,
        timestamp_ms: i64,
    },
}

impl LiveInsightEvent {
    pub fn id(&self) -> &str {
        match self {
            Self::ActionItem { id, .. }
            | Self::Decision { id, .. }
            | Self::RiskSignal { id, .. }
            | Self::QuestionSuggestion { id, .. }
            | Self::Commitment { id, .. }
            | Self::TopicShift { id, .. }
            | Self::KeyInsight { id, .. }
            | Self::Deadline { id, .. } => id,
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            Self::ActionItem { timestamp_ms, .. }
            | Self::Decision { timestamp_ms, .. }
            | Self::RiskSignal { timestamp_ms, .. }
            | Self::QuestionSuggestion { timestamp_ms, .. }
            | Self::Commitment { timestamp_ms, .. }
            | Self::TopicShift { timestamp_ms, .. }
            | Self::KeyInsight { timestamp_ms, .. }
            | Self::Deadline { timestamp_ms, .. } => *timestamp_ms,
        }
    }

    fn event_type(&self) -> &str {
        match self {
            Self::ActionItem { .. } => "action_item",
            Self::Decision { .. } => "decision",
            Self::RiskSignal { .. } => "risk_signal",
            Self::QuestionSuggestion { .. } => "question_suggestion",
            Self::Commitment { .. } => "commitment",
            Self::TopicShift { .. } => "topic_shift",
            Self::KeyInsight { .. } => "key_insight",
            Self::Deadline { .. } => "deadline",
        }
    }

    fn text_content(&self) -> &str {
        match self {
            Self::ActionItem { text, .. }
            | Self::Decision { text, .. }
            | Self::RiskSignal { text, .. }
            | Self::QuestionSuggestion { text, .. }
            | Self::Commitment { text, .. }
            | Self::KeyInsight { text, .. }
            | Self::Deadline { text, .. } => text,
            Self::TopicShift { to_topic, .. } => to_topic,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Conversation State
// ═══════════════════════════════════════════════════════════════════════════

/// Conversation state tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationState {
    pub current_topic: String,
    pub unresolved_questions: Vec<String>,
    pub sentiment_score: f32, // -1.0 to 1.0
    pub speaker_talk_time: HashMap<String, i64>,
    /// Meeting energy: 0..100 based on speech rate and speaker diversity
    pub energy_score: f32,
}

/// Meeting-level aggregate statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeetingStats {
    pub total_segments: u64,
    pub total_words: u64,
    pub unique_speakers: usize,
    pub action_item_count: u32,
    pub decision_count: u32,
    pub risk_count: u32,
    pub deadline_count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════
// Live Intel Agent
// ═══════════════════════════════════════════════════════════════════════════

/// Minimum word count to analyze a segment (skip fragments like "yeah", "uh huh")
const MIN_WORDS_FOR_ANALYSIS: usize = 4;
/// Cooldown in milliseconds between same-type events
const EVENT_COOLDOWN_MS: i64 = 30_000;

/// Positive sentiment signal words
const POSITIVE_SIGNALS: &[&str] = &[
    "great",
    "excellent",
    "amazing",
    "perfect",
    "wonderful",
    "love",
    "excited",
    "happy",
    "good news",
    "well done",
    "congratulations",
    "impressive",
    "fantastic",
    "successful",
    "on track",
    "ahead of schedule",
    "optimistic",
    "thrilled",
    "confident",
    "proud",
];

/// Negative sentiment signal words
const NEGATIVE_SIGNALS: &[&str] = &[
    "concern",
    "worried",
    "problem",
    "issue",
    "risk",
    "blocker",
    "frustrated",
    "disappointed",
    "behind schedule",
    "delayed",
    "pushback",
    "not working",
    "failing",
    "struggling",
    "confusing",
    "unclear",
    "missed",
    "broken",
    "bug",
    "urgent",
];

/// Deadline/time reference patterns
const DEADLINE_PATTERNS: &[&str] = &[
    "by friday",
    "by monday",
    "by tuesday",
    "by wednesday",
    "by thursday",
    "by saturday",
    "by sunday",
    "by end of day",
    "by eod",
    "by end of week",
    "by end of month",
    "by end of quarter",
    "next week",
    "next month",
    "next sprint",
    "this friday",
    "this week",
    "due date",
    "deadline is",
    "need it by",
    "needs to be done by",
    "target date",
    "ship by",
    "launch date",
    "go-live",
    "before the",
    "no later than",
];

pub struct LiveIntelAgent {
    /// Rolling context window
    context_window: VecDeque<TranscriptSegment>,
    /// Maximum segments to keep in context
    max_context_segments: usize,
    /// Current conversation state
    pub conversation_state: ConversationState,
    /// Meeting-level aggregate stats
    pub stats: MeetingStats,
    /// Generated insight counter
    insight_counter: u64,
    /// Emitted events
    emitted_events: Vec<LiveInsightEvent>,
    /// Dedup: set of (event_type, first-40-chars-of-text) to prevent duplicates
    seen_hashes: HashSet<String>,
    /// Last emission timestamp per event type for cooldown
    last_event_ts: HashMap<String, i64>,
    /// Running sentiment samples for weighted average
    sentiment_samples: VecDeque<f32>,
    /// Word timestamps for WPM calculation
    word_timestamps: VecDeque<(i64, u32)>, // (timestamp_ms, word_count)
}

impl LiveIntelAgent {
    pub fn new() -> Self {
        Self {
            context_window: VecDeque::new(),
            max_context_segments: 80, // ~8 minutes at typical speaking rate
            conversation_state: ConversationState::default(),
            stats: MeetingStats::default(),
            insight_counter: 0,
            emitted_events: Vec::new(),
            seen_hashes: HashSet::new(),
            last_event_ts: HashMap::new(),
            sentiment_samples: VecDeque::new(),
            word_timestamps: VecDeque::new(),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Core Processing
    // ═══════════════════════════════════════════════════════════════════════

    /// Process a new transcript segment and extract insights
    pub fn process_segment(&mut self, segment: TranscriptSegment) -> Vec<LiveInsightEvent> {
        let mut events = Vec::new();

        // Add to context window
        self.context_window.push_back(segment.clone());
        while self.context_window.len() > self.max_context_segments {
            self.context_window.pop_front();
        }

        // Update stats
        let word_count = segment.text.split_whitespace().count() as u64;
        self.stats.total_segments += 1;
        self.stats.total_words += word_count;

        // Update speaker tracking
        if let Some(ref speaker) = segment.speaker {
            let entry = self
                .conversation_state
                .speaker_talk_time
                .entry(speaker.clone())
                .or_insert(0);
            *entry += word_count as i64;
            self.stats.unique_speakers = self.conversation_state.speaker_talk_time.len();
        }

        // Update sentiment
        self.update_sentiment(&segment);

        // Update energy score
        self.update_energy(segment.timestamp_ms, word_count as u32);

        // Skip very short segments (fragments, filler words)
        if word_count < MIN_WORDS_FOR_ANALYSIS as u64 {
            return events;
        }

        // Extract insights from segment
        events.extend(self.detect_action_items(&segment));
        events.extend(self.detect_decisions(&segment));
        events.extend(self.detect_commitments(&segment));
        events.extend(self.detect_risks(&segment));
        events.extend(self.detect_deadlines(&segment));
        events.extend(self.detect_topic_shifts(&segment));
        events.extend(self.generate_question_suggestions(&segment));

        // Dedup and store
        let events: Vec<_> = events.into_iter().filter(|e| self.is_unique(e)).collect();
        self.emitted_events.extend(events.clone());

        events
    }

    /// Get all events emitted so far
    pub fn get_all_events(&self) -> &[LiveInsightEvent] {
        &self.emitted_events
    }

    /// Get meeting stats
    pub fn get_stats(&self) -> &MeetingStats {
        &self.stats
    }

    /// Clear context and reset state
    pub fn reset(&mut self) {
        self.context_window.clear();
        self.conversation_state = ConversationState::default();
        self.stats = MeetingStats::default();
        self.emitted_events.clear();
        self.seen_hashes.clear();
        self.last_event_ts.clear();
        self.sentiment_samples.clear();
        self.word_timestamps.clear();
        self.insight_counter = 0;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Internal Utilities
    // ═══════════════════════════════════════════════════════════════════════

    fn generate_id(&mut self, prefix: &str) -> String {
        self.insight_counter += 1;
        format!("{}_{}", prefix, self.insight_counter)
    }

    /// Check cooldown — suppress same-type events within 30s
    fn check_cooldown(&mut self, event_type: &str, timestamp_ms: i64) -> bool {
        if let Some(&last_ts) = self.last_event_ts.get(event_type) {
            if (timestamp_ms - last_ts).abs() < EVENT_COOLDOWN_MS {
                return false; // Still in cooldown
            }
        }
        self.last_event_ts
            .insert(event_type.to_string(), timestamp_ms);
        true
    }

    /// Dedup check — prevents identical insights from rule + AI paths
    fn is_unique(&mut self, event: &LiveInsightEvent) -> bool {
        let text = event.text_content();
        let key = format!(
            "{}:{}",
            event.event_type(),
            &text[..text.len().min(60)].to_lowercase()
        );
        self.seen_hashes.insert(key)
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

    /// Count how many patterns match in the text (for multi-signal confirmation)
    fn count_matches(text: &str, patterns: &[&str]) -> usize {
        patterns.iter().filter(|p| text.contains(**p)).count()
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Sentiment & Energy
    // ═══════════════════════════════════════════════════════════════════════

    fn update_sentiment(&mut self, segment: &TranscriptSegment) {
        let lower = segment.text.to_lowercase();
        let positive_hits = Self::count_matches(&lower, POSITIVE_SIGNALS);
        let negative_hits = Self::count_matches(&lower, NEGATIVE_SIGNALS);

        if positive_hits > 0 || negative_hits > 0 {
            let sample = if positive_hits > negative_hits {
                0.5 + (positive_hits as f32 * 0.15).min(0.5)
            } else if negative_hits > positive_hits {
                -0.5 - (negative_hits as f32 * 0.15).min(0.5)
            } else {
                0.0
            };

            self.sentiment_samples.push_back(sample);
            // Keep last 30 samples for weighted average
            while self.sentiment_samples.len() > 30 {
                self.sentiment_samples.pop_front();
            }

            // Exponential weighted average — recent samples matter more
            if !self.sentiment_samples.is_empty() {
                let mut weight_sum = 0.0_f32;
                let mut value_sum = 0.0_f32;
                let len = self.sentiment_samples.len() as f32;
                for (i, &val) in self.sentiment_samples.iter().enumerate() {
                    let weight = (i as f32 / len).exp();
                    weight_sum += weight;
                    value_sum += val * weight;
                }
                self.conversation_state.sentiment_score = (value_sum / weight_sum).clamp(-1.0, 1.0);
            }
        }
    }

    fn update_energy(&mut self, timestamp_ms: i64, word_count: u32) {
        self.word_timestamps.push_back((timestamp_ms, word_count));

        // Keep last 2 minutes of data
        let cutoff = timestamp_ms - 120_000;
        while self
            .word_timestamps
            .front()
            .map_or(false, |(ts, _)| *ts < cutoff)
        {
            self.word_timestamps.pop_front();
        }

        if self.word_timestamps.len() < 3 {
            return;
        }

        // Calculate words per minute
        let total_words: u32 = self.word_timestamps.iter().map(|(_, w)| w).sum();
        let time_span_ms = timestamp_ms
            - self
                .word_timestamps
                .front()
                .map(|(ts, _)| *ts)
                .unwrap_or(timestamp_ms);
        let time_span_min = (time_span_ms as f32 / 60_000.0).max(0.1);
        let wpm = total_words as f32 / time_span_min;

        // Speaker diversity factor (0..1): how many unique speakers in recent window
        let recent_speakers: HashSet<_> = self
            .context_window
            .iter()
            .rev()
            .take(15)
            .filter_map(|s| s.speaker.as_ref())
            .collect();
        let diversity = (recent_speakers.len() as f32 / 4.0).min(1.0); // Normalize to 4 speakers = max

        // Energy = WPM factor (0..60) + diversity factor (0..40)
        // Typical conversational rate is 100-150 WPM; lively discussion is 150+
        let wpm_score = ((wpm / 150.0) * 60.0).clamp(0.0, 60.0);
        let diversity_score = diversity * 40.0;
        self.conversation_state.energy_score = (wpm_score + diversity_score).clamp(0.0, 100.0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Rule-Based Detectors
    // ═══════════════════════════════════════════════════════════════════════

    /// Detect action items — requires strong signal (task + assignment context)
    fn detect_action_items(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        // Strong action patterns (high confidence, standalone)
        let strong_patterns: &[(&str, Option<&str>)] = &[
            ("action item", None),
            ("follow up on", None),
            ("let's make sure", None),
            ("make sure to", None),
            ("need to follow up", None),
            ("i'll take care of", Some("speaker")),
            ("i will handle", Some("speaker")),
            ("i can do that", Some("speaker")),
            ("i'll get that done", Some("speaker")),
            ("i'll send", Some("speaker")),
            ("i'll schedule", Some("speaker")),
            ("i'll set up", Some("speaker")),
            ("i'll update", Some("speaker")),
            ("i'll reach out", Some("speaker")),
            ("assign that to", None),
            ("can you take", None),
            ("could you handle", None),
            ("will you take care of", None),
            ("please make sure", None),
            ("please send", None),
            ("please schedule", None),
            ("please follow up", None),
        ];

        // Weak patterns — require at least 2 co-occurring signals
        let weak_patterns = ["can you", "could you", "please", "need to", "should"];

        // Check strong patterns first
        for (pattern, assignee_hint) in strong_patterns {
            if text_lower.contains(pattern) {
                if !self.check_cooldown("action_item", segment.timestamp_ms) {
                    return Vec::new();
                }
                let assignee = match assignee_hint {
                    Some("speaker") => segment.speaker.clone(),
                    _ => None,
                };
                self.stats.action_item_count += 1;
                return vec![LiveInsightEvent::ActionItem {
                    id: self.generate_id("action"),
                    text: segment.text.clone(),
                    assignee,
                    timestamp_ms: segment.timestamp_ms,
                }];
            }
        }

        // Weak patterns: require 2+ co-occurring signals
        let weak_hits = weak_patterns
            .iter()
            .filter(|p| text_lower.contains(**p))
            .count();
        if weak_hits >= 2 {
            if !self.check_cooldown("action_item", segment.timestamp_ms) {
                return Vec::new();
            }
            self.stats.action_item_count += 1;
            return vec![LiveInsightEvent::ActionItem {
                id: self.generate_id("action"),
                text: segment.text.clone(),
                assignee: None,
                timestamp_ms: segment.timestamp_ms,
            }];
        }

        Vec::new()
    }

    /// Detect decisions — requires strong decision language
    fn detect_decisions(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        let patterns = [
            "we decided",
            "we agreed",
            "let's go with",
            "the decision is",
            "we're going to go with",
            "we'll do",
            "that's the plan",
            "sounds good, let's",
            "approved",
            "settled on",
            "final answer is",
            "we're going with",
            "consensus is",
            "we've landed on",
            "signed off on",
        ];

        for pattern in patterns {
            if text_lower.contains(pattern) {
                if !self.check_cooldown("decision", segment.timestamp_ms) {
                    return Vec::new();
                }
                self.stats.decision_count += 1;
                return vec![LiveInsightEvent::Decision {
                    id: self.generate_id("decision"),
                    text: segment.text.clone(),
                    context: self.get_recent_context(3),
                    timestamp_ms: segment.timestamp_ms,
                }];
            }
        }

        Vec::new()
    }

    /// Detect commitments — personal promises
    fn detect_commitments(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        let patterns = [
            "i commit",
            "i promise",
            "you have my word",
            "i guarantee",
            "i'll make sure",
            "count on me",
            "i'll get it done",
            "i'll own that",
            "i take responsibility",
            "i'm on it",
            "leave it to me",
            "i've got this",
        ];

        for pattern in patterns {
            if text_lower.contains(pattern) {
                if !self.check_cooldown("commitment", segment.timestamp_ms) {
                    return Vec::new();
                }
                return vec![LiveInsightEvent::Commitment {
                    id: self.generate_id("commit"),
                    text: segment.text.clone(),
                    by: segment.speaker.clone(),
                    timestamp_ms: segment.timestamp_ms,
                }];
            }
        }

        Vec::new()
    }

    /// Detect risks/issues — severity is proportional to signal strength
    fn detect_risks(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        let risk_patterns: &[(&str, f32)] = &[
            ("blocker", 0.8),
            ("blocked", 0.7),
            ("not going to make it", 0.9),
            ("can't ship", 0.9),
            ("showstopper", 0.9),
            ("deal-breaker", 0.9),
            ("frustrated", 0.7),
            ("behind schedule", 0.7),
            ("delayed", 0.6),
            ("at risk", 0.7),
            ("critical issue", 0.8),
            ("major problem", 0.8),
            ("security concern", 0.8),
            ("compliance issue", 0.8),
            ("budget overrun", 0.7),
            ("scope creep", 0.6),
            ("disagree", 0.5),
            ("pushback", 0.5),
            ("concern about", 0.5),
            ("worried about", 0.6),
            ("risk of", 0.6),
            ("problem with", 0.5),
        ];

        let mut best_match: Option<(&str, f32)> = None;
        for &(pattern, severity) in risk_patterns {
            if text_lower.contains(pattern) {
                match best_match {
                    Some((_, s)) if severity > s => best_match = Some((pattern, severity)),
                    None => best_match = Some((pattern, severity)),
                    _ => {}
                }
            }
        }

        if let Some((_, severity)) = best_match {
            if !self.check_cooldown("risk", segment.timestamp_ms) {
                return Vec::new();
            }
            self.stats.risk_count += 1;
            return vec![LiveInsightEvent::RiskSignal {
                id: self.generate_id("risk"),
                text: segment.text.clone(),
                severity,
                timestamp_ms: segment.timestamp_ms,
            }];
        }

        Vec::new()
    }

    /// Detect deadlines and time-bound commitments
    fn detect_deadlines(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        for pattern in DEADLINE_PATTERNS {
            if text_lower.contains(pattern) {
                if !self.check_cooldown("deadline", segment.timestamp_ms) {
                    return Vec::new();
                }

                // Extract the deadline reference (the matched phrase + surrounding context)
                let deadline_ref = self.extract_deadline_context(&text_lower, pattern);

                self.stats.deadline_count += 1;
                return vec![LiveInsightEvent::Deadline {
                    id: self.generate_id("deadline"),
                    text: segment.text.clone(),
                    deadline_ref,
                    owner: segment.speaker.clone(),
                    timestamp_ms: segment.timestamp_ms,
                }];
            }
        }

        Vec::new()
    }

    /// Extract the deadline phrase with surrounding context
    fn extract_deadline_context(&self, text: &str, matched_pattern: &str) -> String {
        if let Some(idx) = text.find(matched_pattern) {
            // Take up to 10 chars before and 30 chars after the match
            let start = idx.saturating_sub(10);
            let end = (idx + matched_pattern.len() + 30).min(text.len());
            let snippet = &text[start..end];
            // Trim to word boundaries
            snippet
                .trim()
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        } else {
            matched_pattern.to_string()
        }
    }

    /// Detect topic shifts — requires explicit transition language
    fn detect_topic_shifts(&mut self, segment: &TranscriptSegment) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        let topic_shift_patterns = [
            "let's move on to",
            "moving on to",
            "next topic",
            "next item",
            "switching gears",
            "let's talk about",
            "let's discuss",
            "onto the next",
            "can we discuss",
            "i want to bring up",
            "next on the agenda",
            "circling back to",
            "let's shift to",
        ];

        for pattern in topic_shift_patterns {
            if text_lower.contains(pattern) {
                let old_topic = self.conversation_state.current_topic.clone();
                let new_topic = self.extract_topic(&segment.text);

                if !new_topic.is_empty() && new_topic != old_topic {
                    self.conversation_state.current_topic = new_topic.clone();

                    return vec![LiveInsightEvent::TopicShift {
                        id: self.generate_id("topic"),
                        from_topic: if old_topic.is_empty() {
                            "(start)".to_string()
                        } else {
                            old_topic
                        },
                        to_topic: new_topic,
                        timestamp_ms: segment.timestamp_ms,
                    }];
                }
                break;
            }
        }

        Vec::new()
    }

    /// Generate question suggestions for ambiguous/unclear statements
    fn generate_question_suggestions(
        &mut self,
        segment: &TranscriptSegment,
    ) -> Vec<LiveInsightEvent> {
        let text_lower = segment.text.to_lowercase();

        let unclear_patterns = [
            ("i'm not sure about", "Can you clarify what you mean?"),
            ("maybe we should", "What would help you decide on this?"),
            (
                "we should probably",
                "What's the specific timeline for this?",
            ),
            (
                "at some point we need to",
                "When specifically should this happen?",
            ),
            ("someone should", "Who specifically will own this?"),
            ("it depends on", "What are the specific dependencies?"),
            ("we might need to", "What would trigger this action?"),
            ("i think we could", "What's the concrete next step here?"),
            ("not sure who", "Can we assign an owner for this?"),
            ("tbd", "When will this be decided?"),
        ];

        for (pattern, suggestion) in unclear_patterns {
            if text_lower.contains(pattern) {
                if !self.check_cooldown("question", segment.timestamp_ms) {
                    return Vec::new();
                }
                return vec![LiveInsightEvent::QuestionSuggestion {
                    id: self.generate_id("question"),
                    text: suggestion.to_string(),
                    reason: format!(
                        "Based on: \"{}\"",
                        segment.text.chars().take(80).collect::<String>()
                    ),
                    timestamp_ms: segment.timestamp_ms,
                }];
            }
        }

        Vec::new()
    }

    fn extract_topic(&self, text: &str) -> String {
        let lower = text.to_lowercase();
        for marker in [
            "about ",
            "to discuss ",
            "discuss ",
            "move on to ",
            "talk about ",
            "shift to ",
            "bring up ",
        ] {
            if let Some(idx) = lower.find(marker) {
                let start = idx + marker.len();
                let topic: String = text[start..]
                    .chars()
                    .take(60)
                    .take_while(|c| *c != '.' && *c != ',' && *c != '?' && *c != '!')
                    .collect();
                let trimmed = topic.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
        String::new()
    }

    // ═══════════════════════════════════════════════════════════════════════
    // AI-Powered Analysis
    // ═══════════════════════════════════════════════════════════════════════

    /// AI-powered batch analysis of transcript segments
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

        // Include conversation state context for the AI
        let state_context = format!(
            "MEETING CONTEXT:\n- Current topic: {}\n- Sentiment: {:.2} (-1 negative, +1 positive)\n- Energy: {:.0}/100\n- Speakers: {}\n",
            if self.conversation_state.current_topic.is_empty() {
                "unknown"
            } else {
                &self.conversation_state.current_topic
            },
            self.conversation_state.sentiment_score,
            self.conversation_state.energy_score,
            self.conversation_state
                .speaker_talk_time
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );

        let prompt = format!(
            "{}\n\n{}\n\nTRANSCRIPT SEGMENTS:\n{}\n\nRespond with a JSON object containing these arrays (all optional):\n{{\n  \"action_items\": [{{\"text\": \"...\", \"assignee\": \"...\", \"priority\": \"high|medium|low\"}}],\n  \"decisions\": [{{\"text\": \"...\", \"made_by\": \"...\"}}],\n  \"risks\": [{{\"text\": \"...\", \"severity\": 0.0-1.0, \"type\": \"...\"}}],\n  \"key_insights\": [{{\"text\": \"...\", \"importance\": 1.0-5.0}}],\n  \"deadlines\": [{{\"text\": \"...\", \"deadline\": \"...\", \"owner\": \"...\"}}]\n}}\n\nJSON RESPONSE:",
            system_prompt, state_context, transcript_text
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

        // Extract JSON from response (handle markdown code blocks)
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

        // Deserialize structures
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
            #[serde(default)]
            deadlines: Vec<AiDeadline>,
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
            #[allow(dead_code)]
            r#type: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct AiKeyInsight {
            text: String,
            #[serde(default)]
            importance: Option<f32>,
        }

        #[derive(serde::Deserialize)]
        struct AiDeadline {
            text: String,
            #[serde(default)]
            deadline: Option<String>,
            #[serde(default)]
            owner: Option<String>,
        }

        let insights: AiInsights = match serde_json::from_str(&json_str) {
            Ok(i) => i,
            Err(e) => {
                log::warn!("Failed to parse AI live intel response: {}", e);
                log::debug!("Raw JSON: {}", &json_str[..json_str.len().min(500)]);
                return events;
            }
        };

        // Default timestamp from latest segment
        let default_ts = segments.last().map(|s| s.timestamp_ms).unwrap_or(0);

        // Action items
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

        // Decisions
        for dec in insights.decisions {
            if !dec.text.is_empty() {
                let context = dec.made_by.unwrap_or_else(|| "Team".to_string());
                events.push(LiveInsightEvent::Decision {
                    id: self.generate_id("ai_decision"),
                    text: dec.text,
                    context,
                    timestamp_ms: default_ts,
                });
            }
        }

        // Risks
        for risk in insights.risks {
            if !risk.text.is_empty() {
                events.push(LiveInsightEvent::RiskSignal {
                    id: self.generate_id("ai_risk"),
                    text: risk.text,
                    severity: risk.severity.unwrap_or(0.5).clamp(0.0, 1.0),
                    timestamp_ms: default_ts,
                });
            }
        }

        // Key insights → proper KeyInsight event type
        for insight in insights.key_insights {
            if !insight.text.is_empty() {
                events.push(LiveInsightEvent::KeyInsight {
                    id: self.generate_id("ai_insight"),
                    text: insight.text,
                    importance: insight.importance.unwrap_or(3.0).clamp(1.0, 5.0),
                    timestamp_ms: default_ts,
                });
            }
        }

        // Deadlines
        for dl in insights.deadlines {
            if !dl.text.is_empty() {
                events.push(LiveInsightEvent::Deadline {
                    id: self.generate_id("ai_deadline"),
                    text: dl.text,
                    deadline_ref: dl.deadline.unwrap_or_default(),
                    owner: dl.owner.filter(|o| !o.is_empty()),
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

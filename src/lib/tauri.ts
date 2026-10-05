// noFriction Meetings - Tauri API Wrappers
// Type-safe wrappers for Tauri commands

import { invoke } from "@tauri-apps/api/core";
import type { RedactionRecord } from "./redaction";
import { withAiConsent } from "./ai";

// Types
export interface RecordingStatus {
    is_recording: boolean;
    duration_seconds: number;
    video_frames: number;
    audio_samples: number;
    audio_warning?: string | null;
}

export interface AudioDevice {
    id: string;
    name: string;
    is_default: boolean;
    is_input: boolean;
}

export interface MonitorInfo {
    id: number;
    name: string;
    width: number;
    height: number;
    is_primary: boolean;
}

export interface Meeting {
    id: string;
    title: string;
    started_at: string;
    ended_at: string | null;
    duration_seconds: number | null;
    calendar_event_id: string | null;
}

export interface Transcript {
    id: number;
    meeting_id: string;
    text: string;
    speaker: string | null;
    timestamp: string;
    is_final: boolean;
    confidence: number;
}

export interface Frame {
    id: number;
    meeting_id: string;
    timestamp: string;
    thumbnail_path: string | null;
    ocr_text: string | null;
}

export interface SearchResult {
    meeting_id: string;
    meeting_title: string;
    transcript_text: string;
    timestamp: string;
    relevance: number;
}

export interface TranscriptEvent {
    text: string;
    is_final: boolean;
    confidence: number;
    start: number;
    duration: number;
    speaker: string | null;
    /** Groups interim hypotheses with their final segments (local Whisper) */
    utterance_id?: string | null;
}

export interface AppSettings {
    /** Always null from the backend: secrets stay in the Keychain. */
    deepgram_api_key: string | null;
    /** {configured, last4} per secret setting key */
    secret_status?: Record<string, { configured: boolean; last4: string | null }>;
    selected_microphone: string | null;
    selected_monitor: number | null;
    auto_start_recording: boolean;
    show_notifications: boolean;
}

// Recording commands
export async function startRecording(): Promise<string> {
    return invoke<string>("start_recording");
}

export async function stopRecording(): Promise<void> {
    return invoke("stop_recording");
}

export async function getRecordingStatus(): Promise<RecordingStatus> {
    return invoke<RecordingStatus>("get_recording_status");
}

// Screenshot command (for preview)
export async function captureScreenshot(monitorId?: number): Promise<string> {
    return invoke<string>("capture_screenshot", { monitorId });
}

// Transcript commands
export async function getTranscripts(meetingId: string): Promise<Transcript[]> {
    return invoke<Transcript[]>("get_transcripts", { meetingId });
}

export async function searchTranscripts(query: string): Promise<SearchResult[]> {
    return invoke<SearchResult[]>("search_transcripts", { query });
}

// Frame commands (for rewind timeline)
export async function getFrames(meetingId: string, limit?: number): Promise<Frame[]> {
    return invoke<Frame[]>("get_frames", { meetingId, limit });
}

export async function getFrameCount(meetingId: string): Promise<number> {
    return invoke<number>("get_frame_count", { meetingId });
}

// Device commands
export async function getAudioDevices(): Promise<AudioDevice[]> {
    return invoke<AudioDevice[]>("get_audio_devices");
}

export async function setAudioDevice(deviceId: string): Promise<void> {
    return invoke("set_audio_device", { deviceId });
}

export async function getMonitors(): Promise<MonitorInfo[]> {
    return invoke<MonitorInfo[]>("get_monitors");
}

export async function setMonitor(monitorId: number): Promise<void> {
    return invoke("set_monitor", { monitorId });
}

// Settings commands
export async function getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>("get_settings");
}

// Set frame capture interval (milliseconds)
export async function setFrameCaptureInterval(intervalMs: number): Promise<void> {
    return invoke("set_frame_capture_interval", { intervalMs });
}

// Meeting commands
export async function getMeetings(limit?: number): Promise<Meeting[]> {
    return invoke<Meeting[]>("get_meetings", { limit });
}

export async function getMeeting(meetingId: string): Promise<Meeting | null> {
    return invoke<Meeting | null>("get_meeting", { meetingId });
}

export async function deleteMeeting(meetingId: string): Promise<void> {
    return invoke("delete_meeting", { meetingId });
}

// Synced Timeline types
export interface TimelineFrame {
    id: string;
    frame_number: number;
    timestamp_ms: number;
    thumbnail_path: string | null;
    /** When the screen stopped being shown (ms from the start), if known */
    end_ms?: number | null;
}

export interface TimelineTranscript {
    id: string;
    timestamp_ms: number;
    text: string;
    speaker: string | null;
    is_final: boolean;
    duration_seconds: number;
    /** End of the line's time span (ms from the start) as a time-range
     *  Delete reads it; null when its time can't be read */
    end_ms?: number | null;
    /** Middle of each word's time (ms from the start), when word timings
     *  are stored (a time range removes exactly those words) */
    word_mids_ms?: number[] | null;
}

export interface SyncedTimeline {
    meeting_id: string;
    meeting_title: string;
    /** RFC3339 meeting start (places stricken-screen markers) */
    started_at?: string;
    duration_seconds: number;
    frames: TimelineFrame[];
    /** Transcript text keeps strike-marker tokens; see lib/redaction.ts */
    transcripts: TimelineTranscript[];
    /** "Stricken from the record" markers (no content) */
    redactions?: RedactionRecord[];
}

// Synced timeline command
export async function getSyncedTimeline(meetingId: string): Promise<SyncedTimeline> {
    return invoke<SyncedTimeline>("get_synced_timeline", { meetingId });
}

// Get frame thumbnail (full or thumbnail size)
export async function getFrameThumbnail(frameId: string, thumbnail: boolean = true): Promise<string | null> {
    return invoke<string | null>("get_frame_thumbnail", { frameId, thumbnail });
}

// Get API key
export async function getApiKey(): Promise<string | null> {
    return invoke<string | null>("get_deepgram_api_key");
}

// Get saved settings
export async function getSavedSettings(): Promise<{ microphone: string | null; monitor_id: number | null }> {
    try {
        const settings = await invoke<AppSettings>("get_settings");
        return {
            microphone: settings.selected_microphone,
            monitor_id: settings.selected_monitor,
        };
    } catch {
        return { microphone: null, monitor_id: null };
    }
}

// ============================================
// Knowledge Base Configuration Commands
// ============================================

// VLM commands
export async function checkVlm(): Promise<boolean> {
    return invoke<boolean>("check_vlm");
}

export async function checkVlmVision(): Promise<boolean> {
    return invoke<boolean>("check_vlm_vision");
}

// Knowledge Base Processing
export async function analyzePendingFrames(limit?: number): Promise<{ frames_processed: number; activities_created: number }> {
    return invoke("analyze_pending_frames", { limit });
}

export async function getPendingFrameCount(): Promise<number> {
    return invoke<number>("get_pending_frame_count");
}

// Knowledge Base Search
export interface KBSearchResult {
    id: string;
    source: string;
    timestamp: string | null;
    app_name: string | null;
    category: string | null;
    summary: string;
    score: number | null;
}

export interface SearchOptions {
    query?: string;
    start_date?: string;
    end_date?: string;
    category?: string;
    limit?: number;
}

export async function searchKnowledgeBase(options: SearchOptions): Promise<KBSearchResult[]> {
    return invoke<KBSearchResult[]>("search_knowledge_base", { options });
}

// ============================================================================
// Video Recording Commands
// ============================================================================

export interface VideoChunk {
    chunk_number: number;
    path: string;
    start_time: string;
    end_time: string | null;
    size_bytes: number;
    duration_secs: number;
}

export interface PinMoment {
    timestamp: string;
    offset_secs: number;
    label: string | null;
    chunk_number: number;
}

export interface RecordingSession {
    meeting_id: string;
    started_at: string;
    chunks: VideoChunk[];
    pin_moments: PinMoment[];
    is_active: boolean;
}

export interface ExtractedFrame {
    path: string;
    timestamp_secs: number;
    chunk_number: number;
    extracted_at: string;
    width: number;
    height: number;
}

export interface StorageStats {
    total_bytes: number;
    video_bytes: number;
    frames_bytes: number;
    meetings_count: number;
    chunks_count: number;
    oldest_meeting: string | null;
    disk_limit_bytes: number;
    usage_percent: number;
}

// Start video recording for a meeting
export async function startVideoRecording(meetingId: string): Promise<void> {
    return invoke("start_video_recording", { meetingId });
}

// Stop video recording
export async function stopVideoRecording(): Promise<RecordingSession> {
    return invoke<RecordingSession>("stop_video_recording");
}

// Get current video recording status
export async function getVideoRecordingStatus(): Promise<RecordingSession | null> {
    return invoke<RecordingSession | null>("get_video_recording_status");
}

// Pin the current moment in recording
export async function videoPinMoment(label?: string): Promise<PinMoment> {
    return invoke<PinMoment>("video_pin_moment", { label });
}

// Extract a frame at a specific timestamp
export async function extractFrameAt(
    meetingId: string,
    chunkNumber: number,
    timestampSecs: number
): Promise<ExtractedFrame> {
    return invoke<ExtractedFrame>("extract_frame_at", {
        meetingId,
        chunkNumber,
        timestampSecs,
    });
}

// Extract thumbnail for timeline view
export async function extractThumbnail(
    meetingId: string,
    chunkNumber: number,
    timestampSecs: number,
    size?: number
): Promise<string> {
    return invoke<string>("extract_thumbnail", {
        meetingId,
        chunkNumber,
        timestampSecs,
        size,
    });
}

// Get storage statistics
export async function getStorageStats(): Promise<StorageStats> {
    return invoke<StorageStats>("get_storage_stats");
}

// Apply retention policies
export async function applyRetention(): Promise<[number, number]> {
    return invoke<[number, number]>("apply_retention");
}

// Delete a meeting's video storage
export async function deleteVideoStorage(meetingId: string): Promise<number> {
    return invoke<number>("delete_video_storage", { meetingId });
}

// ============================================
// Activity Theme Commands
// ============================================

export interface ThemeSettings {
    active_theme: string;
    prospecting_interval_ms: number;
    fundraising_interval_ms: number;
    product_dev_interval_ms: number;
    admin_interval_ms: number;
    personal_interval_ms: number;
}

// Set the active theme
export async function setActiveTheme(theme: string): Promise<void> {
    return invoke<void>("set_active_theme", { theme });
}

// Get the current active theme
export async function getActiveTheme(): Promise<string> {
    return invoke<string>("get_active_theme");
}

// Get all theme settings
export async function getThemeSettings(): Promise<ThemeSettings> {
    return invoke<ThemeSettings>("get_theme_settings");
}

// Set screenshot interval for a specific theme
export async function setThemeInterval(theme: string, intervalMs: number): Promise<void> {
    return invoke<void>("set_theme_interval", { theme, intervalMs });
}

// Get time spent in a theme today (in hours)
// Get time spent in a theme today (in hours)
export async function getThemeTimeToday(theme: string): Promise<number> {
    return invoke<number>("get_theme_time_today", { theme });
}

// Debug logging to terminal
export async function debugLog(message: string): Promise<void> {
    return invoke("debug_log", { message });
}

// AI / LLM Commands
export async function aiChat(presetId: string, message: string, meetingId?: string): Promise<string> {
    return withAiConsent(() => invoke<string>("ai_chat", {
        presetId,
        message,
        meetingId
    }));
}

// ============================================
// Intelligence / Meeting State Commands
// ============================================

export interface MeetingState {
    meeting_id: string | null;
    mode: 'pre' | 'live' | 'catchup';
    minutes_since_start: number;
    minutes_until_start: number;
    confidence: number;
    title: string;
    attendees: string[];
    is_transcript_running: boolean;
    is_meeting_window_active: boolean;
}

export interface InsightItem {
    text: string;
    importance: number;
}

export interface Decision {
    text: string;
    made_by: string | null;
}

export interface RiskSignal {
    text: string;
    severity: number;
    signal_type: string;
}

export interface CatchUpCapsule {
    what_missed: InsightItem[];
    current_topic: string;
    decisions: Decision[];
    open_threads: string[];
    next_moves: string[];
    risks: RiskSignal[];
    questions_to_ask: string[];
    ten_second_version: string;
    sixty_second_version: string;
    confidence: number;
    generated_at_minute: number;
}

export interface LiveInsightEvent {
    type: string;
    id: string;
    text?: string;
    assignee?: string;
    context?: string;
    severity?: number;
    by?: string;
    from_topic?: string;
    to_topic?: string;
    reason?: string;
    importance?: number;
    deadline_ref?: string;
    owner?: string;
    timestamp_ms: number;
}

export async function getMeetingState(): Promise<MeetingState> {
    return invoke<MeetingState>("get_meeting_state");
}

export async function generateCatchUp(meetingId: string): Promise<CatchUpCapsule> {
    return withAiConsent(() => invoke<CatchUpCapsule>("generate_catch_up", { meetingId }));
}

export async function getLiveInsights(meetingId: string): Promise<LiveInsightEvent[]> {
    return invoke<LiveInsightEvent[]>("get_live_insights", { meetingId });
}

export async function pinInsight(meetingId: string, insightType: string, insightText: string, timestampMs: number): Promise<void> {
    return invoke("pin_insight", {
        meetingId,
        insightType,
        insightText,
        timestampMs
    });
}

export async function markDecision(meetingId: string, decisionText: string, context: string | null): Promise<void> {
    return invoke("mark_decision", {
        meetingId,
        decisionText,
        context
    });
}

// ============================================
// Always-On Recording Commands
// ============================================

export type CaptureMode = 'Ambient' | 'Meeting' | 'Paused';

export interface AlwaysOnSettings {
    enabled: boolean;
    idle_timeout_mins: number;
    ambient_interval_secs: number;
    meeting_interval_secs: number;
    retention_hours: number;
    calendar_detection: boolean;
    app_detection: boolean;
}

export async function getCaptureMode(): Promise<CaptureMode> {
    return invoke<string>("get_capture_mode").then(mode => mode as CaptureMode);
}

export async function startAmbientCapture(): Promise<void> {
    return invoke("start_ambient_capture");
}

export async function startMeetingCapture(): Promise<void> {
    return invoke("start_meeting_capture");
}

export async function pauseCapture(): Promise<void> {
    return invoke("pause_capture");
}

// Link accessibility captures to the current meeting
export async function setAccessibilityMeetingId(meetingId: string | null): Promise<void> {
    return invoke("set_accessibility_meeting_id", { meetingId });
}

export async function getAlwaysOnSettings(): Promise<AlwaysOnSettings> {
    return invoke<AlwaysOnSettings>("get_always_on_settings");
}

export async function setAlwaysOnEnabled(enabled: boolean): Promise<void> {
    return invoke("set_always_on_enabled", { enabled });
}


// ============================================
// Meeting-end detection (auto-stop)
// ============================================

export type MeetingEndSignal = "mic_released" | "window_closed" | "calendar_ended" | "silence";

/** Payload of `meeting-end-detected` */
export interface MeetingEndDetected {
    meeting_id: string;
    kind: MeetingEndSignal;
    reason: string;
    /** Seconds until the recording stops */
    countdown: number;
    deadline: string;
}

export interface MeetingEndStatus {
    monitoring: boolean;
    pending: MeetingEndDetected | null;
}

export interface AutoStopSettings {
    enabled: boolean;
    silenceMinutes: number;
}

/** "Keep recording": cancel the countdown and snooze detection */
export async function meetingEndKeepRecording(): Promise<boolean> {
    return invoke<boolean>("meeting_end_keep_recording");
}

export async function getMeetingEndStatus(): Promise<MeetingEndStatus> {
    return invoke<MeetingEndStatus>("get_meeting_end_status");
}

export async function getAutoStopSettings(): Promise<AutoStopSettings> {
    return invoke<AutoStopSettings>("get_auto_stop_settings");
}

export async function setAutoStopSettings(enabled: boolean, silenceMinutes?: number): Promise<AutoStopSettings> {
    return invoke<AutoStopSettings>("set_auto_stop_settings", { enabled, silenceMinutes });
}

export async function setGenieMode(isGenie: boolean): Promise<void> {
    return invoke("set_genie_mode", { isGenie });
}

// ============================================
// v3.0.0: Obsidian Vault Commands
// ============================================

export interface VaultTopic {
    name: string;
    path: string;
    meetings: string[];
    noteCount: number;
    createdAt: string;
    tags: string[];
}

export interface VaultFile {
    name: string;
    path: string;
    relativePath: string;
    isDir: boolean;
    modified: string;
    size: number;
    extension: string | null;
}

export interface VaultFileContent {
    path: string;
    content: string;
    frontmatter: Record<string, any>;
    body: string;
}

export interface VaultTreeNode {
    name: string;
    path: string;
    isDir: boolean;
    children: VaultTreeNode[];
}

export interface VaultStatus {
    configured: boolean;
    path: string | null;
    valid: boolean;
    topicCount: number;
    totalFiles: number;
}

export interface VaultSearchResult {
    filePath: string;
    fileName: string;
    matchingLine: string;
    lineNumber: number;
    context: string;
}

export async function getVaultStatus(): Promise<VaultStatus> {
    return invoke<VaultStatus>("get_vault_status");
}

export async function listVaultTopics(): Promise<VaultTopic[]> {
    return invoke<VaultTopic[]>("list_vault_topics");
}

export async function getVaultTopic(topicName: string): Promise<VaultTopic> {
    return invoke<VaultTopic>("get_vault_topic", { topicName });
}

export async function createVaultTopic(name: string, tags: string[]): Promise<VaultTopic> {
    return invoke<VaultTopic>("create_vault_topic", { name, tags });
}

export async function exportMeetingToVault(topicName: string, meetingId: string): Promise<string> {
    return invoke<string>("export_meeting_to_vault", { topicName, meetingId });
}

export async function readVaultFile(filePath: string): Promise<VaultFileContent> {
    return invoke<VaultFileContent>("read_vault_file", { filePath });
}

export async function writeVaultNote(topicName: string, fileName: string, content: string): Promise<string> {
    return invoke<string>("write_vault_note", { topicName, fileName, content });
}

export async function uploadToVault(topicName: string, sourcePath: string, destName?: string): Promise<string> {
    return invoke<string>("upload_to_vault", { topicName, sourcePath, destName });
}

export async function listVaultFiles(subPath?: string): Promise<VaultFile[]> {
    return invoke<VaultFile[]>("list_vault_files", { subPath });
}

export async function searchVault(query: string): Promise<VaultSearchResult[]> {
    return invoke<VaultSearchResult[]>("search_vault", { query });
}

export async function getVaultTree(): Promise<VaultTreeNode> {
    return invoke<VaultTreeNode>("get_vault_tree");
}

export async function deleteVaultItem(itemPath: string): Promise<void> {
    return invoke("delete_vault_item", { itemPath });
}

export async function setVaultPath(vaultPath: string): Promise<void> {
    return invoke("set_vault_path", { vaultPath });
}

// ============================================================================
// Obsidian Knowledge Management APIs
// ============================================================================

export interface VaultLink {
    sourceFile: string;
    target: string;
    displayText: string;
    lineNumber: number;
}

export interface BacklinkResult {
    targetFile: string;
    backlinks: VaultLink[];
}

export interface VaultTag {
    name: string;
    fileCount: number;
    files: string[];
}

export interface GraphNode {
    id: string;
    label: string;
    fileType: string;
}

export interface GraphEdge {
    source: string;
    target: string;
}

export interface VaultGraph {
    nodes: GraphNode[];
    edges: GraphEdge[];
}

export async function getVaultBacklinks(filePath: string): Promise<BacklinkResult> {
    return invoke<BacklinkResult>("get_vault_backlinks", { filePath });
}

export async function listVaultTags(): Promise<VaultTag[]> {
    return invoke<VaultTag[]>("list_vault_tags");
}

export async function getFilesByTag(tag: string): Promise<VaultFile[]> {
    return invoke<VaultFile[]>("get_files_by_tag", { tag });
}

export async function getVaultGraph(): Promise<VaultGraph> {
    return invoke<VaultGraph>("get_vault_graph");
}

// ─── Calendar Intelligence ─────────────────────────────────────────

export interface EnrichedAttendee {
    email: string;
    name: string;
    company: string;
    domain: string;
}

export interface CalendarEventEnriched {
    event_id: string;
    title: string;
    start_time: string;
    end_time: string;
    location: string | null;
    meeting_url: string | null;
    calendar_name: string;
    attendees: EnrichedAttendee[];
    attendee_count: number;
}

export interface MeetingIntelResult {
    event_title: string;
    attendees_count: number;
    companies_count: number;
    attendees: { name: string; email: string; company: string }[];
    companies: { name: string; domain: string }[];
}

export async function getEnrichedCalendarEvents(): Promise<CalendarEventEnriched[]> {
    return invoke<CalendarEventEnriched[]>("get_enriched_calendar_events");
}

export async function generateMeetingIntel(eventId: string, topicName: string): Promise<MeetingIntelResult> {
    return withAiConsent(() => invoke<MeetingIntelResult>("generate_meeting_intel", { eventId, topicName }));
}

// Calendar Integration — Meeting Attendees
export interface MeetingAttendee {
    id: number;
    meeting_id: string;
    name: string;
    email: string;
    company: string | null;
    role: string;
}

export interface CalendarMatchEvent {
    meeting_id: string;
    event_id: string;
    event_title: string;
    attendee_count: number;
    attendee_names: string[];
    attendee_emails: string[];
    start_time: string;
    end_time: string;
}

export async function getMeetingAttendees(meetingId: string): Promise<MeetingAttendee[]> {
    return invoke<MeetingAttendee[]>("get_meeting_attendees", { meetingId });
}

// Calendar Access
export async function checkCalendarAccess(): Promise<boolean> {
    return invoke<boolean>("check_calendar_access");
}

export async function requestCalendarAccess(): Promise<boolean> {
    return invoke<boolean>("request_calendar_access");
}

// Meeting Title Update
export async function updateMeetingTitle(meetingId: string, title: string): Promise<void> {
    return invoke<void>("update_meeting_title", { meetingId, title });
}

// People Lookup — AI-powered attendee enrichment
export interface AttendeeProfile {
    email: string;
    name: string;
    company: string;
    company_domain: string;
    briefing: string;
}

export interface CompanyProfile {
    domain: string;
    name: string;
    briefing: string;
}

export interface MeetingIntelPackage {
    event_title: string;
    attendees: AttendeeProfile[];
    companies: CompanyProfile[];
    meeting_prep: string;
}

export async function lookupAttendees(eventTitle: string, attendeeEmails: string[]): Promise<MeetingIntelPackage> {
    return withAiConsent(() => invoke<MeetingIntelPackage>("lookup_attendees", { eventTitle, attendeeEmails }));
}

// Recording-Calendar Overlap
export async function matchRecordingToCalendar(meetingId: string): Promise<CalendarMatchEvent | null> {
    return invoke<CalendarMatchEvent | null>("match_recording_to_calendar", { meetingId });
}

// Data Chatbot (RAG)
export interface ChatSource {
    id: string;
    summary: string;
    source: string;
    score: number | null;
    timestamp: string | null;
    app_name: string | null;
}

export interface ChatResponse {
    answer: string;
    sources: ChatSource[];
    context_count: number;
}

export interface ChatHistoryMessage {
    role: "user" | "assistant";
    content: string;
}

export async function chatWithData(
    message: string,
    history: ChatHistoryMessage[]
): Promise<ChatResponse> {
    return withAiConsent(() => invoke<ChatResponse>("chat_with_data", { message, history }));
}

// ============================================
// Meeting Report Prompt Commands
// ============================================

export interface MeetingReport {
    summary: string;
    key_topics: string[];
    decisions: { text: string; made_by: string | null; context: string | null }[];
    action_items: { task: string; assignee: string | null; due_date: string | null; priority: string | null }[];
    participants: string[];
}

export async function getMeetingReportPrompt(): Promise<string> {
    return invoke<string>("get_meeting_report_prompt");
}

export async function setMeetingReportPrompt(prompt: string): Promise<void> {
    return invoke("set_meeting_report_prompt", { prompt });
}

export async function generateMeetingReport(meetingId: string): Promise<MeetingReport> {
    return withAiConsent(() => invoke<MeetingReport>("generate_meeting_report", { meetingId }));
}

// ============================================
// Prompt Management Commands
// ============================================

export interface PromptRecord {
    id: string;
    name: string;
    description: string | null;
    category: string;
    system_prompt: string;
    user_prompt_template: string | null;
    model_id: string | null;
    temperature: number;
    max_tokens: number | null;
    theme: string | null;
    version: number;
    is_builtin: boolean;
    is_active: boolean;
    created_at: string;
    updated_at: string;
}

export interface PromptUpdate {
    name?: string;
    description?: string;
    category?: string;
    system_prompt?: string;
    user_prompt_template?: string;
    model_id?: string;
    temperature?: number;
    max_tokens?: number;
    is_active?: boolean;
}

export async function listPrompts(category?: string): Promise<PromptRecord[]> {
    return invoke<PromptRecord[]>("list_prompts", { category: category ?? null });
}

export async function getPrompt(id: string): Promise<PromptRecord | null> {
    return invoke<PromptRecord | null>("get_prompt", { id });
}

export async function updatePrompt(id: string, updates: PromptUpdate): Promise<PromptRecord | null> {
    return invoke<PromptRecord | null>("update_prompt", { id, updates });
}

export async function deletePrompt(id: string): Promise<boolean> {
    return invoke<boolean>("delete_prompt", { id });
}

export async function duplicatePrompt(id: string, newName: string): Promise<PromptRecord | null> {
    return invoke<PromptRecord | null>("duplicate_prompt", { id, newName });
}

export async function listPromptsByTheme(theme: string): Promise<PromptRecord[]> {
    return invoke<PromptRecord[]>("list_prompts_by_theme", { theme });
}

export async function testPrompt(promptId: string, testInput: string): Promise<string> {
    return withAiConsent(() => invoke<string>("test_prompt", { promptId, testInput }));
}

// ── Capture sources: which displays / windows are screenshotted ──────────

export type CaptureTarget =
    | { kind: "display"; id: number }
    | { kind: "window"; id: number };

export interface CaptureSource {
    target: CaptureTarget;
    title: string;
    app_name: string | null;
    width: number;
    height: number;
    is_primary: boolean;
    thumbnail: string | null;
}

export interface FrameCapturedEvent {
    state_id: string | null;
    meeting_id: string | null;
    path: string;
    source: string;
    label: string;
    timestamp: string;
    manual: boolean;
}

export const targetKey = (t: CaptureTarget) => `${t.kind}:${t.id}`;

export async function listCaptureSources(withThumbnails = true): Promise<CaptureSource[]> {
    return invoke("list_capture_sources", { withThumbnails });
}

export async function getCaptureTargets(): Promise<CaptureTarget[]> {
    return invoke("get_capture_targets");
}

export async function setCaptureTargets(targets: CaptureTarget[]): Promise<void> {
    return invoke("set_capture_targets", { targets });
}

export async function snapCaptureTarget(target: CaptureTarget): Promise<{ path: string; label: string }> {
    return invoke("snap_capture_target", { target });
}

// ── People (from the calendar) + LinkedIn ────────────────────────────────

export interface Person {
    id: string;
    email: string;
    name: string | null;
    company: string | null;
    company_domain: string | null;
    linkedin_url: string | null;
    notes: string | null;
    is_self: boolean;
    role: string | null;
    meeting_count: number;
    last_met: string | null;
}

export interface MeetingDetails {
    meeting_id: string;
    calendar_event_id: string | null;
    event_title: string | null;
    scheduled_start: string | null;
    scheduled_end: string | null;
    location: string | null;
    meeting_url: string | null;
    notes: string | null;
    calendar_name: string | null;
    organizer_email: string | null;
}

export interface MeetingPeople {
    details: MeetingDetails | null;
    people: Person[];
}

export type CalendarAccess = "authorized" | "denied" | "restricted" | "not_determined" | "unknown";

export interface CalendarSyncResult {
    access: CalendarAccess;
    report: { meetings_checked: number; meetings_linked: number; people_linked: number } | null;
}

export const syncCalendar = (relinkAll = false): Promise<CalendarSyncResult> =>
    invoke("sync_calendar", { relinkAll });

export const getCalendarAccessStatus = (): Promise<CalendarAccess> =>
    invoke("get_calendar_access_status");

export const getMeetingPeople = (meetingId: string): Promise<MeetingPeople> =>
    invoke("get_meeting_people", { meetingId });

export const listPeople = (): Promise<Person[]> => invoke("list_people");

export const setPersonLinkedin = (personId: string, url: string | null): Promise<string | null> =>
    invoke("set_person_linkedin", { personId, url });

/** LinkedIn has no lookup API; a prefilled people search is the honest shortcut. */
export const linkedinSearchUrl = (p: Pick<Person, "name" | "email" | "company">) => {
    const who = p.name || p.email.split("@")[0].replace(/[._-]+/g, " ");
    const q = [who, p.company].filter(Boolean).join(" ");
    return `https://www.linkedin.com/search/results/people/?keywords=${encodeURIComponent(q)}`;
};

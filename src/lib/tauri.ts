// noFriction Meetings - Tauri API Wrappers
// Type-safe wrappers for Tauri commands

import { invoke } from "@tauri-apps/api/core";

// Types
export interface RecordingStatus {
    is_recording: boolean;
    duration_seconds: number;
    video_frames: number;
    audio_samples: number;
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
}

export interface AppSettings {
    deepgram_api_key: string | null;
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
export async function setDeepgramApiKey(apiKey: string): Promise<void> {
    return invoke("set_deepgram_api_key", { apiKey });
}

export async function getDeepgramApiKey(): Promise<string | null> {
    return invoke<string | null>("get_deepgram_api_key");
}

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
}

export interface TimelineTranscript {
    id: string;
    timestamp_ms: number;
    text: string;
    speaker: string | null;
    is_final: boolean;
    duration_seconds: number;
}

export interface SyncedTimeline {
    meeting_id: string;
    meeting_title: string;
    duration_seconds: number;
    frames: TimelineFrame[];
    transcripts: TimelineTranscript[];
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

// Supabase commands
export async function configureSupabase(connectionString: string): Promise<void> {
    return invoke("configure_supabase", { connectionString });
}

export async function checkSupabase(): Promise<boolean> {
    return invoke<boolean>("check_supabase");
}

// Pinecone commands
export async function configurePinecone(apiKey: string, indexHost: string, namespace?: string): Promise<void> {
    return invoke("configure_pinecone", { apiKey, indexHost, namespace });
}

export async function checkPinecone(): Promise<boolean> {
    return invoke<boolean>("check_pinecone");
}

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

export async function syncToCloud(limit?: number): Promise<{ activities_synced: number; pinecone_upserts: number; supabase_inserts: number }> {
    return invoke("sync_to_cloud", { limit });
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
    sources?: string[];
}

export async function searchKnowledgeBase(options: SearchOptions): Promise<KBSearchResult[]> {
    return invoke<KBSearchResult[]>("search_knowledge_base", { options });
}

export async function quickSemanticSearch(query: string, limit?: number): Promise<KBSearchResult[]> {
    return invoke<KBSearchResult[]>("quick_semantic_search", { query, limit });
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

// Trigger manual ingest for a meeting
export async function triggerMeetingIngest(meetingId: string): Promise<string> {
    return invoke<string>("trigger_meeting_ingest", { meetingId });
}

// Debug logging to terminal
export async function debugLog(message: string): Promise<void> {
    return invoke("debug_log", { message });
}

// AI / LLM Commands
export async function aiChat(presetId: string, message: string, meetingId?: string): Promise<string> {
    return invoke<string>("ai_chat", {
        presetId,
        message,
        meetingId
    });
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
    timestamp_ms: number;
}

export async function getMeetingState(): Promise<MeetingState> {
    return invoke<MeetingState>("get_meeting_state");
}

export async function generateCatchUp(meetingId: string): Promise<CatchUpCapsule> {
    return invoke<CatchUpCapsule>("generate_catch_up", { meetingId });
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
// Meeting Intelligence & Window Management
// ============================================

export async function dismissMeetingDetection(detectionId: string): Promise<void> {
    return invoke("dismiss_meeting_detection", { detectionId });
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

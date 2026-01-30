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
    return invoke<string>("capture_screenshot", { monitor_id: monitorId });
}

// Transcript commands
export async function getTranscripts(meetingId: string): Promise<Transcript[]> {
    return invoke<Transcript[]>("get_transcripts", { meeting_id: meetingId });
}

export async function searchTranscripts(query: string): Promise<SearchResult[]> {
    return invoke<SearchResult[]>("search_transcripts", { query });
}

// Frame commands (for rewind timeline)
export async function getFrames(meetingId: string, limit?: number): Promise<Frame[]> {
    return invoke<Frame[]>("get_frames", { meeting_id: meetingId, limit });
}

export async function getFrameCount(meetingId: string): Promise<number> {
    return invoke<number>("get_frame_count", { meeting_id: meetingId });
}

// Device commands
export async function getAudioDevices(): Promise<AudioDevice[]> {
    return invoke<AudioDevice[]>("get_audio_devices");
}

export async function setAudioDevice(deviceId: string): Promise<void> {
    return invoke("set_audio_device", { device_id: deviceId });
}

export async function getMonitors(): Promise<MonitorInfo[]> {
    return invoke<MonitorInfo[]>("get_monitors");
}

export async function setMonitor(monitorId: number): Promise<void> {
    return invoke("set_monitor", { monitor_id: monitorId });
}

// Settings commands
export async function setDeepgramApiKey(apiKey: string): Promise<void> {
    return invoke("set_deepgram_api_key", { api_key: apiKey });
}

export async function getDeepgramApiKey(): Promise<string | null> {
    return invoke<string | null>("get_deepgram_api_key");
}

export async function getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>("get_settings");
}

// Set frame capture interval (milliseconds)
export async function setFrameCaptureInterval(intervalMs: number): Promise<void> {
    return invoke("set_frame_capture_interval", { interval_ms: intervalMs });
}

// Meeting commands
export async function getMeetings(limit?: number): Promise<Meeting[]> {
    return invoke<Meeting[]>("get_meetings", { limit });
}

export async function getMeeting(meetingId: string): Promise<Meeting | null> {
    return invoke<Meeting | null>("get_meeting", { meeting_id: meetingId });
}

export async function deleteMeeting(meetingId: string): Promise<void> {
    return invoke("delete_meeting", { meeting_id: meetingId });
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
    return invoke("configure_supabase", { connection_string: connectionString });
}

export async function checkSupabase(): Promise<boolean> {
    return invoke<boolean>("check_supabase");
}

// Pinecone commands
export async function configurePinecone(apiKey: string, indexHost: string, namespace?: string): Promise<void> {
    return invoke("configure_pinecone", { api_key: apiKey, index_host: indexHost, namespace });
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
    return invoke("start_video_recording", { meeting_id: meetingId });
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
        meeting_id: meetingId,
        chunk_number: chunkNumber,
        timestamp_secs: timestampSecs,
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
        meeting_id: meetingId,
        chunk_number: chunkNumber,
        timestamp_secs: timestampSecs,
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
    return invoke<number>("delete_video_storage", { meeting_id: meetingId });
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
    return invoke<void>("set_theme_interval", { theme, interval_ms: intervalMs });
}

// Get time spent in a theme today (in hours)
// Get time spent in a theme today (in hours)
export async function getThemeTimeToday(theme: string): Promise<number> {
    return invoke<number>("get_theme_time_today", { theme });
}

// Trigger manual ingest for a meeting
export async function triggerMeetingIngest(meetingId: string): Promise<string> {
    return invoke<string>("trigger_meeting_ingest", { meeting_id: meetingId });
}

// Debug logging to terminal
export async function debugLog(message: string): Promise<void> {
    return invoke("debug_log", { message });
}


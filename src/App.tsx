// noFriction Meetings - Main App
// Single-binary macOS meeting transcription app with rewind timeline

import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { LiveTranscriptView } from "./components/LiveTranscript";
import { RecordingControls } from "./components/RecordingControls";
import { MeetingHistory } from "./components/MeetingHistory";
import { SearchBar } from "./components/SearchBar";
import { RewindGallery } from "./components/RewindGallery";
import { FullSettings } from "./components/FullSettings";
import { AIChat } from "./components/AIChat";
import { PromptLibrary } from "./components/PromptLibrary";
import { CommandPalette, useCommandPalette } from "./components/CommandPalette";
import { useRecording } from "./hooks/useRecording";
import { useTranscripts } from "./hooks/useTranscripts";

type Tab = "live" | "rewind" | "settings" | "prompts";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("live");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [showAIPanel, setShowAIPanel] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Hooks
  const recording = useRecording();
  const transcripts = useTranscripts(recording.meetingId);
  const commandPalette = useCommandPalette();

  // Global keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // ⌘R - Toggle recording
      if ((e.metaKey || e.ctrlKey) && e.key === "r") {
        e.preventDefault();
        if (recording.isRecording) {
          recording.stopRecording();
        } else {
          transcripts.clearLiveTranscripts();
          recording.startRecording();
        }
      }
      // ⌘\ - Toggle sidebar
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        e.preventDefault();
        setSidebarCollapsed(prev => !prev);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [recording, transcripts]);

  // Listen for native menu events
  useEffect(() => {
    const listeners: (() => void)[] = [];

    const setupListeners = async () => {
      // Navigation events
      listeners.push(await listen("menu:view_live", () => setActiveTab("live")));
      listeners.push(await listen("menu:view_rewind", () => setActiveTab("rewind")));
      listeners.push(await listen("menu:view_settings", () => setActiveTab("settings")));
      listeners.push(await listen("menu:view_prompts", () => setActiveTab("prompts")));

      // Command palette
      listeners.push(await listen("menu:command_palette", () => commandPalette.open()));

      // Recording events
      listeners.push(await listen("menu:new_recording", async () => {
        if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }
      }));
      listeners.push(await listen("menu:stop_recording", async () => {
        if (recording.isRecording) {
          await recording.stopRecording();
        }
      }));

      // AI events - open AI slide panel
      listeners.push(await listen("menu:summarize", () => {
        setShowAIPanel(true);
      }));
      listeners.push(await listen("menu:action_items", () => {
        setShowAIPanel(true);
      }));
      listeners.push(await listen("menu:ask_ai", () => setShowAIPanel(true)));
    };

    setupListeners();

    return () => {
      listeners.forEach(unlisten => unlisten());
    };
  }, [recording, transcripts, commandPalette]);

  const handleToggleRecording = async () => {
    try {
      if (recording.isRecording) {
        await recording.stopRecording();
      } else {
        transcripts.clearLiveTranscripts();
        await recording.startRecording();
      }
    } catch (err) {
      console.error("Recording error:", err);
    }
  };

  const handleSelectMeeting = (meetingId: string) => {
    setSelectedMeetingId(meetingId);
    transcripts.loadTranscripts(meetingId);
    // Don't force tab switch - user can navigate freely
  };

  // Determine which meeting to show in rewind/AI
  const rewindMeetingId = recording.isRecording
    ? recording.meetingId
    : selectedMeetingId;

  return (
    <div className="app-container">
      {/* Header */}
      <header className="app-header glass-panel">
        <div className="app-title">
          <h1>noFriction Meetings</h1>
          <span className="subtitle">Live Transcription & AI Analysis</span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
          {recording.isRecording && (
            <div className="badge badge-error" style={{ animation: "pulse 2s infinite" }}>
              ● Recording
            </div>
          )}
          {recording.error && (
            <div className="badge badge-warning" title={recording.error}>
              ⚠️ Error
            </div>
          )}
        </div>
      </header>

      {/* Main Content */}
      <div className="main-content">
        {/* Main Panel - Full Width Content Area */}
        <div className="main-panel glass-panel">
          <div className="panel-header">
            <div className="tabs">
              <button
                className={`tab ${activeTab === "live" ? "active" : ""}`}
                onClick={() => setActiveTab("live")}
              >
                📝 Live
              </button>
              <button
                className={`tab ${activeTab === "rewind" ? "active" : ""}`}
                onClick={() => setActiveTab("rewind")}
              >
                🎬 Rewind
              </button>
              <div className="tab-spacer" />
              <button
                className={`tab tab-toggle ${showAIPanel ? "active" : ""}`}
                onClick={() => setShowAIPanel(!showAIPanel)}
                title="Toggle AI Assistant (⌘⇧I)"
              >
                🤖 AI
              </button>
            </div>
          </div>

          {/* Tab Content */}
          <div className="panel-content scrollable">
            {/* Main Views (Live / Rewind) */}
            {(activeTab === "live" || activeTab === "rewind") && (
              <div className="content-with-ai-panel">
                <div className="main-view-area">
                  {activeTab === "live" && (
                    <div className="live-view">
                      <SearchBar
                        onSearch={transcripts.search}
                        isSearching={transcripts.isSearching}
                      />
                      <div className="live-transcripts scrollable">
                        {transcripts.searchResults.length > 0 ? (
                          <div className="search-results">
                            {transcripts.searchResults.map((result, idx) => (
                              <div
                                key={idx}
                                className="search-result-item"
                                onClick={() => handleSelectMeeting(result.meeting_id)}
                              >
                                <div className="result-header">
                                  <span className="result-title">{result.meeting_title}</span>
                                  <span className="result-date">
                                    {new Date(result.timestamp).toLocaleDateString()}
                                  </span>
                                </div>
                                <p className="result-text">{result.transcript_text}</p>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <LiveTranscriptView
                            transcripts={transcripts.liveTranscripts}
                            isRecording={recording.isRecording}
                          />
                        )}
                      </div>
                    </div>
                  )}

                  {activeTab === "rewind" && (
                    <RewindGallery
                      meetingId={rewindMeetingId}
                      isRecording={recording.isRecording}
                    />
                  )}
                </div>

                {/* AI Slide Panel */}
                <div className={`ai-slide-panel ${showAIPanel ? "open" : ""}`}>
                  <div className="ai-panel-header">
                    <span>🤖 AI Assistant</span>
                    <button
                      className="ai-panel-close"
                      onClick={() => setShowAIPanel(false)}
                      title="Close AI Panel"
                    >
                      ✕
                    </button>
                  </div>
                  <AIChat meetingId={rewindMeetingId} />
                </div>
              </div>
            )}

            {/* Settings (accessed via ⌘,) */}
            {activeTab === "settings" && (
              <FullSettings />
            )}

            {/* Prompts (accessed via ⌘⇧P) */}
            {activeTab === "prompts" && (
              <PromptLibrary />
            )}
          </div>
        </div>

        {/* Sidebar - Recording Controls */}
        <div className={`sidebar ${sidebarCollapsed ? "collapsed" : ""}`}>
          <button
            className="sidebar-toggle"
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            title={sidebarCollapsed ? "Expand Sidebar (⌘\\)" : "Collapse Sidebar (⌘\\)"}
          >
            {sidebarCollapsed ? "▶" : "◀"}
          </button>

          <RecordingControls
            isRecording={recording.isRecording}
            isPaused={recording.isPaused}
            duration={recording.duration}
            videoFrames={recording.videoFrames}
            audioSamples={recording.audioSamples}
            onToggle={handleToggleRecording}
            onPause={recording.pauseRecording}
          />

          {!sidebarCollapsed && (
            <div className="sidebar-meetings glass-panel">
              <h3>Past Meetings</h3>
              <div className="meetings-list scrollable">
                <MeetingHistory
                  onSelectMeeting={handleSelectMeeting}
                  selectedMeetingId={selectedMeetingId}
                  compact
                />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Command Palette (⌘K) */}
      <CommandPalette
        isOpen={commandPalette.isOpen}
        onClose={commandPalette.close}
        onNavigate={(tab) => setActiveTab(tab as Tab)}
        onStartRecording={async () => {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }}
        onStopRecording={recording.stopRecording}
        isRecording={recording.isRecording}
        currentMeetingId={recording.meetingId}
      />
    </div>
  );
}

export default App;

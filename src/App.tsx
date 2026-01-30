// noFriction Meetings - Sidebar Layout
import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { HelpSection } from "./components/Help";
import { debugLog } from "./lib/tauri";
import "./App.css";
import { Sidebar } from "./components/Sidebar";
import { LiveTranscriptView } from "./components/LiveTranscript";
import { MeetingHistory } from "./components/MeetingHistory";
import { SearchBar } from "./components/SearchBar";
import { RewindGallery } from "./components/RewindGallery";
import { FullSettings } from "./components/FullSettings";
import { KBSearch } from "./components/KBSearch";
import { InsightsView } from "./components/InsightsView";
import { EntitiesView } from "./components/EntitiesView";
import { ActivityTimeline } from "./components/ActivityTimeline";
import { CommandPalette, useCommandPalette } from "./components/CommandPalette";
import { SetupWizard, useSetupRequired } from "./components/SetupWizard";
import { AdminConsole } from "./components/AdminConsole";
import { useRecording } from "./hooks/useRecording";
import { useTranscripts } from "./hooks/useTranscripts";

type Tab = "live" | "rewind" | "kb" | "insights" | "intel" | "timeline" | "admin" | "settings" | "help";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("live");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [meetingListRefreshKey, setMeetingListRefreshKey] = useState(0);

  const recording = useRecording();
  const transcripts = useTranscripts(recording.meetingId);
  const commandPalette = useCommandPalette();
  const setupRequired = useSetupRequired();

  // Menu event listeners
  useEffect(() => {
    const listeners: (() => void)[] = [];

    const setupListeners = async () => {
      listeners.push(await listen("menu:search", () => setActiveTab("kb")));
      listeners.push(await listen("menu:insights", () => setActiveTab("insights")));
      listeners.push(await listen("menu:settings", () => setActiveTab("settings")));
    };

    setupListeners();
    return () => listeners.forEach(unlisten => unlisten());
  }, []);

  // Hooks must be unconditional
  const [isBackendReady, setIsBackendReady] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [isLongLoading, setIsLongLoading] = useState(false);

  // Listen for startup events and poll as fallback
  useEffect(() => {
    let unlistenReady: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;
    let pollInterval: number | null = null;

    const setupListeners = async () => {
      try {
        console.log("Setting up event listeners...");
        unlistenReady = await listen("app-ready", () => {
          console.log("Event: app-ready");
          setIsBackendReady(true);
        });
        unlistenError = await listen<string>("init-error", (e) => {
          console.error("Event: init-error", e);
          setInitError(e.payload);
        });
      } catch (e) {
        console.error("Failed to setup listeners:", e);
      }
    };
    setupListeners();

    // Safer Polling fallback using check_init_status
    const pollBackend = async () => {
      try {
        const status = await invoke<{ "Ready": null } | { "Depending": null } | { "Failed": string } | "Initializing" | "Ready">("check_init_status");
        console.log("Poll status:", status);

        if (status === "Ready" || (typeof status === 'object' && 'Ready' in status)) {
          console.log("Backend Ready confirmed via poll");
          setIsBackendReady(true);
        } else if (typeof status === 'object' && 'Failed' in status) {
          // @ts-ignore
          const errorMsg = status.Failed;
          console.error("Backend Failed via poll:", errorMsg);
          setInitError(errorMsg);
        }
      } catch (e) {
        // Command might not be registered yet if very early
      }
    };

    pollInterval = window.setInterval(() => {
      // Stop polling to be safe
      if (isBackendReady || initError) {
        if (pollInterval) clearInterval(pollInterval);
        return;
      }
      pollBackend();
    }, 500);

    // Timeout warning
    const timeout = setTimeout(() => setIsLongLoading(true), 8000);

    return () => {
      if (unlistenReady) unlistenReady();
      if (unlistenError) unlistenError();
      if (pollInterval) clearInterval(pollInterval);
      clearTimeout(timeout);
    };
  }, [isBackendReady, initError]);

  if (setupRequired === null) {
    return (
      <div className="app-loading">
        <div className="loading-spinner" />
      </div>
    );
  }

  if (setupRequired) {
    return <SetupWizard onComplete={() => window.location.reload()} />;
  }

  const handleToggleRecording = async () => {
    try {
      if (recording.isRecording) {
        await recording.stopRecording();
        // Refresh meeting list after recording stops
        setMeetingListRefreshKey((k) => k + 1);
      } else {
        transcripts.clearLiveTranscripts();
        await recording.startRecording();
      }
    } catch (err) {
      console.error("Recording error:", err);
    }
  };

  if (setupRequired === null || !isBackendReady || initError) {
    return (
      <div className="app-loading" style={{ flexDirection: 'column', gap: '16px', background: '#1a1d29', color: 'white' }}>
        {initError ? (
          <>
            <div style={{ fontSize: '48px' }}>⚠️</div>
            <h2 style={{ fontSize: '20px', fontWeight: 600 }}>Startup Failed</h2>
            <p style={{ color: '#ef4444', maxWidth: '400px', textAlign: 'center', background: 'rgba(0,0,0,0.2)', padding: '12px', borderRadius: '8px' }}>
              {initError}
            </p>
            <button onClick={() => window.location.reload()} className="btn btn-secondary" style={{ marginTop: '16px' }}>Retry</button>
          </>
        ) : (
          <>
            <div className="loading-spinner" style={{ borderColor: 'rgba(255,255,255,0.1)', borderTopColor: '#7c3aed' }} />
            <div>
              <p style={{ color: '#e5e7eb', fontSize: 14, fontWeight: 500 }}>Initializing noFriction Meetings...</p>
              {isLongLoading && (
                <p style={{ color: '#9ca3af', fontSize: 12, marginTop: '8px' }}>
                  Taking longer than expected. Please wait...
                </p>
              )}
            </div>
          </>
        )}
      </div>
    );
  }

  // When a meeting is selected
  const handleMeetingSelect = (meetingId: string) => {
    debugLog(`Meeting selected: ${meetingId}`);
    setSelectedMeetingId(meetingId);
    transcripts.loadTranscripts(meetingId);

    // Only switch to rewind if we're not already on timeline
    // This preserves the current tab when selecting meetings from Timeline view
    if (activeTab !== "timeline") {
      setActiveTab("rewind");
    }
  };

  const rewindMeetingId = selectedMeetingId || (recording.isRecording ? recording.meetingId : null);

  // Tab content titles
  const tabTitles: Record<Tab, string> = {
    live: "Live Transcription",
    rewind: "Meeting Playback",
    kb: "Knowledge Base",
    insights: "Activity Insights",
    intel: "Deep Intel",
    timeline: "Activity Timeline",
    admin: "Management Suite",
    settings: "Settings",
    help: "Help & Documentation",
  };

  return (
    <div className="app-container">
      {/* Left Sidebar */}
      <Sidebar
        activeTab={activeTab}
        onTabChange={(tab) => setActiveTab(tab as Tab)}
        recording={recording}
        onToggleRecording={handleToggleRecording}
      />

      {/* Main Content Area */}
      <div className="main-content">
        <div className="content-header">
          <h1 className="content-title">{tabTitles[activeTab]}</h1>
          <p className="content-subtitle">
            {recording.isRecording && "● Recording in progress"}
            {recording.error && `⚠️ ${recording.error}`}
          </p>
        </div>

        <div className="content-body">
          {/* Live View */}
          {activeTab === "live" && (
            <>
              <div className="search-container">
                <SearchBar
                  onSearch={transcripts.search}
                  isSearching={transcripts.isSearching}
                />
              </div>
              {transcripts.searchResults.length > 0 ? (
                <div className="search-results">
                  {transcripts.searchResults.map((result, idx) => (
                    <div
                      key={idx}
                      className="meeting-card"
                      onClick={() => handleMeetingSelect(result.meeting_id)}
                    >
                      <div className="meeting-title">{result.meeting_title}</div>
                      <div className="meeting-date">
                        {new Date(result.timestamp).toLocaleDateString()}
                      </div>
                      <p style={{ marginTop: '8px', fontSize: '14px', color: '#6b7280' }}>
                        {result.transcript_text}
                      </p>
                    </div>
                  ))}
                </div>
              ) : (
                <LiveTranscriptView
                  transcripts={transcripts.liveTranscripts}
                  isRecording={recording.isRecording}
                />
              )}
            </>
          )}

          {/* Rewind View */}
          {activeTab === "rewind" && (
            <RewindGallery
              meetingId={rewindMeetingId}
              isRecording={recording.isRecording}
            />
          )}

          {/* Knowledge Base */}
          {activeTab === "kb" && <KBSearch />}

          {/* Insights */}
          {activeTab === "insights" && <InsightsView />}

          {/* Settings */}
          {activeTab === "settings" && <FullSettings />}

          {/* Intel */}
          {activeTab === "intel" && <EntitiesView />}

          {/* Activity Timeline */}
          {activeTab === "timeline" && (
            <ActivityTimeline meetingId={rewindMeetingId} />
          )}

          {/* Admin Console */}
          {activeTab === "admin" && <AdminConsole />}
          {activeTab === "help" && <HelpSection />}
        </div>
      </div>

      {/* Right Panel - Meeting History (on Rewind and Timeline tabs) */}
      {(activeTab === "rewind" || activeTab === "timeline") && (
        <div className="right-panel">
          <div className="panel-header">
            <h2 className="panel-title">Recent Meetings</h2>
          </div>
          <div className="panel-body">
            <MeetingHistory
              onSelectMeeting={handleMeetingSelect}
              selectedMeetingId={selectedMeetingId}
              refreshKey={meetingListRefreshKey}
              compact
            />
          </div>
        </div>
      )}

      {/* Command Palette */}
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

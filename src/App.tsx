// noFriction Meetings - Sidebar Layout
import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { CommandPalette, useCommandPalette } from "./components/CommandPalette";
import { debugLog } from "./lib/tauri";
import "./App.css";
import { MeetingDetectionBanner } from "./components/MeetingDetectionBanner";
import { SetupWizard, useSetupRequired } from "./features/onboarding/SetupWizard";
import { useRecording } from "./hooks/useRecording";
import { useTranscripts } from "./hooks/useTranscripts";
import { GenieView } from "./components/GenieView";
import { AgencyLayout, AgencyMode } from "./components/agency/AgencyLayout";


function App() {
  const [activeMode, setActiveMode] = useState<AgencyMode>("flow");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [meetingListRefreshKey, setMeetingListRefreshKey] = useState(0);

  const recording = useRecording();
  const transcripts = useTranscripts(recording.meetingId);
  const commandPalette = useCommandPalette();
  const setupRequired = useSetupRequired();
  const [isGenieMode, setIsGenieMode] = useState(false);
  const isGenieModeRef = useRef(isGenieMode);
  isGenieModeRef.current = isGenieMode;

  // Menu event listeners
  useEffect(() => {
    const listeners: (() => void)[] = [];

    const setupListeners = async () => {
      listeners.push(await listen("menu:search", () => setActiveMode("deck")));
      listeners.push(await listen("menu:insights", () => setActiveMode("deck")));
      listeners.push(await listen("menu:settings", () => setActiveMode("deck")));
      // Tray menu events
      listeners.push(await listen("tray:start_recording", async () => {
        if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }
      }));
      listeners.push(await listen("tray:stop_recording", async () => {
        if (recording.isRecording) {
          await recording.stopRecording();
          setMeetingListRefreshKey((k) => k + 1);
        }
      }));
      listeners.push(await listen("enter-genie-mode", async () => {
        if (!isGenieModeRef.current) {
          await invoke("set_genie_mode", { isGenie: true });
          setIsGenieMode(true);
        }
      }));
    };

    setupListeners();
    return () => listeners.forEach(unlisten => unlisten());
  }, [recording, transcripts]);

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
    setActiveMode("deck");
  };


  if (isGenieMode) {
    return (
      <GenieView
        onRestore={() => setIsGenieMode(false)}
        liveTranscripts={transcripts.liveTranscripts.map(t => t.text)}
        isRecording={recording.isRecording}
        onStop={async () => {
          await recording.stopRecording();
          setMeetingListRefreshKey((k) => k + 1);
        }}
        meetingId={recording.meetingId}
      />
    );
  }

  return (
    <div className={`app-container ${isBackendReady ? 'ready' : ''}`}>
      <AgencyLayout
        activeMode={activeMode}
        onModeChange={setActiveMode}
        recording={recording}
        transcripts={transcripts}
        onSelectMeeting={handleMeetingSelect}
        selectedMeetingId={selectedMeetingId}
        onToggleRecording={handleToggleRecording}
        refreshKey={meetingListRefreshKey}
      />

      {/* Meeting Detection Banner - shows when meetings detected */}
      {!recording.isRecording && activeMode === 'flow' && (
        <div style={{ position: 'fixed', bottom: 20, left: '50%', transform: 'translateX(-50%)', zIndex: 100 }}>
          <MeetingDetectionBanner
            onStartRecording={async () => {
              transcripts.clearLiveTranscripts();
              await recording.startRecording();
            }}
          />
        </div>
      )}

      {/* Command Palette */}
      <CommandPalette
        isOpen={commandPalette.isOpen}
        onClose={commandPalette.close}
        onNavigate={(tab: string) => {
          // Map legacy tabs to modes roughly
          if (tab === 'live') setActiveMode('flow');
          else setActiveMode('deck');
        }}
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

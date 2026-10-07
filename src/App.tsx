// noFriction Meetings - Sidebar Layout
import { useState, useEffect, useRef, useCallback } from "react";
import { AiConsentModal } from "./components/AiConsentModal";
import { PaywallModal } from "./components/PaywallModal";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { CommandPalette, useCommandPalette } from "./components/CommandPalette";
import { debugLog } from "./lib/tauri";
import "./App.css";
import { MeetingEndBanner } from "./components/MeetingEndBanner";
import { SetupWizard, useSetupRequired } from "./features/onboarding/SetupWizard";
import { isOffline } from "./lib/offline";
import { useRecording } from "./hooks/useRecording";
import { useTranscripts } from "./hooks/useTranscripts";
import { GenieView } from "./components/GenieView";
import { AgencyLayout, AgencyMode } from "./components/agency/AgencyLayout";
import { openSettings } from "./lib/navigation";
import { RecordPicker, RecordPickerContext } from "./components/RecordPicker";
import { ClassRecordingNotice, TimeLimitBanner } from "./components/TimedRecording";
import { CLASS_NOTICE_EVENT, TIMED_EVENTS, type TimedAutoStop } from "./lib/timedRecording";
import type { StartPlan } from "./lib/recordPlan";


function App() {
  const [activeMode, setActiveMode] = useState<AgencyMode>("flow");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [meetingListRefreshKey, setMeetingListRefreshKey] = useState(0);

  const recording = useRecording();
  const transcripts = useTranscripts(recording.meetingId);
  const commandPalette = useCommandPalette();
  const [setupRequired, setSetupRequired] = useSetupRequired();
  const [isGenieMode, setIsGenieMode] = useState(false);
  // The Record sheet (Record button clicks) and the one-time class notice
  const [recordPickerOpen, setRecordPickerOpen] = useState(false);
  const [showClassNotice, setShowClassNotice] = useState(false);
  const closeClassNotice = useCallback(() => setShowClassNotice(false), []);
  // A recording started some other way (tray, ⌘N) while the sheet was open
  useEffect(() => {
    if (recording.isRecording) setRecordPickerOpen(false);
  }, [recording.isRecording]);
  const isGenieModeRef = useRef(isGenieMode);
  isGenieModeRef.current = isGenieMode;

  // Latest hook values for the (register-once) native event handlers below
  const recordingRef = useRef(recording);
  recordingRef.current = recording;
  const transcriptsRef = useRef(transcripts);
  transcriptsRef.current = transcripts;
  const commandPaletteRef = useRef(commandPalette);
  commandPaletteRef.current = commandPalette;

  // Menu event listeners — registered once. Re-registering on every render
  // raced the async listen() calls against cleanup and leaked handlers, so a
  // single tray click could start or stop recording several times.
  useEffect(() => {
    const listeners: (() => void)[] = [];
    let disposed = false;
    const add = (off: () => void) => (disposed ? off() : listeners.push(off));

    const setupListeners = async () => {
      // Read through refs so handlers always see current state
      const recording = {
        get isRecording() { return recordingRef.current.isRecording; },
        get isPaused() { return recordingRef.current.isPaused; },
        get meetingId() { return recordingRef.current.meetingId; },
        // No plan: the remembered type and length (shortcut, tray, capture modes)
        startRecording: (plan?: StartPlan) => recordingRef.current.startRecording(plan),
        stopRecording: () => recordingRef.current.stopRecording(),
        pauseRecording: () => recordingRef.current.pauseRecording(),
        resumeRecording: () => recordingRef.current.resumeRecording(),
      };
      const transcripts = { clearLiveTranscripts: () => transcriptsRef.current.clearLiveTranscripts() };
      add(await listen("menu:search", () => setActiveMode("deck")));
      add(await listen("menu:insights", () => setActiveMode("deck")));
      // Menu bar (menu_builder.rs) and tray: every item does something
      add(await listen("menu:settings", () => openSettings("general")));
      add(await listen("menu:view_settings", () => openSettings("general")));
      add(await listen("menu:view_live", () => setActiveMode("flow")));
      add(await listen("menu:view_rewind", () => setActiveMode("deck")));
      add(await listen("menu:view_prompts", () => setActiveMode("prompts")));
      add(await listen("menu:ask_ai", () => setActiveMode("chat")));
      add(await listen("menu:help", () => setActiveMode("help")));
      add(await listen("menu:command_palette", () => commandPaletteRef.current.open()));
      add(await listen("menu:new_recording", async () => {
        if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        } else if (recording.isPaused) {
          await recording.resumeRecording();
        }
      }));
      add(await listen("menu:stop_recording", async () => {
        if (recording.isRecording) {
          await recording.stopRecording();
          setMeetingListRefreshKey((k) => k + 1);
        }
      }));
      // Tray menu events. "Start Recording" uses the remembered type and
      // length; "Start Recording For > 30 Minutes" sends a length (and
      // remembers it) and records the remembered type.
      add(await listen<{ duration?: string } | null>("tray:start_recording", async (e) => {
        if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          const duration = e.payload?.duration;
          await recording.startRecording(duration ? { duration, remember: true } : undefined);
        }
      }));
      add(await listen("tray:stop_recording", async () => {
        if (recording.isRecording) {
          await recording.stopRecording();
          setMeetingListRefreshKey((k) => k + 1);
        }
      }));
      // Meeting-end detection: the countdown ran out — stop through the
      // same path as the user's Stop (video, accessibility, notes/report)
      add(await listen("meeting-end-auto-stop", async () => {
        if (recording.isRecording) {
          try {
            await recording.stopRecording();
          } catch (err) {
            console.error("Auto-stop failed:", err);
          }
          setMeetingListRefreshKey((k) => k + 1);
        }
      }));
      // Time limit reached (timed_recording.rs): stop through the user's own
      // Stop path. Only the recording the limit belongs to; the backend
      // stops it itself if this doesn't happen within a few seconds.
      add(await listen<TimedAutoStop>(TIMED_EVENTS.autoStop, async (e) => {
        const target = e.payload?.meetingId;
        if (recording.isRecording && (!target || target === recording.meetingId)) {
          try {
            await recording.stopRecording();
          } catch (err) {
            console.error("Timed auto-stop failed:", err);
          }
          setMeetingListRefreshKey((k) => k + 1);
        }
      }));
      // Backend had to stop it itself (UI didn't respond in time)
      add(await listen("recording-stopped-automatically", () => {
        setMeetingListRefreshKey((k) => k + 1);
      }));
      add(await listen("tray:pause_recording", async () => {
        if (recording.isRecording && !recording.isPaused) {
          await recording.pauseRecording();
        }
      }));
      add(await listen("tray:resume_recording", async () => {
        if (recording.isRecording && recording.isPaused) {
          await recording.resumeRecording();
        }
      }));
      // Capture mode events from tray
      add(await listen("menu:mode_ambient", async () => {
        if (recording.isPaused) {
          await recording.resumeRecording();
        } else if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }
      }));
      add(await listen("menu:mode_meeting", async () => {
        if (recording.isPaused) {
          await recording.resumeRecording();
        } else if (!recording.isRecording) {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }
      }));
      add(await listen("menu:mode_pause", async () => {
        if (recording.isRecording && !recording.isPaused) {
          await recording.pauseRecording();
        }
      }));
      add(await listen("enter-genie-mode", async () => {
        if (!isGenieModeRef.current) {
          await invoke("set_genie_mode", { isGenie: true });
          setIsGenieMode(true);
        }
      }));
      // First Class-type recording ever (any start path; recording_kind.rs):
      // a one-time reminder about school policy
      add(await listen(CLASS_NOTICE_EVENT, () => setShowClassNotice(true)));
      // Calendar integration: log when recording matches a calendar event
      add(await listen<{ event_title: string; attendee_count: number; attendee_names: string[] }>("calendar_match", (event) => {
        const { event_title, attendee_count, attendee_names } = event.payload;
        const names = attendee_names.slice(0, 3).join(", ");
        const extra = attendee_count > 3 ? ` +${attendee_count - 3} more` : "";
        console.log(`📅 Recording linked to "${event_title}" — ${attendee_count} attendees: ${names}${extra}`);
      }));
    };

    setupListeners();
    return () => {
      disposed = true;
      listeners.forEach((unlisten) => unlisten());
    };
  }, []);

  // Hooks must be unconditional
  const [isBackendReady, setIsBackendReady] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [isLongLoading, setIsLongLoading] = useState(false);

  // Listen for startup events and poll as fallback
  useEffect(() => {
    // Browser preview (no Tauri IPC): mock mode — skip backend readiness gate
    if (isOffline()) {
      setIsBackendReady(true);
      return;
    }

    let unlistenReady: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;
    let pollInterval: number | null = null;

    const setupListeners = async () => {
      try {
        unlistenReady = await listen("app-ready", () => {
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

        if (status === "Ready" || (typeof status === 'object' && 'Ready' in status)) {
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

  const handleToggleRecording = async () => {
    try {
      if (recording.isRecording) {
        await recording.stopRecording();
        // Refresh meeting list after recording stops
        setMeetingListRefreshKey((k) => k + 1);
      } else {
        // A Record button: ask "What is it?" and "How long?" first
        setRecordPickerOpen(true);
      }
    } catch (err) {
      console.error("Recording error:", err);
    }
  };

  const openRecordPicker = () => {
    if (!recording.isRecording) setRecordPickerOpen(true);
  };

  // The Record sheet's Start (an error stays in the sheet)
  const startFromPicker = async (plan: StartPlan) => {
    transcripts.clearLiveTranscripts();
    await recording.startRecording(plan);
    setRecordPickerOpen(false);
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
            <div className="loading-spinner" style={{ borderColor: 'rgba(255,255,255,0.1)', borderTopColor: 'var(--accent-primary, #6366f1)' }} />
            <div>
              <p style={{ color: '#e5e7eb', fontSize: 14, fontWeight: 500 }}>Starting noFriction…</p>
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

  // First run (or re-run from Settings → General). Shown once the backend
  // is ready: the steps call commands that need the database and models dir.
  if (setupRequired) {
    return <SetupWizard onComplete={() => setSetupRequired(false)} />;
  }

  // When a meeting is selected
  const handleMeetingSelect = (meetingId: string) => {
    debugLog(`Meeting selected: ${meetingId}`);
    setSelectedMeetingId(meetingId);
    transcripts.loadTranscripts(meetingId);
    setActiveMode("deck");
  };


  const meetingEndBanner = (
    <MeetingEndBanner
      isRecording={recording.isRecording}
      onStopNow={async () => {
        await recording.stopRecording();
        setMeetingListRefreshKey((k) => k + 1);
      }}
    />
  );

  if (isGenieMode) {
    return (
      <>
      {meetingEndBanner}
      <TimeLimitBanner isRecording={recording.isRecording} />
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
      </>
    );
  }

  return (
    <RecordPickerContext.Provider value={{ open: openRecordPicker }}>
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
        onOpenCommandPalette={commandPalette.open}
      />

      {/* "Send recording content to your endpoint?" (App Review 5.1.2(i)) */}
      <AiConsentModal />
      <PaywallModal />

      {/* "This seems to have ended — stopping in 30s" */}
      {meetingEndBanner}

      {/* Timed recording: "5 minutes left" with +15 min / No limit */}
      <TimeLimitBanner isRecording={recording.isRecording} />
      {recordPickerOpen && !recording.isRecording && (
        <RecordPicker onCancel={() => setRecordPickerOpen(false)} onStart={startFromPicker} />
      )}
      {showClassNotice && <ClassRecordingNotice onClose={closeClassNotice} />}

      {/* Command Palette */}
      <CommandPalette
        isOpen={commandPalette.isOpen}
        onClose={commandPalette.close}
        onNavigate={(tab: string) => {
          if (tab === 'live') setActiveMode('flow');
          else if (tab === 'chat') setActiveMode('chat');
          else if (tab === 'prompts') setActiveMode('prompts');
          else if (tab === 'help') setActiveMode('help');
          else if (tab === 'settings') openSettings('general');
          else if (tab === 'settings:ai') openSettings('ai');
          else setActiveMode('deck');
        }}
        onSelectMeeting={handleMeetingSelect}
        onStartRecording={async () => {
          transcripts.clearLiveTranscripts();
          await recording.startRecording();
        }}
        onStopRecording={recording.stopRecording}
        isRecording={recording.isRecording}
        currentMeetingId={recording.meetingId}
      />
    </div>
    </RecordPickerContext.Provider>
  );
}

export default App;

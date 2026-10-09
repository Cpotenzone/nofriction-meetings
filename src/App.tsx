// noFriction - the app
//
// Three views (Record · Recordings · Chat), the Record sheet, the setup
// screens on first run, and the native menu / tray / shortcut events.
import { useState, useEffect, useRef, useCallback } from "react";
import { AiConsentModal } from "./components/AiConsentModal";
import { PaywallModal } from "./components/PaywallModal";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { debugLog } from "./lib/tauri";
import "./App.css";
import { SetupWizard, useSetupRequired } from "./features/onboarding/SetupWizard";
import { isOffline } from "./lib/offline";
import { useRecording } from "./hooks/useRecording";
import { useTranscripts } from "./hooks/useTranscripts";
import { Shell, type AppMode } from "./components/Shell";
import { openHelp, openSettings, requestSearchFocus } from "./lib/navigation";
import { RecordPicker, RecordPickerContext } from "./components/RecordPicker";
import { ClassRecordingNotice } from "./components/TimedRecording";
import { CLASS_NOTICE_EVENT, TIMED_EVENTS, type TimedAutoStop } from "./lib/timedRecording";
import type { StartPlan } from "./lib/recordPlan";

function App() {
  const [activeMode, setActiveMode] = useState<AppMode>("record");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [meetingListRefreshKey, setMeetingListRefreshKey] = useState(0);

  const recording = useRecording();
  const transcripts = useTranscripts(recording.meetingId);
  const [setupRequired, setSetupRequired] = useSetupRequired();
  // The Record sheet (Record button clicks) and the one-time class notice
  const [recordPickerOpen, setRecordPickerOpen] = useState(false);
  const [showClassNotice, setShowClassNotice] = useState(false);
  const closeClassNotice = useCallback(() => setShowClassNotice(false), []);
  // A recording started some other way (tray, ⌘N) while the sheet was open
  useEffect(() => {
    if (recording.isRecording) setRecordPickerOpen(false);
  }, [recording.isRecording]);

  // Latest hook values for the (register-once) native event handlers below
  const recordingRef = useRef(recording);
  recordingRef.current = recording;
  const transcriptsRef = useRef(transcripts);
  transcriptsRef.current = transcripts;

  // Menu event listeners, registered once. Re-registering on every render
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
        // No plan: the remembered type and length (shortcut, tray)
        startRecording: (plan?: StartPlan) => recordingRef.current.startRecording(plan),
        stopRecording: () => recordingRef.current.stopRecording(),
        pauseRecording: () => recordingRef.current.pauseRecording(),
        resumeRecording: () => recordingRef.current.resumeRecording(),
      };
      const transcripts = { clearLiveTranscripts: () => transcriptsRef.current.clearLiveTranscripts() };
      // Menu bar (menu_builder.rs) and tray (tray_builder.rs): the same names as the window
      add(await listen("menu:settings", () => openSettings("recording")));
      add(await listen("menu:view_settings", () => openSettings("recording")));
      add(await listen("menu:view_record", () => setActiveMode("record")));
      add(await listen("menu:view_recordings", () => setActiveMode("recordings")));
      add(await listen("menu:view_chat", () => setActiveMode("chat")));
      add(await listen("menu:search", () => { setActiveMode("recordings"); requestSearchFocus(); }));
      add(await listen("menu:help", () => openHelp()));
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
      add(await listen("menu:pause_recording", async () => {
        if (recording.isRecording && !recording.isPaused) {
          await recording.pauseRecording();
        }
      }));
      // Tray: Start Recording uses the remembered type and length
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
      // Meeting-end detection: the countdown ran out. Stop through the
      // same path as the user's Stop (video, accessibility, notes)
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
      // First Class-type recording ever (any start path; recording_kind.rs):
      // a one-time reminder about school policy
      add(await listen(CLASS_NOTICE_EVENT, () => setShowClassNotice(true)));
    };

    setupListeners();
    return () => {
      disposed = true;
      listeners.forEach((unlisten) => unlisten());
    };
  }, []);

  // ⌘K: the one search field, at the top of Recordings
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setActiveMode("recordings");
        requestSearchFocus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Hooks must be unconditional
  const [isBackendReady, setIsBackendReady] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [isLongLoading, setIsLongLoading] = useState(false);

  // Listen for startup events and poll as fallback
  useEffect(() => {
    // Browser preview (no Tauri IPC): mock mode, skip the backend readiness gate
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
      if (isBackendReady || initError) {
        if (pollInterval) clearInterval(pollInterval);
        return;
      }
      pollBackend();
    }, 500);

    const timeout = setTimeout(() => setIsLongLoading(true), 8000);

    return () => {
      if (unlistenReady) unlistenReady();
      if (unlistenError) unlistenError();
      if (pollInterval) clearInterval(pollInterval);
      clearTimeout(timeout);
    };
  }, [isBackendReady, initError]);

  const stopRecording = async () => {
    try {
      await recording.stopRecording();
      setMeetingListRefreshKey((k) => k + 1);
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
      <div className="app-loading" style={{ flexDirection: 'column', gap: '16px' }}>
        {initError ? (
          <>
            <h2 style={{ fontSize: '20px', fontWeight: 600 }}>noFriction couldn't start</h2>
            <p style={{ color: '#ef4444', maxWidth: '400px', textAlign: 'center', background: 'rgba(0,0,0,0.2)', padding: '12px', borderRadius: '8px' }}>
              {initError}
            </p>
            <button onClick={() => window.location.reload()} className="btn-secondary" style={{ marginTop: '16px' }}>Try again</button>
          </>
        ) : (
          <>
            <div className="loading-spinner" />
            <div>
              <p style={{ fontSize: 14, fontWeight: 500 }}>Starting noFriction…</p>
              {isLongLoading && (
                <p style={{ color: 'var(--text-secondary)', fontSize: 12, marginTop: '8px' }}>
                  Taking longer than expected.
                </p>
              )}
            </div>
          </>
        )}
      </div>
    );
  }

  // First run. Shown once the backend is ready: the steps call commands
  // that need the database and the models folder.
  if (setupRequired) {
    return <SetupWizard onComplete={() => setSetupRequired(false)} />;
  }

  const handleMeetingSelect = (meetingId: string) => {
    debugLog(`Meeting selected: ${meetingId}`);
    setSelectedMeetingId(meetingId);
    transcripts.loadTranscripts(meetingId);
    setActiveMode("recordings");
  };

  return (
    <RecordPickerContext.Provider value={{ open: openRecordPicker }}>
    <div className="app-container ready">
      <Shell
        activeMode={activeMode}
        onModeChange={setActiveMode}
        recording={recording}
        transcripts={transcripts}
        onSelectMeeting={handleMeetingSelect}
        selectedMeetingId={selectedMeetingId}
        onStop={stopRecording}
        refreshKey={meetingListRefreshKey}
      />

      {/* "Send recording content to your endpoint?" (App Review 5.1.2(i)) */}
      <AiConsentModal />
      <PaywallModal />

      {recordPickerOpen && !recording.isRecording && (
        <RecordPicker onCancel={() => setRecordPickerOpen(false)} onStart={startFromPicker} />
      )}
      {showClassNotice && <ClassRecordingNotice onClose={closeClassNotice} />}
    </div>
    </RecordPickerContext.Provider>
  );
}

export default App;

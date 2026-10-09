// The window: top bar, one banner slot under it, one of three views
// (Record · Recordings · Chat), and the Settings and Help windows.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { TopBar } from "./TopBar";
import { RecordView } from "./views/RecordView";
import { RecordingsView } from "./views/RecordingsView";
import { RecordingsChat } from "./chat/RecordingsChat";
import { SettingsWindow } from "./SettingsWindow";
import { HelpWindow } from "./HelpWindow";
import { MeetingEndBanner } from "./MeetingEndBanner";
import { TimeLimitBanner } from "./TimedRecording";
import { useRecording } from "../hooks/useRecording";
import { useTranscripts } from "../hooks/useTranscripts";
import { onOpenHelp, onOpenSettings, type SettingsCategory } from "../lib/navigation";
import { getMeeting } from "../lib/tauri";
import { segmentCarryOver } from "../lib/recordPlan";
import { getTimedRecordingStatus } from "../lib/timedRecording";
import "./Shell.css";

export type AppMode = "record" | "recordings" | "chat";

interface ShellProps {
    activeMode: AppMode;
    onModeChange: (mode: AppMode) => void;
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
    onSelectMeeting: (id: string) => void;
    selectedMeetingId: string | null;
    onStop: () => Promise<void>;
    refreshKey: number;
}

export function Shell({
    activeMode,
    onModeChange,
    recording,
    transcripts,
    onSelectMeeting,
    selectedMeetingId,
    onStop,
    refreshKey,
}: ShellProps) {
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>("recording");
    const [helpOpen, setHelpOpen] = useState(false);

    // "Set up AI" notices, ⌘, and the menu open Settings here; Help → noFriction Help opens Help
    useEffect(
        () =>
            onOpenSettings((category) => {
                setSettingsCategory(category);
                setSettingsOpen(true);
            }),
        [],
    );
    useEffect(() => onOpenHelp(() => setHelpOpen(true)), []);

    // Long recordings: at 75 minutes, offer a new segment (native dialog)
    useEffect(() => {
        const unlisten = listen("recording_segment_prompt", async () => {
            if (!recording.isRecording) return;
            const shouldSegment = await ask(
                "This recording has reached 75 minutes. Long recordings can lose quality.\n\nStart a new segment? It keeps the type, the notebook and the time left.",
                { title: "Long recording", okLabel: "Start new segment", cancelLabel: "Keep recording" },
            );
            if (shouldSegment) handleSegmentConfirm();
        });
        return () => {
            unlisten.then((fn) => fn());
        };
    }, [recording.isRecording]); // eslint-disable-line react-hooks/exhaustive-deps

    const handleSegmentConfirm = async () => {
        // The new segment keeps the type, the notebook and the time that was
        // left (no "how long?" sheet: the user already chose)
        let carry = segmentCarryOver(null, null, Date.now());
        try {
            const [status, meeting] = await Promise.all([
                getTimedRecordingStatus(),
                recording.meetingId ? getMeeting(recording.meetingId) : Promise.resolve(null),
            ]);
            carry = segmentCarryOver(status, meeting, Date.now());
        } catch (e) {
            console.warn("Segment carry-over unavailable:", e);
        }
        await onStop();
        setTimeout(() => {
            transcripts.clearLiveTranscripts();
            recording.startRecording(carry).catch((e) => console.error("New segment failed to start:", e));
        }, 1500);
    };

    return (
        <div className="shell">
            <TopBar
                activeMode={activeMode}
                onModeChange={onModeChange}
                isRecording={recording.isRecording}
                onStop={() => void onStop()}
                onOpenSettings={() => {
                    setSettingsCategory("recording");
                    setSettingsOpen(true);
                }}
            />

            {/* One banner slot under the top bar: "This seems to have ended" and
                "5 minutes left" render here, never over the bar */}
            <div className="shell__banners">
                <MeetingEndBanner isRecording={recording.isRecording} onStopNow={onStop} />
                <TimeLimitBanner isRecording={recording.isRecording} />
            </div>

            <main className="shell__content">
                {activeMode === "record" && <RecordView recording={recording} transcripts={transcripts} />}
                {activeMode === "recordings" && (
                    <RecordingsView
                        onSelectMeeting={onSelectMeeting}
                        selectedMeetingId={selectedMeetingId}
                        refreshKey={refreshKey}
                        isRecording={recording.isRecording}
                        recordingMeetingId={recording.meetingId}
                    />
                )}
                {activeMode === "chat" && <RecordingsChat selectedMeetingId={selectedMeetingId} onOpenRecording={onSelectMeeting} />}
            </main>

            <SettingsWindow isOpen={settingsOpen} onClose={() => setSettingsOpen(false)} initialCategory={settingsCategory} />
            <HelpWindow isOpen={helpOpen} onClose={() => setHelpOpen(false)} />
        </div>
    );
}

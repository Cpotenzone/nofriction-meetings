// Record: idle, a title, the promise and one Record button. While
// recording, the capture bar (status, time left, what is captured, Mark,
// Capture screen), the live transcript, and the Screens captured so far.
// No AI runs here.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { LiveTranscriptView } from "../LiveTranscript";
import { CaptureBar, Screens } from "../CaptureBar";
import { useRecording } from "../../hooks/useRecording";
import { useTranscripts } from "../../hooks/useTranscripts";
import { useRecordPicker } from "../RecordPicker";
import { PrivacyPromiseBadge } from "../PrivacyPromise";

interface RecordViewProps {
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
}

interface TranscriptionStatus {
    connected: boolean;
    provider: string;
    error: string | null;
}

export function RecordView({ recording, transcripts }: RecordViewProps) {
    const recordPicker = useRecordPicker();
    const [sttStatus, setSttStatus] = useState<TranscriptionStatus | null>(null);

    // Transcription health: a failure must never be silent (screenshots but no
    // transcript, with no explanation)
    useEffect(() => {
        let unlisten: (() => void) | null = null;
        let disposed = false;
        listen<TranscriptionStatus>("transcription_status", (e) => setSttStatus(e.payload)).then((fn) => {
            if (disposed) fn();
            else unlisten = fn;
        });
        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    if (!recording.isRecording) {
        return (
            <div className="record-view is-idle">
                <LiveTranscriptView isRecording={false} transcripts={[]} onStartRecording={recordPicker.open} />
            </div>
        );
    }

    return (
        <div className="record-view is-live">
            <CaptureBar isRecording sttStatus={sttStatus} audioWarning={recording.audioWarning} />
            <div className="record-view__body">
                <div className="record-view__transcript">
                    <LiveTranscriptView isRecording transcripts={transcripts.liveTranscripts} />
                    {transcripts.liveTranscripts.length > 0 && <PrivacyPromiseBadge />}
                </div>
                <aside className="record-view__screens">
                    <Screens isRecording />
                </aside>
            </div>
        </div>
    );
}

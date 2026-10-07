// Meeting-end countdown: "This seems to have ended" / "<reason> — stopping in 30s"
// with [Keep recording] (snooze detection) and [Stop now]. The backend
// (meeting_end.rs) decides; when the countdown runs out it emits
// `meeting-end-auto-stop`, which App.tsx handles through the normal Stop path.
import React, { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import * as tauri from '../lib/tauri';
import type { MeetingEndDetected } from '../lib/tauri';

interface MeetingEndBannerProps {
    isRecording: boolean;
    /** The user's own Stop action (video, accessibility unlink, notes…) */
    onStopNow: () => Promise<void> | void;
}

export const MeetingEndBanner: React.FC<MeetingEndBannerProps> = ({ isRecording, onStopNow }) => {
    const [pending, setPending] = useState<{ reason: string; deadline: number } | null>(null);
    const [now, setNow] = useState(() => Date.now());

    useEffect(() => {
        const show = (p: MeetingEndDetected) =>
            setPending({ reason: p.reason, deadline: Date.now() + p.countdown * 1000 });
        const offs = [
            listen<MeetingEndDetected>('meeting-end-detected', (e) => show(e.payload)),
            listen('meeting-end-cancelled', () => setPending(null)),
            listen('meeting-end-auto-stop', () => setPending(null)),
        ];
        // Re-sync if the countdown started before this view mounted
        tauri.getMeetingEndStatus()
            .then((s) => { if (s.pending) show(s.pending); })
            .catch(() => { /* backend not ready */ });
        return () => { offs.forEach((p) => p.then((off) => off())); };
    }, []);

    useEffect(() => {
        if (!isRecording) setPending(null);
    }, [isRecording]);

    useEffect(() => {
        if (!pending) return;
        const id = window.setInterval(() => setNow(Date.now()), 250);
        return () => window.clearInterval(id);
    }, [pending]);

    if (!pending || !isRecording) return null;
    const secondsLeft = Math.max(0, Math.ceil((pending.deadline - now) / 1000));

    return (
        <div style={{ position: 'fixed', bottom: 20, left: '50%', transform: 'translateX(-50%)', zIndex: 120 }}>
            <div className="meeting-detection-banner sliding-in" role="alertdialog" aria-live="assertive">
                <div className="mdb-icon">⏹</div>
                <div className="mdb-content">
                    <div className="mdb-title">This seems to have ended</div>
                    <div className="mdb-subtitle">
                        {pending.reason} — stopping in {secondsLeft}s
                    </div>
                </div>
                <div className="mdb-actions">
                    <button
                        className="mdb-btn mdb-btn-secondary"
                        onClick={async () => {
                            setPending(null);
                            try { await tauri.meetingEndKeepRecording(); } catch (e) { console.error('Keep recording failed:', e); }
                        }}
                        aria-label="Keep recording"
                    >
                        Keep recording
                    </button>
                    <button
                        className="mdb-btn mdb-btn-primary"
                        onClick={async () => {
                            setPending(null);
                            await onStopNow();
                        }}
                        aria-label="Stop recording now"
                    >
                        Stop now
                    </button>
                </div>
            </div>
        </div>
    );
};

export default MeetingEndBanner;

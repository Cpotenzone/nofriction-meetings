// Timed recording while it runs: the capture bar's clock ("12:34 left", or
// elapsed time without a limit) with +15 min / No limit, the warning banner
// 5 minutes before the end (2 for a 15-minute recording), and the one-time
// class-recording notice (first Class-type recording). The backend owns the
// timer (timed_recording.rs).

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { secondsUntil, timerLabel } from "../lib/recordPlan";
import {
    CLASS_RECORDING_NOTICE,
    TIMED_EVENTS,
    extendTimedRecording,
    getTimedRecordingStatus,
    removeTimedRecordingLimit,
    type TimedStatus,
} from "../lib/timedRecording";
import { ClockIcon } from "./icons";
import "./TimedRecording.css";

/** The running plan, kept current from `timed-recording-changed`. */
export function useTimedStatus(isRecording: boolean): TimedStatus | null {
    const [status, setStatus] = useState<TimedStatus | null>(null);
    useEffect(() => {
        let disposed = false;
        let off: (() => void) | null = null;
        listen<TimedStatus | null>(TIMED_EVENTS.changed, (e) => setStatus(e.payload ?? null)).then((fn) =>
            disposed ? fn() : (off = fn)
        );
        return () => {
            disposed = true;
            off?.();
        };
    }, []);
    useEffect(() => {
        if (!isRecording) {
            setStatus(null);
            return;
        }
        getTimedRecordingStatus().then(setStatus).catch(() => {});
    }, [isRecording]);
    return status;
}

function useNow(active: boolean, everyMs = 1000): number {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        if (!active) return;
        setNow(Date.now());
        const id = window.setInterval(() => setNow(Date.now()), everyMs);
        return () => window.clearInterval(id);
    }, [active, everyMs]);
    return now;
}

function stopsAt(deadline: string): string {
    return new Date(deadline).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function useLimitActions(status: TimedStatus | null) {
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const run = useCallback(
        async (fn: (id: string | null) => Promise<TimedStatus>) => {
            setBusy(true);
            setError(null);
            try {
                await fn(status?.meetingId ?? null);
            } catch (e) {
                setError(String(e));
            } finally {
                setBusy(false);
            }
        },
        [status?.meetingId]
    );
    return {
        busy,
        error,
        extend: () => run(extendTimedRecording),
        removeLimit: () => run(removeTimedRecordingLimit),
    };
}

/** Capture bar: remaining (or elapsed) time and the two controls. */
export function RecordingTimer({ isRecording }: { isRecording: boolean }) {
    const status = useTimedStatus(isRecording);
    const now = useNow(isRecording && !!status);
    const { busy, error, extend, removeLimit } = useLimitActions(status);
    if (!isRecording || !status) return null;
    const label = timerLabel({ deadline: status.deadline, startedAt: status.startedAt, nowMs: now });
    const low = !!status.deadline && secondsUntil(status.deadline, now) <= 5 * 60;
    return (
        <div className={`tlim ${low ? "is-low" : ""}`} role="group" aria-label="Recording time">
            <span
                className="tlim__clock"
                title={status.deadline ? `Stops at ${stopsAt(status.deadline)}` : "No time limit"}
                aria-label={status.deadline ? `${label}, stops at ${stopsAt(status.deadline)}` : `Recording for ${label}, no time limit`}
            >
                {label}
            </span>
            {status.deadline && (
                <>
                    <button type="button" className="tlim__btn" onClick={extend} disabled={busy} title="Add 15 minutes">
                        +15 min
                    </button>
                    <button type="button" className="tlim__btn" onClick={removeLimit} disabled={busy} title="Keep recording until you stop">
                        No limit
                    </button>
                </>
            )}
            {error && <span className="tlim__error" role="alert">{error}</span>}
        </div>
    );
}

/** "5 minutes left — stops at 10:45" with +15 min / No limit. */
export function TimeLimitBanner({ isRecording }: { isRecording: boolean }) {
    const status = useTimedStatus(isRecording);
    const [warning, setWarning] = useState<TimedStatus | null>(null);
    const now = useNow(!!warning);
    const { busy, error, extend, removeLimit } = useLimitActions(status ?? warning);

    useEffect(() => {
        let disposed = false;
        let off: (() => void) | null = null;
        listen<TimedStatus>(TIMED_EVENTS.warning, (e) => setWarning(e.payload)).then((fn) =>
            disposed ? fn() : (off = fn)
        );
        return () => {
            disposed = true;
            off?.();
        };
    }, []);

    // Extended, limit removed, or stopped: the warning no longer applies
    useEffect(() => {
        if (!isRecording || !status || !status.deadline || !status.warned) setWarning(null);
    }, [isRecording, status]);

    if (!warning || !isRecording) return null;
    const deadline = status?.deadline ?? warning.deadline;
    if (!deadline) return null;
    const minutes = Math.max(1, Math.ceil(secondsUntil(deadline, now) / 60));

    return (
        <div className="tlim-banner" role="alertdialog" aria-live="assertive" aria-labelledby="tlim-banner-title">
            <div className="meeting-detection-banner sliding-in">
                <div className="mdb-icon" aria-hidden><ClockIcon size={26} strokeWidth={1.75} /></div>
                <div className="mdb-content">
                    <div className="mdb-title" id="tlim-banner-title">
                        {minutes === 1 ? "1 minute left" : `${minutes} minutes left`}
                    </div>
                    <div className="mdb-subtitle">
                        Recording stops at {stopsAt(deadline)}
                        {error ? ` — ${error}` : ""}
                    </div>
                </div>
                <div className="mdb-actions">
                    <button className="mdb-btn mdb-btn-secondary" onClick={() => setWarning(null)} aria-label="Dismiss">
                        OK
                    </button>
                    <button className="mdb-btn mdb-btn-secondary" onClick={removeLimit} disabled={busy}>
                        No limit
                    </button>
                    <button className="mdb-btn mdb-btn-primary" onClick={extend} disabled={busy}>
                        +15 min
                    </button>
                </div>
            </div>
        </div>
    );
}

/** One-time, non-blocking: shown when the first Class-type recording starts. */
export function ClassRecordingNotice({ onClose }: { onClose: () => void }) {
    useEffect(() => {
        const id = window.setTimeout(onClose, 30_000);
        return () => window.clearTimeout(id);
    }, [onClose]);
    return (
        <div className="class-notice" role="status" aria-live="polite">
            <p>{CLASS_RECORDING_NOTICE}</p>
            <button type="button" className="class-notice__ok" onClick={onClose}>
                Got it
            </button>
        </div>
    );
}

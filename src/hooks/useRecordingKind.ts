// A recording's type (Meeting · Class · Personal), for views that label
// things by it: marks, the Notes layout hint, the Review guide's name.
// Cached per recording; a change made in the app is broadcast so every
// open view relabels at once.

import { useEffect, useState } from "react";
import * as tauri from "../lib/tauri";
import { DEFAULT_KIND, parseKind, type RecordingKind } from "../lib/recordingKind";

const cache = new Map<string, RecordingKind>();
const CHANGED_EVENT = "nf:recording-kind-changed";

/** Call after a recording's type was changed (or learnt), so open views update. */
export function notifyRecordingKind(meetingId: string, kind: RecordingKind): void {
    cache.set(meetingId, kind);
    window.dispatchEvent(new CustomEvent(CHANGED_EVENT, { detail: { meetingId, kind } }));
}

/** The type of `meetingId` (Meeting while unknown or loading). */
export function useRecordingKind(meetingId: string | null | undefined): RecordingKind {
    const [kind, setKind] = useState<RecordingKind>(() => (meetingId && cache.get(meetingId)) || DEFAULT_KIND);

    useEffect(() => {
        if (!meetingId) {
            setKind(DEFAULT_KIND);
            return;
        }
        let live = true;
        setKind(cache.get(meetingId) ?? DEFAULT_KIND);
        tauri
            .getMeeting(meetingId)
            .then((m) => {
                if (!live || !m) return;
                const k = parseKind(m.recording_kind);
                cache.set(meetingId, k);
                setKind(k);
            })
            .catch(() => {});
        const on = (e: Event) => {
            const d = (e as CustomEvent<{ meetingId: string; kind: RecordingKind }>).detail;
            if (d?.meetingId === meetingId) setKind(d.kind);
        };
        window.addEventListener(CHANGED_EVENT, on);
        return () => {
            live = false;
            window.removeEventListener(CHANGED_EVENT, on);
        };
    }, [meetingId]);

    return kind;
}

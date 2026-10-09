// Capture bar: "Mark" the moment while recording. One click marks ★
// Important; a small card then offers the other types (the third is
// labelled by the recording's type: On the test / Follow up / Remember) and
// a note for a few seconds. ⌃⌥⌘M (global) and File → Mark Moment do the
// same from anywhere; their marks show the same card here.
// docs/STUDY_TOOLS.md

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { MarkIcon } from "../icons";
import { KindPicker, MarkKindContext, MarkerGlyph } from "./MarkerBits";
import { MARKER_ADDED_EVENT, MARKER_FAILED_EVENT, markersApi, notifyMarkersChanged } from "../../lib/study";
import { clock, markerLabel, type Marker, type MarkerKind } from "../../lib/studyLogic";
import { useRecordingKind } from "../../hooks/useRecordingKind";

/** How long the type/note card stays without interaction */
const CARD_MS = 7000;

export function MarkButton({ isRecording }: { isRecording: boolean }) {
    const [marker, setMarker] = useState<Marker | null>(null);
    const [note, setNote] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const timer = useRef<number | null>(null);
    const holding = useRef(false);
    const shownId = useRef<string | null>(null);
    const recKind = useRecordingKind(marker?.meeting_id ?? null);

    const clearTimer = () => {
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = null;
    };
    const close = useCallback(() => {
        clearTimer();
        holding.current = false;
        shownId.current = null;
        setMarker(null);
        setNote("");
    }, []);
    const armTimer = useCallback(() => {
        clearTimer();
        timer.current = window.setTimeout(() => {
            if (!holding.current) close();
        }, CARD_MS);
    }, [close]);

    const show = useCallback(
        (m: Marker) => {
            setError(null);
            // A new mark starts with its own note; the same one keeps the typing
            if (shownId.current !== m.id) setNote(m.note ?? "");
            shownId.current = m.id;
            setMarker(m);
            notifyMarkersChanged(m.meeting_id);
            armTimer();
        },
        [armTimer],
    );

    // Marks from the hotkey / menu (and our own button) arrive as events
    useEffect(() => {
        const offs: (() => void)[] = [];
        let disposed = false;
        const add = (off: () => void) => (disposed ? off() : offs.push(off));
        listen<Marker>(MARKER_ADDED_EVENT, (e) => show(e.payload)).then(add);
        listen<string>(MARKER_FAILED_EVENT, (e) => {
            setError(e.payload);
            window.setTimeout(() => setError(null), 3000);
        }).then(add);
        return () => {
            disposed = true;
            offs.forEach((o) => o());
            clearTimer();
        };
    }, [show]);

    useEffect(() => {
        if (!isRecording) close();
    }, [isRecording, close]);

    const mark = async () => {
        if (busy) return;
        setBusy(true);
        try {
            show(await markersApi.markNow());
        } catch (e) {
            setError(String(e));
            window.setTimeout(() => setError(null), 3000);
        } finally {
            setBusy(false);
        }
    };

    const setKind = async (k: MarkerKind) => {
        if (!marker) return;
        armTimer();
        try {
            show(await markersApi.setKind(marker.id, k));
        } catch (e) {
            setError(String(e));
        }
    };

    const saveNote = async () => {
        if (!marker || (note.trim() === (marker.note ?? ""))) return;
        try {
            const m = await markersApi.setNote(marker.id, note);
            setMarker(m);
            notifyMarkersChanged(m.meeting_id);
        } catch (e) {
            setError(String(e));
        }
    };

    return (
        <div className="study-mark">
            <button
                className="cbar__snap study-mark__btn"
                onClick={mark}
                type="button"
                disabled={!isRecording || busy}
                title={isRecording ? "Mark this moment ★ (⌃⌥⌘M, also when another app is in front)" : "Start a recording to mark moments"}
                aria-label="Mark this moment"
            >
                <MarkIcon size={15} />
                <span>Mark</span>
            </button>

            {marker && (
                <MarkKindContext.Provider value={recKind}>
                <div
                    className="study-mark__card"
                    role="dialog"
                    aria-label="Marked moment"
                    onMouseEnter={() => (holding.current = true)}
                    onMouseLeave={() => {
                        holding.current = false;
                        armTimer();
                    }}
                >
                    <div className="study-mark__head">
                        <MarkerGlyph kind={marker.kind} />
                        <span>
                            Marked {markerLabel(marker.kind, recKind)} at {clock(marker.offset_ms)}
                        </span>
                        <button className="rd-btn rd-btn-ghost study-mark__close" type="button" onClick={close} aria-label="Close">
                            Done
                        </button>
                    </div>
                    <KindPicker value={marker.kind} onChange={setKind} />
                    <input
                        className="study-input"
                        value={note}
                        maxLength={280}
                        placeholder="Add a note (optional)"
                        aria-label="Note for this mark"
                        onFocus={() => {
                            holding.current = true;
                            clearTimer();
                        }}
                        onChange={(e) => setNote(e.target.value)}
                        onBlur={() => {
                            holding.current = false;
                            void saveNote();
                            armTimer();
                        }}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                void saveNote().then(close);
                            } else if (e.key === "Escape") {
                                close();
                            }
                        }}
                    />
                </div>
                </MarkKindContext.Provider>
            )}
            {error && !marker && (
                <div className="study-mark__card is-error" role="status">
                    {error}
                </div>
            )}
        </div>
    );
}

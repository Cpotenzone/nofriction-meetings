// Recordings view: the meeting's moment markers. A list that jumps to each
// moment, filtered by type ("everything marked for the test"); markers on
// the scrubber; and a marker chip inline in the transcript at its time.
// docs/STUDY_TOOLS.md

import { useCallback, useEffect, useState } from "react";
import { TrashIcon } from "../icons";
import { KindPicker, MarkerGlyph } from "./MarkerBits";
import { MARKERS_CHANGED_EVENT, markersApi, notifyMarkersChanged } from "../../lib/study";
import {
    MARKER_KINDS,
    MARKER_META,
    clock,
    countByKind,
    filterMarkers,
    type Marker,
    type MarkerFilter,
    type MarkerKind,
} from "../../lib/studyLogic";

/** A meeting's markers, kept fresh when any view changes them. */
export function useMarkers(meetingId: string | null, reloadKey = 0) {
    const [markers, setMarkers] = useState<Marker[]>([]);
    const load = useCallback(() => {
        if (!meetingId) {
            setMarkers([]);
            return;
        }
        markersApi
            .list(meetingId)
            .then(setMarkers)
            .catch(() => setMarkers([]));
    }, [meetingId]);
    useEffect(load, [load, reloadKey]);
    useEffect(() => {
        const on = (e: Event) => {
            const id = (e as CustomEvent<{ meetingId: string }>).detail?.meetingId;
            if (!id || id === meetingId) load();
        };
        window.addEventListener(MARKERS_CHANGED_EVENT, on);
        return () => window.removeEventListener(MARKERS_CHANGED_EVENT, on);
    }, [meetingId, load]);
    return { markers, setMarkers, reload: load };
}

interface ListProps {
    meetingId: string;
    markers: Marker[];
    /** The scrubber's time, for "Mark here" */
    currentMs: number;
    onJump: (ms: number) => void;
    onChanged: () => void;
}

export function MarkerList({ meetingId, markers, currentMs, onJump, onChanged }: ListProps) {
    const [filter, setFilter] = useState<MarkerFilter>("all");
    const [open, setOpen] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const counts = countByKind(markers);
    const shown = filterMarkers(markers, filter);

    const run = async (fn: () => Promise<unknown>) => {
        setError(null);
        try {
            await fn();
            notifyMarkersChanged(meetingId);
            onChanged();
        } catch (e) {
            setError(String(e));
        }
    };

    return (
        <section className="study-markers" aria-label="Moment markers">
            <div className="study-markers__bar">
                <button
                    type="button"
                    className="rd-btn rd-btn-ghost study-markers__toggle"
                    aria-expanded={open}
                    onClick={() => setOpen((o) => !o)}
                >
                    Markers ({markers.length})
                </button>
                <div className="study-filter" role="group" aria-label="Show markers of type">
                    <button type="button" className={`study-chip${filter === "all" ? " is-on" : ""}`} aria-pressed={filter === "all"} onClick={() => setFilter("all")}>
                        All
                    </button>
                    {MARKER_KINDS.map((k) => (
                        <button
                            key={k}
                            type="button"
                            className={`study-chip is-${k}${filter === k ? " is-on" : ""}`}
                            aria-pressed={filter === k}
                            title={`Only ${MARKER_META[k].label}`}
                            onClick={() => {
                                setFilter(k);
                                setOpen(true);
                            }}
                        >
                            <MarkerGlyph kind={k} size={12} /> {counts[k]}
                        </button>
                    ))}
                </div>
                <button
                    type="button"
                    className="rd-btn rd-btn-ghost"
                    style={{ padding: "0 6px", marginLeft: "auto" }}
                    title={`Mark ★ at ${clock(currentMs)} (where the scrubber is)`}
                    onClick={() => run(() => markersApi.add(meetingId, currentMs))}
                >
                    Mark {clock(currentMs)}
                </button>
            </div>
            {error && (
                <p className="rd-tip" role="alert">
                    {error}
                </p>
            )}
            {open && shown.length > 0 && (
                <ul className="study-markers__list">
                    {shown.map((m) => (
                        <MarkerRow key={m.id} m={m} onJump={onJump} run={run} />
                    ))}
                </ul>
            )}
            {open && markers.length > 0 && shown.length === 0 && (
                <p className="study-muted">No {MARKER_META[filter as MarkerKind]?.label ?? ""} markers.</p>
            )}
        </section>
    );
}

function MarkerRow({ m, onJump, run }: { m: Marker; onJump: (ms: number) => void; run: (fn: () => Promise<unknown>) => void }) {
    const [editing, setEditing] = useState(false);
    const [note, setNote] = useState(m.note ?? "");
    useEffect(() => setNote(m.note ?? ""), [m.note]);
    const save = () => {
        setEditing(false);
        if (note.trim() !== (m.note ?? "")) run(() => markersApi.setNote(m.id, note));
    };
    return (
        <li className={`study-markers__row is-${m.kind}`}>
            <button type="button" className="study-time" onClick={() => onJump(m.offset_ms)} title="Jump to this moment">
                <MarkerGlyph kind={m.kind} /> {clock(m.offset_ms)}
            </button>
            {editing ? (
                <input
                    className="study-input"
                    autoFocus
                    value={note}
                    maxLength={280}
                    placeholder="Note"
                    aria-label="Marker note"
                    onChange={(e) => setNote(e.target.value)}
                    onBlur={save}
                    onKeyDown={(e) => {
                        if (e.key === "Enter") save();
                        if (e.key === "Escape") {
                            setNote(m.note ?? "");
                            setEditing(false);
                        }
                    }}
                />
            ) : (
                <button type="button" className="study-note" onClick={() => setEditing(true)} title="Edit the note">
                    {m.note ? m.note : <span className="study-muted">{MARKER_META[m.kind].label} · add a note</span>}
                </button>
            )}
            <KindPicker compact value={m.kind} onChange={(k) => run(() => markersApi.setKind(m.id, k))} />
            <button
                type="button"
                className="rd-btn rd-btn-ghost study-icon-btn"
                aria-label={`Delete the marker at ${clock(m.offset_ms)}`}
                title="Delete marker"
                onClick={() => run(() => markersApi.remove(m.id))}
            >
                <TrashIcon size={13} />
            </button>
        </li>
    );
}

/** A marker inside the transcript, at its time. */
export function MarkerInline({ m, onJump }: { m: Marker; onJump: (ms: number) => void }) {
    return (
        <button
            type="button"
            className={`study-inline is-${m.kind}`}
            onClick={(e) => {
                e.stopPropagation();
                onJump(m.offset_ms);
            }}
            title={`${MARKER_META[m.kind].label} at ${clock(m.offset_ms)}`}
        >
            <MarkerGlyph kind={m.kind} size={12} />
            <span className="study-inline__label">{MARKER_META[m.kind].label}</span>
            <span className="study-inline__time">{clock(m.offset_ms)}</span>
            {m.note && <span className="study-inline__note">{m.note}</span>}
        </button>
    );
}

/** Pins above the scrubber; click one to jump there. */
export function MarkerPins({ markers, pct, onJump }: { markers: Marker[]; pct: (ms: number) => number; onJump: (ms: number) => void }) {
    if (markers.length === 0) return null;
    return (
        <div className="study-pins" aria-label="Markers on the timeline">
            {markers.map((m) => (
                <button
                    key={m.id}
                    type="button"
                    className={`study-pin is-${m.kind}`}
                    style={{ left: `${pct(m.offset_ms)}%` }}
                    title={`${MARKER_META[m.kind].symbol} ${MARKER_META[m.kind].label} · ${clock(m.offset_ms)}${m.note ? ` · ${m.note}` : ""}`}
                    aria-label={`${MARKER_META[m.kind].label} at ${clock(m.offset_ms)}`}
                    onClick={() => onJump(m.offset_ms)}
                >
                    <MarkerGlyph kind={m.kind} size={10} />
                </button>
            ))}
        </div>
    );
}

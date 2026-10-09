// Rewind: the recording's marks. Pins on the timeline and a chip inline
// in the transcript at each mark's time, plus a list (collapsed by
// default) that jumps to each one, filtered by type. Labels follow the
// recording's type (MarkKindContext). docs/STUDY_TOOLS.md

import { useCallback, useEffect, useState } from "react";
import { TrashIcon } from "../icons";
import { KindPicker, MarkerGlyph, useMarkerMeta } from "./MarkerBits";
import { MARKERS_CHANGED_EVENT, markersApi, notifyMarkersChanged } from "../../lib/study";
import {
    MARKER_KINDS,
    clock,
    countByKind,
    filterMarkers,
    type Marker,
    type MarkerFilter,
    type MarkerKind,
} from "../../lib/studyLogic";

/** A recording's markers, kept fresh when any view changes them. */
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
    const [open, setOpen] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const counts = countByKind(markers);
    const shown = filterMarkers(markers, filter);
    const meta = useMarkerMeta();

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
        <section className="study-markers" aria-label="Marks">
            <div className="study-markers__bar">
                <button
                    type="button"
                    className="rd-btn rd-btn-ghost study-markers__toggle"
                    aria-expanded={open}
                    onClick={() => setOpen((o) => !o)}
                >
                    {open ? "▾" : "▸"} Marks ({markers.length})
                </button>
                {open && <div className="study-filter" role="group" aria-label="Show marks of type">
                    <button type="button" className={`study-chip${filter === "all" ? " is-on" : ""}`} aria-pressed={filter === "all"} onClick={() => setFilter("all")}>
                        All
                    </button>
                    {MARKER_KINDS.map((k) => (
                        <button
                            key={k}
                            type="button"
                            className={`study-chip is-${k}${filter === k ? " is-on" : ""}`}
                            aria-pressed={filter === k}
                            title={`Only ${meta(k).label}`}
                            aria-label={`Only ${meta(k).label} (${counts[k]})`}
                            onClick={() => {
                                setFilter(k);
                                setOpen(true);
                            }}
                        >
                            <MarkerGlyph kind={k} size={12} /> {counts[k]}
                        </button>
                    ))}
                </div>}
                {open && <button
                    type="button"
                    className="rd-btn rd-btn-ghost"
                    style={{ padding: "0 6px", marginLeft: "auto" }}
                    title={`Mark ★ at ${clock(currentMs)} (where the scrubber is)`}
                    onClick={() => run(() => markersApi.add(meetingId, currentMs))}
                >
                    Mark {clock(currentMs)}
                </button>}
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
                <p className="study-muted">No {filter === "all" ? "" : meta(filter as MarkerKind).label} marks.</p>
            )}
        </section>
    );
}

function MarkerRow({ m, onJump, run }: { m: Marker; onJump: (ms: number) => void; run: (fn: () => Promise<unknown>) => void }) {
    const [editing, setEditing] = useState(false);
    const [note, setNote] = useState(m.note ?? "");
    const meta = useMarkerMeta();
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
                    aria-label="Mark note"
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
                    {m.note ? m.note : <span className="study-muted">{meta(m.kind).label} · add a note</span>}
                </button>
            )}
            <KindPicker compact value={m.kind} onChange={(k) => run(() => markersApi.setKind(m.id, k))} />
            <button
                type="button"
                className="rd-btn rd-btn-ghost study-icon-btn"
                aria-label={`Delete the mark at ${clock(m.offset_ms)}`}
                title="Delete mark"
                onClick={() => run(() => markersApi.remove(m.id))}
            >
                <TrashIcon size={13} />
            </button>
        </li>
    );
}

/** A marker inside the transcript, at its time. */
export function MarkerInline({ m, onJump }: { m: Marker; onJump: (ms: number) => void }) {
    const label = useMarkerMeta()(m.kind).label;
    return (
        <button
            type="button"
            className={`study-inline is-${m.kind}`}
            onClick={(e) => {
                e.stopPropagation();
                onJump(m.offset_ms);
            }}
            title={`${label} at ${clock(m.offset_ms)}`}
        >
            <MarkerGlyph kind={m.kind} size={12} />
            <span className="study-inline__label">{label}</span>
            <span className="study-inline__time">{clock(m.offset_ms)}</span>
            {m.note && <span className="study-inline__note">{m.note}</span>}
        </button>
    );
}

/** Pins above the scrubber; click one to jump there. */
export function MarkerPins({ markers, pct, onJump }: { markers: Marker[]; pct: (ms: number) => number; onJump: (ms: number) => void }) {
    const meta = useMarkerMeta();
    if (markers.length === 0) return null;
    return (
        <div className="study-pins" aria-label="Marks on the timeline">
            {markers.map((m) => (
                <button
                    key={m.id}
                    type="button"
                    className={`study-pin is-${m.kind}`}
                    style={{ left: `${pct(m.offset_ms)}%` }}
                    title={`${meta(m.kind).symbol} ${meta(m.kind).label} · ${clock(m.offset_ms)}${m.note ? ` · ${m.note}` : ""}`}
                    aria-label={`${meta(m.kind).label} at ${clock(m.offset_ms)}`}
                    onClick={() => onJump(m.offset_ms)}
                >
                    <MarkerGlyph kind={m.kind} size={10} />
                </button>
            ))}
        </div>
    );
}

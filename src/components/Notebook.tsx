// Notebooks in the library: the "Notebooks: All · BIO 101 · …" filter
// chips, and the title block of an open recording, where the type and the
// notebook are two tappable rows (src-tauri/src/notebooks.rs,
// recording_kind.rs).

import { useEffect, useId, useRef, useState } from "react";
import * as tauri from "../lib/tauri";
import { canonicalNotebook } from "../lib/recordPlan";
import { NOTEBOOKS_LABEL, RECORDING_KINDS, notebookPlaceholder, parseKind, type RecordingKind } from "../lib/recordingKind";
import { listRecentNotebooks, setMeetingNotebook, setMeetingRecordingKind } from "../lib/timedRecording";
import { notifyRecordingKind } from "../hooks/useRecordingKind";
import { BookIcon, TagIcon } from "./icons";
import "./TimedRecording.css";

/** Recent notebooks, reloaded when `refreshKey` changes. */
export function useRecentNotebooks(refreshKey: unknown): string[] {
    const [notebooks, setNotebooks] = useState<string[]>([]);
    useEffect(() => {
        let live = true;
        listRecentNotebooks()
            .then((c) => live && setNotebooks(c))
            .catch(() => {});
        return () => { live = false; };
    }, [refreshKey]);
    return notebooks;
}

/** Filter chips titled "Notebooks"; hidden until at least one recording has a notebook. */
export function NotebookFilterChips({
    notebooks,
    value,
    onChange,
}: {
    notebooks: string[];
    value: string | null;
    onChange: (notebook: string | null) => void;
}) {
    const titleId = useId();
    if (notebooks.length === 0 && !value) return null;
    const shown = value && !notebooks.includes(value) ? [value, ...notebooks] : notebooks;
    return (
        <div className="class-filter" role="toolbar" aria-labelledby={titleId}>
            <span id={titleId} className="class-filter__title">{NOTEBOOKS_LABEL}</span>
            <button
                type="button"
                className={`class-chip ${value === null ? "is-on" : ""}`}
                aria-pressed={value === null}
                onClick={() => onChange(null)}
            >
                All
            </button>
            {shown.map((c) => (
                <button
                    key={c}
                    type="button"
                    className={`class-chip ${value === c ? "is-on" : ""}`}
                    aria-pressed={value === c}
                    title={c}
                    onClick={() => onChange(value === c ? null : c)}
                >
                    {c}
                </button>
            ))}
        </div>
    );
}

const formatWhen = (iso: string, seconds: number | null | undefined) => {
    const d = new Date(iso);
    const when = d.toLocaleString([], { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
    if (!seconds) return when;
    const mins = Math.round(seconds / 60);
    return `${when} · ${mins >= 60 ? `${Math.floor(mins / 60)}h ${mins % 60}m` : `${mins} min`}`;
};

/**
 * The open recording's title block: the title and date, then two tappable
 * rows with no labels: the type (a menu; new notes and the Review guide use
 * it) and the notebook (click to edit; saves on Enter or blur).
 */
export function RecordingTitleBlock({ meetingId, refreshKey = 0, onSaved }: { meetingId: string; refreshKey?: unknown; onSaved?: () => void }) {
    const [meeting, setMeeting] = useState<tauri.Meeting | null>(null);
    const [kind, setKind] = useState<RecordingKind>("meeting");
    const [saved, setSaved] = useState<string | null>(null);
    const [draft, setDraft] = useState("");
    const [editing, setEditing] = useState(false);
    const [status, setStatus] = useState<string | null>(null);
    const recents = useRecentNotebooks(meetingId);
    const listId = useId();
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        let live = true;
        setStatus(null);
        setEditing(false);
        tauri.getMeeting(meetingId)
            .then((m) => {
                if (!live) return;
                setMeeting(m);
                setSaved(m?.class_name ?? null);
                setDraft(m?.class_name ?? "");
                setKind(parseKind(m?.recording_kind));
            })
            .catch(() => {});
        return () => { live = false; };
    }, [meetingId, refreshKey]);

    useEffect(() => {
        if (editing) inputRef.current?.focus();
    }, [editing]);

    useEffect(() => {
        if (!status) return;
        const t = window.setTimeout(() => setStatus(null), 2000);
        return () => window.clearTimeout(t);
    }, [status]);

    const changeKind = async (next: RecordingKind) => {
        if (next === kind) return;
        const before = kind;
        setKind(next);
        try {
            const stored = parseKind(await setMeetingRecordingKind(meetingId, next));
            setKind(stored);
            notifyRecordingKind(meetingId, stored);
            setStatus("Saved. New notes use it.");
            onSaved?.();
        } catch {
            setKind(before);
            setStatus("Couldn't save");
        }
    };

    const commit = async () => {
        setEditing(false);
        const next = canonicalNotebook(draft, recents);
        if ((next ?? null) === (saved ?? null)) {
            setDraft(next ?? "");
            return;
        }
        try {
            const stored = await setMeetingNotebook(meetingId, next);
            setSaved(stored);
            setDraft(stored ?? "");
            setStatus("Saved");
            onSaved?.();
        } catch {
            setDraft(saved ?? "");
            setStatus("Couldn't save");
        }
    };

    return (
        <header className="rec-title">
            <h2 className="rec-title__name">{meeting?.title ?? ""}</h2>
            {meeting && <p className="rec-title__when">{formatWhen(meeting.started_at, meeting.duration_seconds)}</p>}
            <div className="rec-title__rows">
                <label className="rec-row" title="What this recording is. New notes and the Review guide follow it.">
                    <TagIcon size={14} />
                    <select
                        className="rec-row__select"
                        value={kind}
                        onChange={(e) => void changeKind(parseKind(e.target.value))}
                        aria-label="Recording type"
                    >
                        {RECORDING_KINDS.map((k) => (
                            <option key={k.value} value={k.value}>{k.label}</option>
                        ))}
                    </select>
                </label>
                {editing ? (
                    <span className="rec-row">
                        <BookIcon size={14} />
                        <input
                            ref={inputRef}
                            className="rec-row__input"
                            value={draft}
                            list={listId}
                            placeholder={`Notebook (${notebookPlaceholder(kind)})`}
                            maxLength={120}
                            aria-label="Notebook"
                            onChange={(e) => setDraft(e.target.value)}
                            onBlur={() => void commit()}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                                if (e.key === "Escape") {
                                    setDraft(saved ?? "");
                                    setEditing(false);
                                }
                            }}
                        />
                        <datalist id={listId}>
                            {recents.map((c) => <option key={c} value={c} />)}
                        </datalist>
                    </span>
                ) : (
                    <button type="button" className={`rec-row rec-row--btn ${saved ? "" : "is-empty"}`} onClick={() => setEditing(true)} title="Notebook: a group for related recordings">
                        <BookIcon size={14} />
                        <span>{saved ?? "Add to a notebook"}</span>
                    </button>
                )}
                {status && <span className="rec-row__status" role="status">{status}</span>}
            </div>
        </header>
    );
}

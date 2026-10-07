// Notebooks in the library: the "Notebooks: All · BIO 101 · …" filter
// chips, and a recording's editable Type and Notebook fields
// (src-tauri/src/notebooks.rs, recording_kind.rs).

import { useEffect, useId, useState } from "react";
import * as tauri from "../lib/tauri";
import { canonicalNotebook } from "../lib/recordPlan";
import {
    NOTEBOOKS_LABEL,
    NOTEBOOK_LABEL,
    RECORDING_KINDS,
    notebookPlaceholder,
    parseKind,
    type RecordingKind,
} from "../lib/recordingKind";
import { listRecentNotebooks, setMeetingNotebook, setMeetingRecordingKind } from "../lib/timedRecording";
import { notifyRecordingKind } from "../hooks/useRecordingKind";
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

type SaveState = "idle" | "saving" | "saved" | "error";

function statusText(state: SaveState, saved: string): string {
    return state === "saving" ? "Saving…" : state === "saved" ? saved : state === "error" ? "Couldn't save" : "";
}

/**
 * "Type: [Meeting ▾]  Notebook: [Acme project]" on a recording. The type
 * saves when picked (new notes and the Review guide use it); the notebook
 * saves on Enter or when the field loses focus.
 */
export function RecordingTypeAndNotebook({ meetingId, onSaved }: { meetingId: string; onSaved?: () => void }) {
    const [kind, setKind] = useState<RecordingKind>("meeting");
    const [kindState, setKindState] = useState<SaveState>("idle");
    const [saved, setSaved] = useState<string | null>(null);
    const [draft, setDraft] = useState("");
    const [state, setState] = useState<SaveState>("idle");
    const recents = useRecentNotebooks(meetingId);
    const listId = useId();

    useEffect(() => {
        let live = true;
        setState("idle");
        setKindState("idle");
        tauri.getMeeting(meetingId)
            .then((m) => {
                if (!live) return;
                setSaved(m?.class_name ?? null);
                setDraft(m?.class_name ?? "");
                setKind(parseKind(m?.recording_kind));
            })
            .catch(() => {});
        return () => { live = false; };
    }, [meetingId]);

    const changeKind = async (next: RecordingKind) => {
        if (next === kind) return;
        const before = kind;
        setKind(next);
        setKindState("saving");
        try {
            const stored = parseKind(await setMeetingRecordingKind(meetingId, next));
            setKind(stored);
            notifyRecordingKind(meetingId, stored);
            setKindState("saved");
            onSaved?.();
        } catch {
            setKind(before);
            setKindState("error");
        }
    };

    const commit = async () => {
        const next = canonicalNotebook(draft, recents);
        if ((next ?? null) === (saved ?? null)) {
            setDraft(next ?? "");
            return;
        }
        setState("saving");
        try {
            const stored = await setMeetingNotebook(meetingId, next);
            setSaved(stored);
            setDraft(stored ?? "");
            setState("saved");
            onSaved?.();
        } catch {
            setState("error");
        }
    };

    return (
        <div className="rec-meta">
            <div className="class-field">
                <label htmlFor={`${listId}-kind`}>Type</label>
                <select
                    id={`${listId}-kind`}
                    value={kind}
                    onChange={(e) => void changeKind(parseKind(e.target.value))}
                    aria-describedby={`${listId}-kind-status`}
                >
                    {RECORDING_KINDS.map((k) => (
                        <option key={k.value} value={k.value}>{k.label}</option>
                    ))}
                </select>
                <span id={`${listId}-kind-status`} className="class-field__status" role="status">
                    {statusText(kindState, "Saved · new notes use it")}
                </span>
            </div>
            <div className="class-field">
                <label htmlFor={`${listId}-input`}>{NOTEBOOK_LABEL}</label>
                <input
                    id={`${listId}-input`}
                    value={draft}
                    list={listId}
                    placeholder={`None (${notebookPlaceholder(kind)})`}
                    maxLength={120}
                    onChange={(e) => { setDraft(e.target.value); setState("idle"); }}
                    onBlur={() => void commit()}
                    onKeyDown={(e) => {
                        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                        if (e.key === "Escape") { setDraft(saved ?? ""); (e.target as HTMLInputElement).blur(); }
                    }}
                    aria-describedby={`${listId}-status`}
                />
                <datalist id={listId}>
                    {recents.map((c) => <option key={c} value={c} />)}
                </datalist>
                <span id={`${listId}-status`} className="class-field__status" role="status">
                    {statusText(state, "Saved")}
                </span>
            </div>
        </div>
    );
}

// Classes in the library: the "All · BIO 101 · …" filter chips, and the
// editable Class field on a recording (src-tauri/src/classes.rs).

import { useEffect, useId, useState } from "react";
import * as tauri from "../lib/tauri";
import { canonicalClassName, normalizeClassName } from "../lib/recordPlan";
import { listRecentClasses, setMeetingClass } from "../lib/timedRecording";
import "./TimedRecording.css";

/** Recent classes, reloaded when `refreshKey` changes. */
export function useRecentClasses(refreshKey: unknown): string[] {
    const [classes, setClasses] = useState<string[]>([]);
    useEffect(() => {
        let live = true;
        listRecentClasses()
            .then((c) => live && setClasses(c))
            .catch(() => {});
        return () => { live = false; };
    }, [refreshKey]);
    return classes;
}

/** Filter chips; hidden until at least one recording has a class. */
export function ClassFilterChips({
    classes,
    value,
    onChange,
}: {
    classes: string[];
    value: string | null;
    onChange: (className: string | null) => void;
}) {
    if (classes.length === 0 && !value) return null;
    const shown = value && !classes.includes(value) ? [value, ...classes] : classes;
    return (
        <div className="class-filter" role="toolbar" aria-label="Filter by class">
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

/** "Class: [BIO 101]" on a recording. Saves on Enter or when the field loses focus. */
export function MeetingClassField({ meetingId, onSaved }: { meetingId: string; onSaved?: () => void }) {
    const [saved, setSaved] = useState<string | null>(null);
    const [draft, setDraft] = useState("");
    const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
    const recents = useRecentClasses(meetingId);
    const listId = useId();

    useEffect(() => {
        let live = true;
        setState("idle");
        tauri.getMeeting(meetingId)
            .then((m) => {
                if (!live) return;
                setSaved(m?.class_name ?? null);
                setDraft(m?.class_name ?? "");
            })
            .catch(() => {});
        return () => { live = false; };
    }, [meetingId]);

    const commit = async () => {
        const next = canonicalClassName(draft, recents);
        if ((next ?? null) === (saved ?? null)) {
            setDraft(next ?? "");
            return;
        }
        setState("saving");
        try {
            const stored = await setMeetingClass(meetingId, next);
            setSaved(stored);
            setDraft(stored ?? "");
            setState("saved");
            onSaved?.();
        } catch {
            setState("error");
        }
    };

    return (
        <div className="class-field">
            <label htmlFor={`${listId}-input`}>Class</label>
            <input
                id={`${listId}-input`}
                value={draft}
                list={listId}
                placeholder="Not a class"
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
                {state === "saving" ? "Saving…" : state === "saved" ? (normalizeClassName(draft) ? "Saved · notes are lecture notes" : "Saved") : state === "error" ? "Couldn't save" : ""}
            </span>
        </div>
    );
}

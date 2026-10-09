// The Record sheet: "What is it?" (Meeting · Class · Personal), "How long?"
// and an optional Notebook with recent notebooks as one-tap chips. Shown
// when a Record button is clicked (not for the menu shortcut, tray or
// command palette, which use the remembered type and length). M / C / P
// pick the type and 1–5 the length (not while typing in Notebook), Enter
// starts, Esc cancels. Logic: lib/recordPlan.ts, lib/recordingKind.ts.

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import {
    DURATION_CHOICES,
    buildStartPlan,
    notebookSuggestions,
    parseChoice,
    pickerKeyAction,
    type DurationChoice,
    type StartPlan,
} from "../lib/recordPlan";
import {
    DEFAULT_KIND,
    NOTEBOOK_LABEL,
    RECORDING_KINDS,
    notebookPlaceholder,
    parseKind,
    type RecordingKind,
} from "../lib/recordingKind";
import { getRecordPrefs } from "../lib/timedRecording";
import "./TimedRecording.css";

/** Opens the Record sheet (provided by App). */
export const RecordPickerContext = createContext<{ open: () => void }>({ open: () => {} });

export function useRecordPicker() {
    return useContext(RecordPickerContext);
}

interface RecordPickerProps {
    onCancel: () => void;
    onStart: (plan: StartPlan) => Promise<void> | void;
}

export function RecordPicker({ onCancel, onStart }: RecordPickerProps) {
    const [kind, setKind] = useState<RecordingKind>(DEFAULT_KIND);
    const [choice, setChoice] = useState<DurationChoice>("none");
    const [notebook, setNotebook] = useState("");
    const [recents, setRecents] = useState<string[]>([]);
    const [starting, setStarting] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const dialogRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        let live = true;
        getRecordPrefs()
            .then((p) => {
                if (!live) return;
                setKind(parseKind(p.defaultKind));
                setChoice(parseChoice(p.defaultDuration));
                setRecents(p.recentNotebooks);
            })
            .catch(() => { /* defaults: meeting, no limit, no recents */ });
        // Focus inside the sheet, so Enter can't also press the Record button behind it
        dialogRef.current?.focus();
        return () => { live = false; };
    }, []);

    const start = useCallback(async () => {
        if (starting) return;
        setStarting(true);
        setError(null);
        try {
            await onStart(buildStartPlan(kind, choice, notebook, recents));
        } catch (e) {
            setError(String(e));
            setStarting(false);
        }
    }, [starting, onStart, kind, choice, notebook, recents]);

    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            const action = pickerKeyAction({
                key: e.key,
                metaKey: e.metaKey,
                ctrlKey: e.ctrlKey,
                altKey: e.altKey,
                isComposing: e.isComposing,
                inTextField: e.target === inputRef.current,
            });
            if (action.type === "none") return;
            e.preventDefault();
            e.stopPropagation();
            if (action.type === "select") setChoice(action.choice);
            else if (action.type === "kind") setKind(action.kind);
            else if (action.type === "start") void start();
            else onCancel();
        };
        // Capture: the sheet owns these keys while it's open
        window.addEventListener("keydown", onKey, true);
        return () => window.removeEventListener("keydown", onKey, true);
    }, [start, onCancel]);

    const chips = notebookSuggestions(notebook, recents);

    return (
        <div className="rpick__scrim" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
            <div
                className="rpick"
                role="dialog"
                aria-modal="true"
                aria-label="New recording"
                tabIndex={-1}
                ref={dialogRef}
            >
                <h2 id="rpick-kind-title">What is it?</h2>
                <div className="rpick__kinds" role="radiogroup" aria-labelledby="rpick-kind-title">
                    {RECORDING_KINDS.map((k) => (
                        <button
                            key={k.value}
                            type="button"
                            role="radio"
                            aria-checked={kind === k.value}
                            className={`rpick__kind ${kind === k.value ? "is-on" : ""}`}
                            onClick={() => setKind(k.value)}
                        >
                            {k.label}
                            <kbd className="rpick__key" aria-hidden>{k.key.toUpperCase()}</kbd>
                        </button>
                    ))}
                </div>

                <div className="rpick__section">
                    <h2 id="rpick-length-title">How long?</h2>
                    <div className="rpick__choices" role="radiogroup" aria-labelledby="rpick-length-title">
                        {DURATION_CHOICES.map((c) => (
                            <button
                                key={c.value}
                                type="button"
                                role="radio"
                                aria-checked={choice === c.value}
                                aria-label={c.spoken}
                                className={`rpick__choice ${choice === c.value ? "is-on" : ""}`}
                                onClick={() => setChoice(c.value)}
                            >
                                <span className="rpick__num">{c.label}</span>
                                <span className="rpick__unit">{c.value === "none" ? "No limit" : "min"}</span>
                                <kbd className="rpick__key" aria-hidden>{c.key}</kbd>
                            </button>
                        ))}
                    </div>
                </div>

                <label className="rpick__label" htmlFor="rpick-notebook">
                    {NOTEBOOK_LABEL} <span className="rpick__optional">optional</span>
                </label>
                <input
                    id="rpick-notebook"
                    ref={inputRef}
                    className="rpick__input"
                    value={notebook}
                    onChange={(e) => setNotebook(e.target.value)}
                    placeholder={notebookPlaceholder(kind)}
                    maxLength={120}
                    autoComplete="off"
                    spellCheck={false}

                />
                {chips.length > 0 && (
                    <div className="rpick__chips" aria-label="Recent notebooks">
                        {chips.map((c) => (
                            <button
                                key={c}
                                type="button"
                                className={`rpick__chip ${notebook.trim().toLowerCase() === c.toLowerCase() ? "is-on" : ""}`}
                                onClick={() => setNotebook(c)}
                            >
                                {c}
                            </button>
                        ))}
                    </div>
                )}

                {error && <p className="rpick__error" role="alert">Couldn't start — {error}</p>}

                <footer className="rpick__foot">
                    <span className="rpick__keys" aria-hidden>M C P type · 1–5 length · Enter start · Esc cancel</span>
                    <button type="button" className="rpick__ghost" onClick={onCancel}>Cancel</button>
                    <button type="button" className="rpick__go" onClick={() => void start()} disabled={starting}>
                        {starting ? "Starting…" : "Start recording"}
                    </button>
                </footer>
            </div>
        </div>
    );
}

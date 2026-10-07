// "How long?" — the Record sheet. Shown when a Record button is clicked
// (not for the menu shortcut, tray or command palette, which use the
// remembered length). 1–5 picks, Enter starts, Esc cancels. Optional Class
// with recent classes as one-tap chips. Logic: lib/recordPlan.ts.

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import {
    DURATION_CHOICES,
    buildStartPlan,
    classSuggestions,
    parseChoice,
    pickerKeyAction,
    type DurationChoice,
    type StartPlan,
} from "../lib/recordPlan";
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
    const [choice, setChoice] = useState<DurationChoice>("none");
    const [className, setClassName] = useState("");
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
                setChoice(parseChoice(p.defaultDuration));
                setRecents(p.recentClasses);
            })
            .catch(() => { /* defaults: no limit, no recents */ });
        // Focus inside the sheet, so Enter can't also press the Record button behind it
        dialogRef.current?.focus();
        return () => { live = false; };
    }, []);

    const start = useCallback(async () => {
        if (starting) return;
        setStarting(true);
        setError(null);
        try {
            await onStart(buildStartPlan(choice, className, recents));
        } catch (e) {
            setError(String(e));
            setStarting(false);
        }
    }, [starting, onStart, choice, className, recents]);

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
            else if (action.type === "start") void start();
            else onCancel();
        };
        // Capture: the sheet owns these keys while it's open
        window.addEventListener("keydown", onKey, true);
        return () => window.removeEventListener("keydown", onKey, true);
    }, [start, onCancel]);

    const chips = classSuggestions(className, recents);

    return (
        <div className="rpick__scrim" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
            <div
                className="rpick"
                role="dialog"
                aria-modal="true"
                aria-labelledby="rpick-title"
                aria-describedby="rpick-sub"
                tabIndex={-1}
                ref={dialogRef}
            >
                <h2 id="rpick-title">How long?</h2>
                <p id="rpick-sub" className="rpick__sub">
                    The recording stops by itself at the end. You can add time while it runs.
                </p>

                <div className="rpick__choices" role="radiogroup" aria-label="Recording length">
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

                <label className="rpick__label" htmlFor="rpick-class">
                    Class <span className="rpick__optional">optional</span>
                </label>
                <input
                    id="rpick-class"
                    ref={inputRef}
                    className="rpick__input"
                    value={className}
                    onChange={(e) => setClassName(e.target.value)}
                    placeholder="e.g. BIO 101 — Cell Biology"
                    maxLength={120}
                    autoComplete="off"
                    spellCheck={false}
                />
                {chips.length > 0 && (
                    <div className="rpick__chips" aria-label="Recent classes">
                        {chips.map((c) => (
                            <button
                                key={c}
                                type="button"
                                className={`rpick__chip ${className.trim().toLowerCase() === c.toLowerCase() ? "is-on" : ""}`}
                                onClick={() => setClassName(c)}
                            >
                                {c}
                            </button>
                        ))}
                    </div>
                )}
                {className.trim() && (
                    <p className="rpick__hint">Notes for a class are written as lecture notes.</p>
                )}

                {error && <p className="rpick__error" role="alert">Couldn't start — {error}</p>}

                <footer className="rpick__foot">
                    <span className="rpick__keys" aria-hidden>1–5 length · Enter start · Esc cancel</span>
                    <button type="button" className="rpick__ghost" onClick={onCancel}>Cancel</button>
                    <button type="button" className="rpick__go" onClick={() => void start()} disabled={starting}>
                        {starting ? "Starting…" : "Start recording"}
                    </button>
                </footer>
            </div>
        </div>
    );
}

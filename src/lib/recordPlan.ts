// The Record sheet's pure logic: "What is it?" (type), "How long?" (timed
// recording) and the optional notebook. No Tauri, no React; tested with
// `npm test`. See docs/TIMED_RECORDING_AND_NOTEBOOKS.md.

import { kindForKey, parseKind, type RecordingKind } from "./recordingKind.ts";

/** A picker choice, in the backend's wire format (timed_recording.rs). */
export type DurationChoice = "15" | "30" | "60" | "90" | "none";

export interface ChoiceInfo {
    value: DurationChoice;
    /** Big label in the sheet */
    label: string;
    /** Accessible name */
    spoken: string;
    /** Key that picks it (1–5) */
    key: string;
}

export const DURATION_CHOICES: readonly ChoiceInfo[] = [
    { value: "15", label: "15", spoken: "15 minutes", key: "1" },
    { value: "30", label: "30", spoken: "30 minutes", key: "2" },
    { value: "60", label: "60", spoken: "60 minutes", key: "3" },
    { value: "90", label: "90", spoken: "90 minutes", key: "4" },
    { value: "none", label: "∞", spoken: "No limit", key: "5" },
];

/** What the Record sheet sends to `start_recording`. */
export interface StartPlan {
    /** "15" | "30" | "60" | "90" | "none", or whole minutes (a carried-over segment) */
    duration?: string;
    /** Missing: the remembered type */
    recordingKind?: RecordingKind;
    /** The optional notebook */
    notebook?: string | null;
    /** Save `duration` (and `recordingKind`) as the remembered choice */
    remember?: boolean;
}

/** The remembered choice from the backend; anything unknown means no limit. */
export function parseChoice(value: string | null | undefined): DurationChoice {
    const v = (value ?? "").trim().toLowerCase();
    return (DURATION_CHOICES.find((c) => c.value === v)?.value ?? "none");
}

export function choiceForKey(key: string): DurationChoice | null {
    return DURATION_CHOICES.find((c) => c.key === key)?.value ?? null;
}

export type PickerAction =
    | { type: "select"; choice: DurationChoice }
    | { type: "kind"; kind: RecordingKind }
    | { type: "start" }
    | { type: "cancel" }
    | { type: "none" };

export interface PickerKey {
    key: string;
    metaKey?: boolean;
    ctrlKey?: boolean;
    altKey?: boolean;
    /** IME composition in progress (Enter confirms the text, not the sheet) */
    isComposing?: boolean;
    /** Focus is in the Notebook field: digits and M/C/P type there instead of picking */
    inTextField?: boolean;
}

/**
 * Keyboard in the Record sheet: M / C / P pick the type and 1–5 the length
 * (not while typing in the Notebook field), Enter starts, Esc cancels.
 * Modified keys are left alone.
 */
export function pickerKeyAction(e: PickerKey): PickerAction {
    if (e.isComposing) return { type: "none" };
    if (e.key === "Escape") return { type: "cancel" };
    if (e.metaKey || e.ctrlKey || e.altKey) return { type: "none" };
    if (e.key === "Enter") return { type: "start" };
    if (!e.inTextField) {
        const choice = choiceForKey(e.key);
        if (choice) return { type: "select", choice };
        const kind = kindForKey(e.key);
        if (kind) return { type: "kind", kind };
    }
    return { type: "none" };
}

export const NOTEBOOK_MAX_LEN = 80;

/** Same rule as the backend (notebooks.rs normalize): trimmed, one space, ≤ 80 chars. */
export function normalizeNotebook(input: string | null | undefined): string | null {
    // eslint-disable-next-line no-control-regex
    const cleaned = (input ?? "").replace(/\s+/g, " ").replace(/[\u0000-\u001f\u007f]/g, "").trim();
    const capped = Array.from(cleaned).slice(0, NOTEBOOK_MAX_LEN).join("").trimEnd();
    return capped ? capped : null;
}

/** An existing notebook spelled the same ignoring case, so "bio 101" joins "BIO 101". */
export function canonicalNotebook(input: string, recents: readonly string[]): string | null {
    const name = normalizeNotebook(input);
    if (!name) return null;
    const lower = name.toLowerCase();
    return recents.find((r) => r.toLowerCase() === lower) ?? name;
}

/** Recent notebooks to offer as chips while typing: prefix matches first, then contains. */
export function notebookSuggestions(input: string, recents: readonly string[], max = 6): string[] {
    const q = (normalizeNotebook(input) ?? "").toLowerCase();
    if (!q) return recents.slice(0, max);
    const starts = recents.filter((r) => r.toLowerCase().startsWith(q));
    const contains = recents.filter((r) => !r.toLowerCase().startsWith(q) && r.toLowerCase().includes(q));
    return [...starts, ...contains].slice(0, max);
}

/** The plan the sheet sends: type and length remembered, the notebook only when one is set. */
export function buildStartPlan(
    kind: RecordingKind,
    choice: DurationChoice,
    notebookInput: string,
    recents: readonly string[],
): StartPlan {
    const notebook = canonicalNotebook(notebookInput, recents);
    return { recordingKind: kind, duration: choice, notebook, remember: true };
}

/** 754 → "12:34", 3725 → "1:02:05" */
export function formatClock(totalSeconds: number): string {
    const s = Math.max(0, Math.floor(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    const pad = (n: number) => String(n).padStart(2, "0");
    return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** Remaining seconds at `nowMs` for an ISO deadline, never negative. */
export function secondsUntil(deadlineIso: string, nowMs: number): number {
    const t = Date.parse(deadlineIso);
    if (Number.isNaN(t)) return 0;
    return Math.max(0, Math.ceil((t - nowMs) / 1000));
}

/** Capture bar text: "12:34 left" with a limit, elapsed time without. */
export function timerLabel(opts: { deadline: string | null; startedAt: string | null; nowMs: number }): string {
    if (opts.deadline) return `${formatClock(secondsUntil(opts.deadline, opts.nowMs))} left`;
    if (opts.startedAt) {
        const started = Date.parse(opts.startedAt);
        if (!Number.isNaN(started)) return formatClock((opts.nowMs - started) / 1000);
    }
    return "";
}

/**
 * "Start New Segment" (75-minute prompt): the new recording keeps the type,
 * the notebook and the time that was left (rounded up), or no limit.
 * Nothing is remembered. Without the old recording's details the type is
 * left out, so the remembered one is used.
 */
export function segmentCarryOver(
    status: { deadline: string | null } | null,
    meeting: { recording_kind?: string | null; class_name?: string | null } | null,
    nowMs: number,
): StartPlan {
    let duration = "none";
    if (status?.deadline) {
        const minutes = Math.ceil(secondsUntil(status.deadline, nowMs) / 60);
        duration = minutes > 0 ? String(minutes) : "none";
    }
    const plan: StartPlan = { duration, notebook: normalizeNotebook(meeting?.class_name ?? null), remember: false };
    if (meeting?.recording_kind) plan.recordingKind = parseKind(meeting.recording_kind);
    return plan;
}


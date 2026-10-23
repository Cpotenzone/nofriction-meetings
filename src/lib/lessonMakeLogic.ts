// Make lesson (docs/LESSONS.md §13): pure helpers for the picker and the
// result, kept free of Tauri imports so `npm test` can load them.

import type { CatalogEntry, MakeLessonFailure } from "./lessons";
import type { MakeLessonRequest } from "./navigation";

/**
 * The Make lesson panel, as the Lessons view embeds it. The panel itself
 * (components/MakeLessonPanel.tsx) uses the app's AI helpers, so the shell
 * passes it in rather than the Lessons chunk importing it (the offline guard
 * scans that chunk for remote URLs; the AI and legal-link modules carry some).
 */
export interface MakeLessonPanelProps {
    /** From Recordings → Make lesson: preselect it and start */
    request: MakeLessonRequest | null;
    onOpenSet: (setId: string, notice: string | null) => void;
    onBack: () => void;
}

/** The recording fields the picker needs (a subset of tauri.ts `Meeting`). */
export interface PickableRecording {
    id: string;
    title: string;
    started_at: string;
    duration_seconds: number | null;
    recording_kind?: string;
}

/** Make lesson works on Class recordings only (§13, OQ-2). */
export function isClassRecording(m: { recording_kind?: string | null }): boolean {
    return (m.recording_kind ?? "").toLowerCase() === "class";
}

/** The Class recordings, newest first. */
export function classRecordings<T extends PickableRecording>(meetings: T[]): T[] {
    return meetings.filter(isClassRecording).sort((a, b) => b.started_at.localeCompare(a.started_at));
}

/** The lesson already made from a recording, if any. */
export function madeLessonFor(catalog: CatalogEntry[] | null, meetingId: string | null): CatalogEntry | null {
    if (!catalog || !meetingId) return null;
    return catalog.find((c) => c.provenance === "generated" && c.sourceRecordingId === meetingId) ?? null;
}

const PART_LABELS: Record<string, string> = {
    concepts: "concepts",
    outline: "lesson plan",
    explanations: "explanations",
    checkpoint: "checkpoint",
    worked_examples: "worked examples",
    mistakes: "common mistakes",
    exam: "final exam questions",
};

/** One line naming what a lesson was made without, or null when nothing failed. */
export function failedPartsSummary(failed: MakeLessonFailure[]): string | null {
    if (failed.length === 0) return null;
    const left: string[] = [];
    const parts: string[] = [];
    for (const f of failed) {
        const label = PART_LABELS[f.part] ?? f.part;
        if (f.lesson && (f.part === "explanations" || f.part === "checkpoint")) {
            left.push(`lesson ${f.lesson} (no ${label})`);
        } else {
            parts.push(f.lesson ? `the ${label} for lesson ${f.lesson}` : `the ${label}`);
        }
    }
    const bits: string[] = [];
    if (left.length) bits.push(`Left out ${left.join(", ")}`);
    if (parts.length) bits.push(`made without ${parts.join(", ")}`);
    let s = bits.join("; ");
    s = s.charAt(0).toUpperCase() + s.slice(1);
    if (failed.some((f) => f.part === "exam")) s += " (the exam uses the checkpoint questions)";
    return `${s}. Make it again to try those parts once more, or choose a larger model in Settings → AI.`;
}

/** The parts of a Make lesson run the panel decides on (lib/lessonMake.ts). */
export interface MakeLessonJobState {
    meetingId: string;
    status: "running" | "done" | "failed";
    error?: unknown;
}

/**
 * Whether a finished or failed run no longer applies and should be forgotten
 * (so its error doesn't come back): another recording is chosen, or the run
 * failed only because AI wasn't set up and now it is. A running job is never
 * stale. `isNoProvider` is `isNoProviderError` (lib/ai), passed in to keep
 * this file free of Tauri imports.
 */
export function isStaleMakeLessonJob(
    job: MakeLessonJobState | null,
    selected: string | null,
    aiConfigured: boolean | null,
    isNoProvider: (e: unknown) => boolean,
): boolean {
    if (!job || job.status === "running") return false;
    if (selected !== null && selected !== job.meetingId) return true;
    return job.status === "failed" && aiConfigured === true && isNoProvider(job.error);
}

/** "48 min", "1 h 05 min", or "" when unknown. */
export function durationLabel(seconds: number | null | undefined): string {
    if (!seconds || seconds <= 0) return "";
    const m = Math.round(seconds / 60);
    if (m < 60) return `${Math.max(1, m)} min`;
    return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")} min`;
}

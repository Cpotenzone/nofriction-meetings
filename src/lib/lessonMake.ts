// Make lesson (docs/LESSONS.md §13): the run's state, kept outside any
// component so it survives leaving the Lessons view. One run at a time in the
// window (the backend also allows one per recording).
//
// The Lessons surface (library, course) reads this store, so it imports
// nothing that carries a remote URL: it is bundled into the Lessons chunk,
// which the offline guard scans (scripts/check-lessons-offline.mjs). The call
// itself is passed in by the Make lesson panel (components/MakeLessonPanel),
// which wraps it in withAiConsent(…, "lessons") for the paywall and consent.

import { listen } from "@tauri-apps/api/event";
import { LESSONS_PROGRESS_EVENT, type MakeLessonProgress, type MakeLessonResult } from "./lessons";

export interface MakeLessonJob {
    meetingId: string;
    /** The recording's title, for the progress line */
    title: string;
    status: "running" | "done" | "failed";
    progress: MakeLessonProgress | null;
    result?: MakeLessonResult;
    error?: unknown;
    /** A finished lesson the Lessons view hasn't opened yet */
    unopened?: boolean;
}

let job: MakeLessonJob | null = null;
const listeners = new Set<(j: MakeLessonJob | null) => void>();

function set(next: MakeLessonJob | null) {
    job = next;
    listeners.forEach((l) => l(job));
}

export function currentMakeLessonJob(): MakeLessonJob | null {
    return job;
}

export function onMakeLessonJob(fn: (j: MakeLessonJob | null) => void): () => void {
    listeners.add(fn);
    return () => {
        listeners.delete(fn);
    };
}

/**
 * Make a lesson from `meetingId` with `run` (the backend call); no-op while
 * one is running. Progress comes from the backend's `lessons_progress` events.
 */
export async function startMakeLesson(meetingId: string, title: string, run: () => Promise<MakeLessonResult>): Promise<void> {
    if (job?.status === "running") return;
    set({ meetingId, title, status: "running", progress: null });
    let off: (() => void) | null = null;
    try {
        off = await listen<MakeLessonProgress>(LESSONS_PROGRESS_EVENT, (e) => {
            if (job?.status === "running" && e.payload.meeting_id === meetingId) set({ ...job, progress: e.payload });
        });
    } catch {
        // No backend (browser preview): the call below fails and says so
    }
    try {
        const result = await run();
        set({ meetingId, title, status: "done", progress: null, result, unopened: true });
    } catch (e) {
        set({ meetingId, title, status: "failed", progress: null, error: e });
    } finally {
        off?.();
    }
}

/** The finished lesson was opened (or the result was seen). */
export function markMakeLessonOpened(): void {
    if (job && job.unopened) set({ ...job, unopened: false });
}

/** Forget a finished or failed run (a new one can start). */
export function clearMakeLessonJob(): void {
    if (job?.status !== "running") set(null);
}

// noFriction - in-app navigation requests
//
// Any component can ask the shell to open Settings at a section (e.g. the
// "Set up AI" notices open Settings → AI), open Help, or focus the one
// search field, without prop drilling. The shell listens.

export type SettingsCategory = "recording" | "transcription" | "ai" | "subscription" | "about";

const OPEN_SETTINGS_EVENT = "nf:open-settings";

export function openSettings(category: SettingsCategory = "recording"): void {
    window.dispatchEvent(new CustomEvent<SettingsCategory>(OPEN_SETTINGS_EVENT, { detail: category }));
}

export function onOpenSettings(fn: (category: SettingsCategory) => void): () => void {
    const handler = (e: Event) => fn((e as CustomEvent<SettingsCategory>).detail ?? "recording");
    window.addEventListener(OPEN_SETTINGS_EVENT, handler);
    return () => window.removeEventListener(OPEN_SETTINGS_EVENT, handler);
}

// ── Help (one document: docs/USER_GUIDE.md) ─────────────────────────────

const OPEN_HELP_EVENT = "nf:open-help";

export function openHelp(): void {
    window.dispatchEvent(new Event(OPEN_HELP_EVENT));
}

export function onOpenHelp(fn: () => void): () => void {
    window.addEventListener(OPEN_HELP_EVENT, fn);
    return () => window.removeEventListener(OPEN_HELP_EVENT, fn);
}

// ── The one search field (⌘K) ───────────────────────────────────────────
//
// ⌘K and View → Search Recordings open Recordings and put the cursor in
// the search field at the top of the list. The field may not be mounted
// yet, so the request is kept until the view takes it.

const SEARCH_FOCUS_EVENT = "nf:focus-search";
let pendingSearchFocus = false;

export function requestSearchFocus(): void {
    pendingSearchFocus = true;
    window.dispatchEvent(new Event(SEARCH_FOCUS_EVENT));
}

/** Consume a pending focus request (true once). */
export function takeSearchFocus(): boolean {
    const r = pendingSearchFocus;
    pendingSearchFocus = false;
    return r;
}

export function onSearchFocus(fn: () => void): () => void {
    window.addEventListener(SEARCH_FOCUS_EVENT, fn);
    return () => window.removeEventListener(SEARCH_FOCUS_EVENT, fn);
}

// ── Open a recording at a moment ────────────────────────────────────────
//
// Chat's citation chips and search results open the recording in Rewind at
// the quoted time. The Recordings view may not be mounted when the request
// is made, so it is kept until the view takes it (`takeRecordingSeek`), and
// also announced for a view that is already open.

export interface RecordingSeekRequest {
    meetingId: string;
    ms: number;
    /** Makes each request new, so the same moment can be asked for again */
    n: number;
}

const SEEK_EVENT = "nf:open-recording-at";
let pendingSeek: RecordingSeekRequest | null = null;

export function requestRecordingSeek(meetingId: string, ms: number): void {
    pendingSeek = { meetingId, ms: Math.max(0, Math.round(ms)), n: Date.now() };
    window.dispatchEvent(new CustomEvent<RecordingSeekRequest>(SEEK_EVENT, { detail: pendingSeek }));
}

/** The pending request for `meetingId` (consumed), or null. */
export function takeRecordingSeek(meetingId: string | null): RecordingSeekRequest | null {
    if (!pendingSeek || !meetingId || pendingSeek.meetingId !== meetingId) return null;
    const r = pendingSeek;
    pendingSeek = null;
    return r;
}

export function onRecordingSeek(fn: (req: RecordingSeekRequest) => void): () => void {
    const handler = (e: Event) => {
        const d = (e as CustomEvent<RecordingSeekRequest>).detail;
        if (d) fn(d);
    };
    window.addEventListener(SEEK_EVENT, handler);
    return () => window.removeEventListener(SEEK_EVENT, handler);
}

/** Key in localStorage that marks the first-run setup as done. */
export const SETUP_COMPLETE_KEY = "nofriction_setup_complete";

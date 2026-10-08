// noFriction Meetings - in-app navigation requests
//
// Any component can ask the shell to open Settings at a category (e.g. the
// "Add an AI key" notices open Settings → AI Engine) without prop drilling.
// AgencyLayout listens and opens the Settings overlay.

export type SettingsCategory = "general" | "transcription" | "obsidian" | "ai" | "subscription" | "data" | "about";

const OPEN_SETTINGS_EVENT = "nf:open-settings";

export function openSettings(category: SettingsCategory = "general"): void {
    window.dispatchEvent(new CustomEvent<SettingsCategory>(OPEN_SETTINGS_EVENT, { detail: category }));
}

export function onOpenSettings(fn: (category: SettingsCategory) => void): () => void {
    const handler = (e: Event) => fn((e as CustomEvent<SettingsCategory>).detail ?? "general");
    window.addEventListener(OPEN_SETTINGS_EVENT, handler);
    return () => window.removeEventListener(OPEN_SETTINGS_EVENT, handler);
}

// ── Open a recording at a moment ────────────────────────────────────────
//
// CHAT's citation chips open the recording in REWIND at the quoted time.
// The REWIND view may not be mounted when the chip is clicked, so the
// request is kept until the view takes it (`takeRecordingSeek`), and also
// announced for a view that is already open.

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

/** Show the setup assistant again (Settings → General). Nothing is reset. */
export function rerunSetupAssistant(): void {
    try {
        localStorage.removeItem(SETUP_COMPLETE_KEY);
    } catch {
        /* storage unavailable: the reload still shows the app */
    }
    window.location.reload();
}

// noFriction Meetings - the home screen's privacy promise
//
// "Recording and transcription work offline. Nothing leaves this Mac unless
// you want it to." Both halves must stay true:
// - Recording and Whisper transcription run on this Mac. Transcription needs
//   a model, downloaded once (Settings → Transcription); until then only
//   recording is offline, and the copy says so instead of overclaiming.
// - Nothing is sent anywhere on its own. Content leaves only through
//   something the user set up or did: AI through a server they entered,
//   exporting, sharing, copying.

export interface PromiseCopy {
    /** First line of the home screen promise */
    headline: string;
    /** Second line */
    detail: string;
    /** One-line badge while a transcript fills the home screen */
    pill: string;
    /** Hover text: what "unless you want it to" means */
    explain: string;
    /** No transcription model yet: offer the one-time download */
    needsModel: boolean;
}

/**
 * The promise for the current state. `offlineReady` is whether a
 * transcription model is installed (`null` while unknown: claim only what
 * is true either way).
 */
export function promiseCopy(offlineReady: boolean | null, device = "this Mac"): PromiseCopy {
    const detail = `Nothing leaves ${device} unless you want it to.`;
    const explain =
        `Recordings, transcripts and screen captures stay on ${device}. ` +
        "Something leaves only when you choose: AI through a server you set up, or exporting and sharing.";
    if (offlineReady) {
        return {
            headline: "Recording and transcription work offline.",
            detail,
            pill: `Works offline · Nothing leaves ${device} unless you want it to`,
            explain,
            needsModel: false,
        };
    }
    return {
        headline: offlineReady === false
            ? "Recording works offline. Download a transcription model once and transcription will too."
            : "Recording works offline.",
        detail,
        pill: `Nothing leaves ${device} unless you want it to`,
        explain,
        needsModel: offlineReady === false,
    };
}

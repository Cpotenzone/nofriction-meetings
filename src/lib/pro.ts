// noFriction Meetings - what's free, what's Pro, and the paywall's words.
//
// Source of truth: docs/PRO.md (owner decision 2026-10-10). Keep this file
// pure (no Tauri imports) so `npm test` can load it.
//
// Feature keys are shared with the Rust gate (`entitlement::ProFeature::key`,
// error `PRO_REQUIRED:<key>: …`) and with iOS (`ProFeature.swift`).

export type ProFeature =
    | "ai"
    | "notes"
    | "follow_up"
    | "review_guide"
    | "chat"
    | "topics"
    | "sync"
    | "transcribe_playing"
    | "obsidian";

export const PRO_FEATURE_KEYS: readonly ProFeature[] = [
    "ai",
    "notes",
    "follow_up",
    "review_guide",
    "chat",
    "topics",
    "sync",
    "transcribe_playing",
    "obsidian",
];

export function isProFeature(k: string): k is ProFeature {
    return (PRO_FEATURE_KEYS as readonly string[]).includes(k);
}

/** Paywall title when a feature opened it. */
const HEADLINES: Record<ProFeature, string> = {
    ai: "AI features are part of noFriction Pro",
    notes: "Notes are part of noFriction Pro",
    follow_up: "Follow-up email is part of noFriction Pro",
    review_guide: "Review guides are part of noFriction Pro",
    chat: "Chat is part of noFriction Pro",
    topics: "Topics are part of noFriction Pro",
    sync: "Sync is part of noFriction Pro",
    transcribe_playing: "Transcribe what's playing is part of noFriction Pro",
    obsidian: "Export to Obsidian is part of noFriction Pro",
};

/** Title of the paywall: names the feature that opened it, if any. */
export function paywallHeadline(feature?: ProFeature | null): string {
    return feature ? HEADLINES[feature] : "noFriction Pro";
}

/** The one line under the title. */
export const PRO_VALUE =
    "Turn every recording into notes, a review guide and answers, and keep your iPhone and Mac in sync.";

export interface ProGroup {
    /** Group id; `groupFor()` maps a feature to it so the paywall can highlight it. */
    id: "study" | "chat" | "sync" | "playing" | "obsidian";
    title: string;
    detail: string;
}

/** What Pro adds, grouped (Mac wording). */
export const PRO_GROUPS: readonly ProGroup[] = [
    {
        id: "study",
        title: "Notes and review",
        detail: "Notes in each recording's style, follow-up emails, topics, and review guides with flashcards and a practice quiz.",
    },
    {
        id: "chat",
        title: "Chat",
        detail: "Ask all your recordings, a notebook or one recording. Answers cite the moment.",
    },
    {
        id: "sync",
        title: "Sync with your iPhone",
        detail: "Recordings, transcripts, notes, marks and screens move between iPhone and Mac directly on your Wi-Fi. No server; pair once with a QR code.",
    },
    {
        id: "playing",
        title: "Transcribe what's playing",
        detail: "On iPhone, while you capture the screen, noFriction also transcribes the video or call you're watching.",
    },
    {
        id: "obsidian",
        title: "Export to Obsidian",
        detail: "Each recording is saved as Markdown in your vault when it stops.",
    },
];

/** The group a feature belongs to. */
export function groupFor(feature?: ProFeature | null): ProGroup["id"] | null {
    switch (feature) {
        case "notes":
        case "follow_up":
        case "review_guide":
        case "topics":
        case "ai":
            return "study";
        case "chat":
            return "chat";
        case "sync":
            return "sync";
        case "transcribe_playing":
            return "playing";
        case "obsidian":
            return "obsidian";
        default:
            return null;
    }
}

/** What stays free, in one line. */
export const FREE_SUMMARY =
    "Always free: recording on iPhone, iPad, Mac and Apple Watch, on-device microphone transcription, screens, marks, notebooks, Rewind, search, Links, calendar and people, Delete and Strike, sharing and JSON export.";

/** Shown with the plans: Pro is the app's features, not an AI service. */
export const PRO_AI_NOTE =
    "AI runs on Apple's on-device model or the endpoint you set up. Pro doesn't include an AI service.";

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * The Pro feature a backend error asks for, or null when it isn't a Pro
 * error. `PRO_REQUIRED:<key>: …` names the feature; the AI gate's plain
 * `PRO_REQUIRED: …` means "ai".
 */
export function proFeatureFromError(e: unknown): ProFeature | null {
    const s = errText(e);
    if (!s.includes("PRO_REQUIRED")) return null;
    const m = /PRO_REQUIRED:([a-z_]+):/.exec(s);
    return m && isProFeature(m[1]) ? m[1] : "ai";
}

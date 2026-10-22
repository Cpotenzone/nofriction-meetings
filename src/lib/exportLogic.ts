// Export a recording as a package (docs/EXPORT.md): what the Export sheet
// decides without the backend. Pure (no Tauri, no React), tested with
// `npm test`. The backend builds the files (src-tauri/src/export_package).

export type ItemSource = "screen" | "photo";

/** A screen or photo the package can hold. */
export interface DraftItem {
    id: string;
    /** ms from the recording's start */
    ms: number;
    source: ItemSource;
    /** "Keynote — Q3 plan", when the Mac knows the app and window */
    label: string | null;
}

export interface DraftNotes {
    generated_at: string;
    stale: boolean;
    summary: string | null;
}

/** What a recording can put in a package (`get_export_draft`). */
export interface ExportDraft {
    meeting_id: string;
    title: string;
    kind: "meeting" | "class" | "personal" | string;
    when: string;
    duration: string | null;
    folder: string;
    notes: DraftNotes | null;
    guide_parts: string[];
    marks: number;
    links: number;
    transcript_lines: number;
    /** What the transcript reads like now; AI parts carry it as their basis */
    fingerprint: string;
    items: DraftItem[];
}

export interface Highlight {
    ms: number;
    label: string;
    quote: string | null;
}

export interface Include {
    notes: boolean;
    highlights: boolean;
    screens: boolean;
    marks: boolean;
    links: boolean;
    email: boolean;
    guide: boolean;
    transcript: boolean;
}

export type IncludeKey = keyof Include;

/** How many pictures the sheet picks for you, and the AI picks at most. */
export const DEFAULT_PICKS = 12;
export const AI_PICKS = 8;

/** "12:03", or "1:02:03" past an hour (same as the package). */
export function clock(ms: number): string {
    const s = Math.floor(Math.max(0, ms) / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = String(s % 60).padStart(2, "0");
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** Evenly spaced indexes into `n` items, at most `k`, first and last kept. */
export function sampleIndexes(n: number, k: number): number[] {
    if (n <= k) return Array.from({ length: n }, (_, i) => i);
    if (k <= 1) return [0];
    const out: number[] = [];
    for (let i = 0; i < k; i++) {
        const v = Math.floor((i * (n - 1)) / (k - 1));
        if (out[out.length - 1] !== v) out.push(v);
    }
    return out;
}

/**
 * What's picked when the sheet opens: every photo (someone took it on
 * purpose), then screens spread over the recording, up to `k` in all.
 */
export function defaultSelection(items: readonly DraftItem[], k = DEFAULT_PICKS): string[] {
    const photos = items.filter((i) => i.source === "photo");
    const screens = items.filter((i) => i.source === "screen");
    const picked = photos.slice(0, k);
    const room = k - picked.length;
    if (room > 0) picked.push(...sampleIndexes(screens.length, room).map((i) => screens[i]));
    return orderByTime(items, new Set(picked.map((i) => i.id)));
}

/** The picked ids in time order (the order the package shows them). */
export function orderByTime(items: readonly DraftItem[], picked: ReadonlySet<string>): string[] {
    return [...items]
        .filter((i) => picked.has(i.id))
        .sort((a, b) => a.ms - b.ms || a.id.localeCompare(b.id))
        .map((i) => i.id);
}

/** What's ticked when the sheet opens: everything that exists. */
export function defaultInclude(d: ExportDraft): Include {
    return {
        notes: d.notes !== null,
        highlights: true,
        screens: d.items.length > 0,
        marks: d.marks > 0,
        links: d.links > 0,
        email: true,
        guide: d.guide_parts.length > 0,
        transcript: d.transcript_lines > 0,
    };
}

/** "Screens", "Photos" or "Screens and photos", for what's there. */
export function picturesNoun(items: readonly DraftItem[]): string {
    const photos = items.filter((i) => i.source === "photo").length;
    if (photos === 0) return "Screens";
    if (photos === items.length) return "Photos";
    return "Screens and photos";
}

function count(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
}

/**
 * One line saying what the package will hold, e.g. "Notes, 3 highlights,
 * 8 screens, marks, transcript". Only parts that are ticked and exist.
 */
export function contentsLine(
    d: ExportDraft,
    inc: Include,
    parts: { picked: readonly DraftItem[]; highlights: number; hasEmail: boolean },
): string {
    const out: string[] = [];
    if (inc.notes && d.notes) out.push("notes");
    if (inc.highlights && parts.highlights > 0) out.push(count(parts.highlights, "highlight", "highlights"));
    if (inc.screens && parts.picked.length > 0) {
        const photos = parts.picked.filter((i) => i.source === "photo").length;
        const screens = parts.picked.length - photos;
        if (screens) out.push(count(screens, "screen", "screens"));
        if (photos) out.push(count(photos, "photo", "photos"));
    }
    if (inc.marks && d.marks > 0) out.push(count(d.marks, "mark", "marks"));
    if (inc.links && d.links > 0) out.push(count(d.links, "link", "links"));
    if (inc.email && parts.hasEmail) out.push("follow-up email");
    if (inc.guide && d.guide_parts.length > 0) out.push(d.kind === "class" ? "study guide" : "review guide");
    if (inc.transcript && d.transcript_lines > 0) out.push("transcript");
    if (out.length === 0) return "Just the title and date";
    const s = out.join(", ");
    return s.charAt(0).toUpperCase() + s.slice(1);
}

/** The picked pictures that still have no caption (for Write captions). */
export function uncaptioned(picked: readonly string[], captions: Readonly<Record<string, string>>): string[] {
    return picked.filter((id) => !(captions[id] ?? "").trim());
}

/** Whether any part of the package as it stands was written by AI. */
export function hasAiParts(
    d: ExportDraft,
    inc: Include,
    parts: { highlights: number; captions: number; hasEmail: boolean },
): boolean {
    return (
        (inc.notes && d.notes !== null) ||
        (inc.highlights && parts.highlights > 0) ||
        (inc.screens && parts.captions > 0) ||
        (inc.email && parts.hasEmail) ||
        (inc.guide && d.guide_parts.length > 0)
    );
}

/** The last part of a saved path, for "Saved Weekly sync.zip". */
export function baseName(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
}

/** The backend refused AI parts made from a transcript edited since. */
export function isTranscriptChanged(e: unknown): boolean {
    return String(e).includes("TRANSCRIPT_CHANGED:");
}

/** The error text without its machine prefix. */
export function plainError(e: unknown): string {
    return String(e)
        .replace(/^Error:\s*/, "")
        .replace(/^.*?TRANSCRIPT_CHANGED:\s*/, "");
}

/**
 * AI captions merged in without touching what the user typed: only ids
 * whose caption is still empty get one. Returns the new captions and the
 * ids now captioned by the AI.
 */
export function mergeCaptions(
    current: Readonly<Record<string, string>>,
    aiIds: ReadonlySet<string>,
    found: readonly { id: string; caption: string }[],
): { captions: Record<string, string>; aiIds: Set<string> } {
    const captions = { ...current };
    const ids = new Set(aiIds);
    for (const c of found) {
        if ((captions[c.id] ?? "").trim()) continue;
        captions[c.id] = c.caption;
        ids.add(c.id);
    }
    return { captions, aiIds: ids };
}

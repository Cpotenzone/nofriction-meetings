// Sync with your iPhone (docs/SYNC.md): pure helpers for Settings → Sync and
// for notes that came from the iPhone. No Tauri imports, so `npm test` loads it.

/** `meeting_notes.model_used` for notes made on the iPhone (Markdown in `summary`). */
export const SYNCED_NOTES_MODEL = "synced-markdown";

export interface SyncedNotesSection {
    heading: string | null;
    paragraphs: string[];
    bullets: string[];
}

const BOLD_LINE = /^\*\*(.+?)\*\*:?\s*$/;
const HEADING = /^#{1,6}\s+(.+)$/;
const BULLET = /^(?:[•\-*]|\d+[.)])\s+(.+)$/;

/** Inline `**bold**` markers stripped (the notes view shows plain text). */
export function plainInline(text: string): string {
    return text.replace(/\*\*(.+?)\*\*/g, "$1").replace(/__(.+?)__/g, "$1");
}

/**
 * The iPhone's notes ("**Summary**", "• item" lines, or plain Markdown
 * headings and lists) as sections for the Notes view.
 */
export function syncedNotesSections(md: string): SyncedNotesSection[] {
    const out: SyncedNotesSection[] = [];
    let cur: SyncedNotesSection | null = null;
    const section = (): SyncedNotesSection => {
        if (!cur) {
            cur = { heading: null, paragraphs: [], bullets: [] };
            out.push(cur);
        }
        return cur;
    };
    for (const raw of md.split(/\r?\n/)) {
        const line = raw.trim();
        if (!line) continue;
        const h = line.match(BOLD_LINE) ?? line.match(HEADING);
        if (h) {
            cur = { heading: plainInline(h[1].trim()), paragraphs: [], bullets: [] };
            out.push(cur);
            continue;
        }
        const b = line.match(BULLET);
        if (b) section().bullets.push(plainInline(b[1].trim()));
        else section().paragraphs.push(plainInline(line));
    }
    return out;
}

/** "Last synced 3 minutes ago" style line for Settings → Sync. */
export function lastSyncedLabel(iso: string | null | undefined, now: Date = new Date()): string {
    if (!iso) return "Not synced yet";
    const t = new Date(iso).getTime();
    if (Number.isNaN(t)) return "Not synced yet";
    const mins = Math.floor((now.getTime() - t) / 60000);
    if (mins < 1) return "Last synced just now";
    if (mins < 60) return `Last synced ${mins} minute${mins === 1 ? "" : "s"} ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `Last synced ${hours} hour${hours === 1 ? "" : "s"} ago`;
    return `Last synced ${new Date(t).toLocaleDateString([], { month: "short", day: "numeric" })}`;
}

/** Seconds left on a pairing code as "4:59". */
export function countdown(secondsLeft: number): string {
    const s = Math.max(0, Math.floor(secondsLeft));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
